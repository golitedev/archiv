//! Headless, bounded read-only browsing with cancellation and stale-result rejection.

use archiv_core::{Entry, EntryId, EntryKind, Location, RelativePath, Source, SourceId, display_name, sort_entries};
use archiv_platform::{DirectoryReader, ReadRequest, ReadSummary};
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

/// Directory browser command, shared by every frontend.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    /// Register a source and browse its root. Requires an absolute physical path.
    AddSource(PathBuf),
    /// Browse within a registered source; symlink resolution is rejected by the adapter.
    Navigate(Location),
    /// Enter an observed physical directory, without launching files.
    Open(EntryId),
    /// Re-enumerate the current location.
    Refresh,
    /// Cancel queued/in-flight work cooperatively.
    Cancel,
}

/// The state of a directory enumeration, never a claim of write capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// No source has been requested.
    Idle,
    /// Work is pending or running.
    Loading,
    /// Enumeration completed, possibly with reported limits.
    Ready,
    /// Enumeration stopped; visible entries are partial.
    Cancelled,
    /// Enumeration failed with actionable context.
    Failed(String),
}

/// Visible snapshot owned by the engine.
pub struct View {
    /// Requested physical location, if any.
    pub location: Option<Location>,
    /// Incrementally delivered entries, sorted once on completion.
    pub entries: Vec<Entry>,
    /// Enumeration state.
    pub phase: Phase,
    /// Completion limits or skipped entries.
    pub summary: Option<ReadSummary>,
    revision: u64,
}
impl View {
    /// Change counter for rebuilding presentation caches only when results change.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// An action was unavailable or could not be admitted safely.
#[derive(Debug)]
pub struct EngineError(String);
impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for EngineError {}

struct Job {
    generation: u64,
    request: ReadRequest,
    cancel: Arc<AtomicBool>,
}
struct Event {
    generation: u64,
    result: Update,
}
enum Update {
    Batch(Vec<Entry>),
    Finished(Result<ReadSummary, String>),
}

/// Two bounded workers; no storage mutation or application launch capability exists.
pub struct Browser {
    sources: Vec<Source>,
    view: View,
    jobs: Option<mpsc::SyncSender<Job>>,
    events: mpsc::Receiver<Event>,
    cancel: Option<Arc<AtomicBool>>,
    generation: u64,
    last_request: u64,
    next_source: u64,
}

impl Browser {
    /// Start two read-only workers. `wake` schedules the frontend to poll after new events.
    pub fn new(reader: Arc<dyn DirectoryReader>, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, EngineError> {
        let (job_tx, job_rx) = mpsc::sync_channel::<Job>(2);
        let (event_tx, event_rx) = mpsc::sync_channel(32);
        let job_rx = Arc::new(Mutex::new(job_rx));
        for number in 0..2 {
            let jobs = Arc::clone(&job_rx);
            let events = event_tx.clone();
            let reader = Arc::clone(&reader);
            let wake = Arc::clone(&wake);
            std::thread::Builder::new()
                .name(format!("archiv-reader-{number}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let Ok(receiver) = jobs.lock() else { return };
                            receiver.recv()
                        };
                        let Ok(job) = job else { return };
                        let send = |result| {
                            let sent = events.send(Event { generation: job.generation, result }).is_ok();
                            if sent {
                                wake();
                            }
                            sent
                        };
                        // Last-resort containment for a faulty adapter. Normal filesystem failures
                        // are Result errors. No mutation exists, and the worker continues serving jobs.
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            reader.read(&job.request, &job.cancel, &mut |batch| {
                                !job.cancel.load(Ordering::Acquire) && send(Update::Batch(batch))
                            })
                        }));
                        let result = match result {
                            Ok(result) => result.map_err(|error| error.to_string()),
                            Err(_) => Err("The directory reader failed unexpectedly; refresh to retry".to_owned()),
                        };
                        if !send(Update::Finished(result)) {
                            return;
                        }
                    }
                })
                .map_err(|error| EngineError(format!("Cannot start directory worker: {error}")))?;
        }
        Ok(Self {
            sources: Vec::new(),
            view: View { location: None, entries: Vec::new(), phase: Phase::Idle, summary: None, revision: 0 },
            jobs: Some(job_tx),
            events: event_rx,
            cancel: None,
            generation: 0,
            last_request: 0,
            next_source: 1,
        })
    }

    /// Explicit sources only; startup never scans the home directory automatically.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    /// Current directory state, readable without IO.
    pub fn view(&self) -> &View {
        &self.view
    }
    /// Source metadata for a physical identity.
    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.sources.iter().find(|source| source.id == id)
    }

    /// Admit a command once. Frontends supply monotonically increasing, nonzero request IDs.
    /// Repeated/older IDs are ignored so replay cannot dispatch the same operation twice.
    pub fn execute(&mut self, request_id: u64, command: Command) -> Result<(), EngineError> {
        if request_id <= self.last_request {
            return Ok(());
        }
        self.last_request = request_id;
        match command {
            Command::AddSource(root) => {
                if !root.is_absolute() {
                    return Err(EngineError("Choose an absolute folder path".into()));
                }
                let existing = self.sources.iter().find(|source| source.root == root).map(|source| source.id);
                let id = if let Some(id) = existing {
                    id
                } else {
                    if self.sources.len() >= 32 {
                        return Err(EngineError("This session supports up to 32 sources".into()));
                    }
                    let id = SourceId(self.next_source);
                    self.next_source = self
                        .next_source
                        .checked_add(1)
                        .ok_or_else(|| EngineError("Source identity limit reached".into()))?;
                    let label = display_name(root.file_name().unwrap_or(root.as_os_str()));
                    self.sources.push(Source { id, root, label });
                    id
                };
                self.navigate(Location { source: id, path: RelativePath::default() })
            }
            Command::Navigate(location) => self.navigate(location),
            Command::Open(id) => {
                let entry =
                    self.view.entries.iter().find(|entry| entry.id == id).ok_or_else(|| {
                        EngineError("The selected entry is no longer in this view; refresh it".into())
                    })?;
                if entry.kind != EntryKind::Directory {
                    return Err(EngineError(
                        "Only real folders can be opened in this version; links are not followed".into(),
                    ));
                }
                self.navigate(Location { source: id.source, path: id.path })
            }
            Command::Refresh => {
                let location = self.view.location.clone().ok_or_else(|| EngineError("Open a source first".into()))?;
                self.navigate(location)
            }
            Command::Cancel => {
                if let Some(cancel) = &self.cancel {
                    cancel.store(true, Ordering::Release);
                }
                if self.view.phase == Phase::Loading {
                    self.view.phase = Phase::Cancelled;
                    self.bump_revision();
                }
                Ok(())
            }
        }
    }

    fn navigate(&mut self, location: Location) -> Result<(), EngineError> {
        let source = self.source(location.source).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
        let generation =
            self.generation.checked_add(1).ok_or_else(|| EngineError("Request generation limit reached".into()))?;
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            generation,
            request: ReadRequest { source, path: location.path.clone() },
            cancel: Arc::clone(&cancel),
        };
        let sender = self.jobs.as_ref().ok_or_else(|| EngineError("Directory workers have stopped".into()))?;
        sender.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                EngineError("Directory workers are busy; wait or cancel before retrying".into())
            }
            mpsc::TrySendError::Disconnected(_) => EngineError("Directory workers have stopped; restart Archiv".into()),
        })?;
        if let Some(previous) = self.cancel.replace(cancel) {
            previous.store(true, Ordering::Release);
        }
        self.generation = generation;
        self.view.location = Some(location);
        self.view.entries.clear();
        self.view.phase = Phase::Loading;
        self.view.summary = None;
        self.bump_revision();
        Ok(())
    }

    /// Consume bounded worker events without blocking. Returns true if a further poll is useful.
    pub fn poll(&mut self) -> bool {
        for _ in 0..64 {
            let Ok(event) = self.events.try_recv() else { return false };
            if event.generation != self.generation {
                continue;
            }
            if self.view.phase == Phase::Cancelled {
                continue;
            }
            match event.result {
                Update::Batch(batch) => {
                    let room = archiv_platform::MAX_ENTRIES.saturating_sub(self.view.entries.len());
                    self.view.entries.extend(batch.into_iter().take(room));
                }
                Update::Finished(result) => {
                    sort_entries(&mut self.view.entries);
                    match result {
                        Ok(summary) => {
                            self.view.phase = if summary.cancelled { Phase::Cancelled } else { Phase::Ready };
                            self.view.summary = Some(summary);
                        }
                        Err(error) => self.view.phase = Phase::Failed(error),
                    }
                }
            }
            self.bump_revision();
        }
        true
    }

    fn bump_revision(&mut self) {
        self.view.revision = self.view.revision.wrapping_add(1);
    }

    /// Indices matching an in-memory filename filter. No recursive search or filesystem reads.
    pub fn filtered_indices(&self, query: &str, show_hidden: bool) -> Vec<usize> {
        let query = query.to_lowercase();
        self.view
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let label = display_name(&entry.name);
                ((show_hidden || !label.starts_with('.')) && label.to_lowercase().contains(&query)).then_some(index)
            })
            .collect()
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
        self.jobs.take();
        // Do not join a worker potentially blocked in kernel IO. Fixed workers exit when channels
        // close; application shutdown never waits indefinitely for an unavailable filesystem.
    }
}

/// Shared within-frame admission filter for replayed immediate-mode layout.
#[derive(Default)]
pub struct FrameCommands {
    frame: Option<u64>,
    seen: HashSet<Command>,
    next: u64,
}
impl FrameCommands {
    /// Execute each equivalent command once per frame, assigning a unique engine request ID.
    pub fn dispatch(&mut self, frame: u64, command: Command, browser: &mut Browser) -> Result<(), EngineError> {
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.seen.clear();
        }
        if !self.seen.insert(command.clone()) {
            return Ok(());
        }
        self.next = self.next.checked_add(1).ok_or_else(|| EngineError("Command identity limit reached".into()))?;
        browser.execute(self.next, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiv_platform::LocalDirectoryReader;
    use std::time::{Duration, Instant};

    fn browser() -> Browser {
        Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap()
    }
    fn settle(browser: &mut Browser) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while browser.view.phase == Phase::Loading && Instant::now() < deadline {
            browser.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_ne!(browser.view.phase, Phase::Loading, "worker failed to finish");
    }

    #[test]
    fn browse_filter_and_navigation_preserve_physical_identity() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Show")).unwrap();
        std::fs::write(root.path().join("Show/episode.mkv"), b"media").unwrap();
        std::fs::write(root.path().join(".hidden"), b"hidden").unwrap();
        let mut browser = browser();
        browser.execute(1, Command::AddSource(root.path().to_owned())).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.phase, Phase::Ready);
        assert_eq!(browser.filtered_indices("show", false).len(), 1);
        assert_eq!(browser.filtered_indices("", false).len(), 1);
        assert_eq!(browser.filtered_indices("", true).len(), 2);
        let folder = browser.view.entries.iter().find(|e| e.kind == EntryKind::Directory).unwrap().id.clone();
        browser.execute(2, Command::Open(folder)).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries.len(), 1);
        assert_eq!(browser.view.entries.first().unwrap().id.path.as_path(), std::path::Path::new("Show/episode.mkv"));
        assert!(browser.execute(3, Command::Open(browser.view.entries.first().unwrap().id.clone())).is_err());
        assert_eq!(std::fs::read(root.path().join("Show/episode.mkv")).unwrap(), b"media");
    }

    #[test]
    fn initial_state_is_idle_and_replayed_commands_are_admitted_once() {
        let root = tempfile::tempdir().unwrap();
        let mut browser = browser();
        assert_eq!(browser.view.phase, Phase::Idle);
        let mut commands = FrameCommands::default();
        commands.dispatch(4, Command::AddSource(root.path().to_owned()), &mut browser).unwrap();
        commands.dispatch(4, Command::AddSource(root.path().to_owned()), &mut browser).unwrap();
        assert_eq!(browser.generation, 1);
        browser.execute(1, Command::Refresh).unwrap();
        assert_eq!(browser.generation, 1);
        settle(&mut browser);
        commands.dispatch(5, Command::Refresh, &mut browser).unwrap();
        assert_eq!(browser.generation, 2);
    }

    #[test]
    fn new_navigation_rejects_stale_results_and_reports_errors() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("old"), b"a").unwrap();
        std::fs::write(b.path().join("new"), b"b").unwrap();
        let mut browser = browser();
        browser.execute(1, Command::AddSource(a.path().to_owned())).unwrap();
        browser.execute(2, Command::AddSource(b.path().to_owned())).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries.len(), 1);
        assert_eq!(browser.view.entries.first().unwrap().name, "new");
        assert_eq!(browser.view.entries.first().unwrap().id.source, SourceId(2));
        browser.execute(3, Command::AddSource(b.path().join("missing"))).unwrap();
        settle(&mut browser);
        assert!(matches!(browser.view.phase, Phase::Failed(_)));
        assert!(browser.view.entries.is_empty());
    }

    #[test]
    fn cancelled_scan_and_obsolete_completion_cannot_replace_the_current_view() {
        let mut browser = browser();
        let (sender, receiver) = mpsc::sync_channel(4);
        browser.events = receiver;
        browser.generation = 9;
        browser.view.phase = Phase::Loading;
        sender.send(Event { generation: 8, result: Update::Finished(Err("obsolete failure".into())) }).unwrap();
        browser.poll();
        assert_eq!(browser.view.phase, Phase::Loading);
        browser.execute(1, Command::Cancel).unwrap();
        sender.send(Event { generation: 9, result: Update::Finished(Ok(ReadSummary::default())) }).unwrap();
        browser.poll();
        assert_eq!(browser.view.phase, Phase::Cancelled);
    }
}
