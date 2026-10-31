//! Headless, bounded read-only browsing with cancellation and stale-result rejection.

use archiv_core::library::{LibrarySettings, merge_directories};
use archiv_core::{Entry, EntryId, EntryKind, Location, RelativePath, Source, SourceId, display_name, sort_entries};
use archiv_platform::{DirectoryReader, ReadRequest, ReadSummary};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

/// Directory browser command, shared by every frontend.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    /// Register a source and browse its root. Requires an absolute physical path.
    AddSource(PathBuf),
    /// Browse a configured virtual collection at its root.
    BrowseCollection(u64),
    /// Navigate back within the current physical or virtual view.
    Up,
    /// Validate and atomically save explicit library settings in a worker.
    SaveLibrary(LibrarySettings),
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
    /// Virtual collection identity, absent for physical navigation.
    pub collection: Option<u64>,
    /// Every physical directory contributing to each merged row.
    pub contributors: HashMap<EntryId, Vec<EntryId>>,
    /// Per-location failures; merged results are explicitly partial when nonempty.
    pub errors: Vec<String>,
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
    requests: Vec<ReadRequest>,
    save: Option<(PathBuf, LibrarySettings)>,
    cancel: Arc<AtomicBool>,
}
struct Event {
    generation: u64,
    result: Update,
}
enum Update {
    Batch(Vec<Entry>),
    Discard(Location),
    Finished(Result<ReadSummary, String>),
    Saved(Result<LibrarySettings, String>),
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
    library: LibrarySettings,
    settings_path: Option<PathBuf>,
    settings_pending: bool,
    settings_status: Option<String>,
    locations: Vec<Location>,
    history: Vec<Vec<Location>>,
    pending: usize,
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
                        if let Some((path, settings)) = job.save {
                            let result = archiv_platform::library::save(&path, &settings).map(|()| settings);
                            if !send(Update::Saved(result)) {
                                return;
                            }
                            continue;
                        }
                        // Sources in a collection are visited sequentially by one bounded worker.
                        for request in job.requests {
                            let result = reader
                                .read(&request, &job.cancel, &mut |batch| {
                                    !job.cancel.load(Ordering::Acquire) && send(Update::Batch(batch))
                                })
                                .map_err(|error| error.to_string());
                            if result.is_err()
                                && !send(Update::Discard(Location { source: request.source.id, path: request.path }))
                            {
                                return;
                            }
                            if !send(Update::Finished(result)) {
                                return;
                            }
                        }
                    }
                })
                .map_err(|error| EngineError(format!("Cannot start directory worker: {error}")))?;
        }
        Ok(Self {
            sources: Vec::new(),
            view: View {
                location: None,
                entries: Vec::new(),
                phase: Phase::Idle,
                summary: None,
                revision: 0,
                collection: None,
                contributors: HashMap::new(),
                errors: Vec::new(),
            },
            jobs: Some(job_tx),
            events: event_rx,
            cancel: None,
            generation: 0,
            last_request: 0,
            next_source: 1,
            library: LibrarySettings::default(),
            settings_path: None,
            settings_pending: false,
            settings_status: None,
            locations: Vec::new(),
            history: Vec::new(),
            pending: 0,
        })
    }

    /// Restore validated source definitions without touching their filesystems.
    pub fn configure(&mut self, settings: LibrarySettings, path: PathBuf) -> Result<(), EngineError> {
        settings.validate().map_err(EngineError)?;
        self.next_source = settings.sources.iter().map(|s| s.id.0).max().unwrap_or(0).saturating_add(1);
        self.sources = settings.sources.clone();
        self.library = settings;
        self.settings_path = Some(path);
        Ok(())
    }
    /// Current persisted library definitions (not temporary Open folder sources).
    pub fn library(&self) -> &LibrarySettings {
        &self.library
    }
    /// Save progress or the last persistence result.
    pub fn settings_status(&self) -> Option<&str> {
        self.settings_status.as_deref()
    }
    /// Whether the bounded settings write is still in flight.
    pub fn settings_pending(&self) -> bool {
        self.settings_pending
    }
    /// Whether parent navigation is available in the current view.
    pub fn can_up(&self) -> bool {
        if self.view.collection.is_some() {
            !self.history.is_empty()
        } else {
            self.view.location.as_ref().is_some_and(|l| l.path.parent().is_some())
        }
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
            Command::SaveLibrary(settings) => {
                settings.validate().map_err(EngineError)?;
                if self.settings_pending {
                    return Err(EngineError("A settings save is already pending".into()));
                }
                let path = self
                    .settings_path
                    .clone()
                    .ok_or_else(|| EngineError("Library settings storage is unavailable".into()))?;
                let job = Job {
                    generation: 0,
                    requests: Vec::new(),
                    save: Some((path, settings)),
                    cancel: Arc::new(AtomicBool::new(false)),
                };
                self.jobs
                    .as_ref()
                    .ok_or_else(|| EngineError("Workers stopped".into()))?
                    .try_send(job)
                    .map_err(|_| EngineError("Workers busy; retry saving shortly".into()))?;
                self.settings_pending = true;
                self.settings_status = Some("Saving library settings…".into());
                Ok(())
            }
            Command::BrowseCollection(id) => {
                let members = self
                    .library
                    .collections
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| EngineError("Unknown collection".into()))?
                    .members
                    .clone();
                self.start(members, Some(id))?;
                self.history.clear();
                Ok(())
            }
            Command::Up => {
                if let Some(previous) = self.history.last().cloned().filter(|_| self.view.collection.is_some()) {
                    self.start(previous, self.view.collection)?;
                    self.history.pop();
                    Ok(())
                } else {
                    let location =
                        self.view.location.as_ref().ok_or_else(|| EngineError("Open a folder first".into()))?;
                    let path = location.path.parent().ok_or_else(|| EngineError("Already at root".into()))?;
                    self.navigate(Location { source: location.source, path })
                }
            }
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
                    self.sources.push(Source { id, root, label, nfs_remote: None });
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
                if let Some(collection) = self.view.collection {
                    let ids = self
                        .view
                        .contributors
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| EngineError("Folder contributors unavailable".into()))?;
                    let locations = ids.into_iter().map(|id| Location { source: id.source, path: id.path }).collect();
                    let previous = self.locations.clone();
                    if self.history.len() >= 256 {
                        return Err(EngineError("Navigation depth limit reached".into()));
                    }
                    self.start(locations, Some(collection))?;
                    self.history.push(previous);
                    Ok(())
                } else {
                    self.navigate(Location { source: id.source, path: id.path })
                }
            }
            Command::Refresh => {
                if self.locations.is_empty() {
                    return Err(EngineError("Open a source first".into()));
                }
                self.start(self.locations.clone(), self.view.collection)
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
        self.start(vec![location], None)?;
        self.history.clear();
        Ok(())
    }

    fn start(&mut self, locations: Vec<Location>, collection: Option<u64>) -> Result<(), EngineError> {
        let requests: Result<Vec<_>, _> = locations
            .iter()
            .map(|location| {
                let source =
                    self.source(location.source).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
                Ok(ReadRequest { source, path: location.path.clone() })
            })
            .collect();
        let generation =
            self.generation.checked_add(1).ok_or_else(|| EngineError("Request generation limit reached".into()))?;
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job { generation, requests: requests?, save: None, cancel: Arc::clone(&cancel) };
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
        self.view.location = if collection.is_none() { locations.first().cloned() } else { None };
        self.pending = locations.len();
        self.locations = locations;
        self.view.collection = collection;
        self.view.contributors.clear();
        self.view.errors.clear();
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
            if let Update::Saved(result) = event.result {
                self.settings_pending = false;
                match result {
                    Ok(settings) => {
                        // Invalidate current scans before changing source identities/roots.
                        if let Some(cancel) = &self.cancel {
                            cancel.store(true, Ordering::Release);
                        }
                        self.view.phase = Phase::Idle;
                        self.view.location = None;
                        self.view.collection = None;
                        self.view.entries.clear();
                        self.view.contributors.clear();
                        self.view.errors.clear();
                        self.view.summary = None;
                        self.locations.clear();
                        self.history.clear();
                        self.sources = settings.sources.clone();
                        self.next_source = self.sources.iter().map(|s| s.id.0).max().unwrap_or(0).saturating_add(1);
                        self.library = settings;
                        self.settings_status =
                            Some("Library settings saved. Choose a source or collection to browse.".into());
                        self.bump_revision();
                    }
                    Err(error) => self.settings_status = Some(error),
                }
                continue;
            }
            if event.generation != self.generation || self.view.phase == Phase::Idle {
                continue;
            }
            if self.view.phase == Phase::Cancelled {
                continue;
            }
            match event.result {
                Update::Batch(batch) => {
                    let room = archiv_platform::MAX_ENTRIES.saturating_sub(self.view.entries.len());
                    if batch.len() > room {
                        self.view.summary.get_or_insert_with(ReadSummary::default).truncated = true;
                    }
                    self.view.entries.extend(batch.into_iter().take(room));
                }
                Update::Discard(location) => {
                    self.view.entries.retain(|e| {
                        e.id.source != location.source || e.id.path.parent().as_ref() != Some(&location.path)
                    });
                }
                Update::Finished(result) => {
                    self.pending = self.pending.saturating_sub(1);
                    match result {
                        Ok(summary) => {
                            let total = self.view.summary.get_or_insert_with(ReadSummary::default);
                            total.entries = total.entries.saturating_add(summary.entries);
                            total.skipped = total.skipped.saturating_add(summary.skipped);
                            total.truncated |= summary.truncated;
                            total.cancelled |= summary.cancelled;
                        }
                        Err(error) => self.view.errors.push(error),
                    }
                    if self.pending == 0 {
                        sort_entries(&mut self.view.entries);
                        if self.view.collection.is_some() {
                            self.view.contributors = merge_directories(&mut self.view.entries);
                        }
                        self.view.phase = if self.view.summary.as_ref().is_some_and(|s| s.cancelled) {
                            Phase::Cancelled
                        } else if self.view.errors.is_empty() {
                            Phase::Ready
                        } else {
                            Phase::Failed(
                                "Some locations could not be read; results are incomplete. See source errors.".into(),
                            )
                        };
                    }
                }
                Update::Saved(_) => {}
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

    fn fixture_library(root: &std::path::Path) -> LibrarySettings {
        LibrarySettings {
            version: 1,
            sources: (1..=4)
                .map(|id| Source {
                    id: SourceId(id),
                    root: root.join(format!("disk{id}")),
                    label: format!("Disk {id}"),
                    nfs_remote: None,
                })
                .collect(),
            collections: vec![archiv_core::library::Collection {
                id: 1,
                label: "Movies".into(),
                members: (1..=4)
                    .map(|id| Location { source: SourceId(id), path: RelativePath::new("movies").unwrap() })
                    .collect(),
            }],
        }
    }

    #[test]
    fn four_disk_merge_is_optional_recursive_and_preserves_file_collisions() {
        let root = tempfile::tempdir().unwrap();
        for id in 1..=4 {
            std::fs::create_dir_all(root.path().join(format!("disk{id}/movies/Title/Extras"))).unwrap();
            std::fs::write(root.path().join(format!("disk{id}/movies/Title/movie.mkv")), format!("disk {id}")).unwrap();
        }
        let mut browser = browser();
        browser.configure(fixture_library(root.path()), root.path().join("library.json")).unwrap();
        assert_eq!(browser.view.phase, Phase::Idle);
        assert!(browser.view.entries.is_empty());
        browser.execute(1, Command::BrowseCollection(1)).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries.len(), 1);
        let title = browser.view.entries[0].id.clone();
        assert_eq!(browser.view.contributors[&title].len(), 4);
        assert!(browser.view.location.is_none());
        browser.execute(2, Command::Open(title)).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries.len(), 5); // one Extras folder and four separate files
        let files: HashSet<_> =
            browser.view.entries.iter().filter(|e| e.kind == EntryKind::File).map(|e| e.id.clone()).collect();
        assert_eq!(files.len(), 4);
        let extras = browser.view.entries.iter().find(|e| e.kind == EntryKind::Directory).unwrap().id.clone();
        assert_eq!(browser.view.contributors[&extras].len(), 4);
        browser.execute(3, Command::Open(extras)).unwrap();
        settle(&mut browser);
        assert!(browser.view.entries.is_empty());
        browser.execute(4, Command::Up).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries.len(), 5);
        browser.execute(5, Command::Up).unwrap();
        settle(&mut browser);
        assert!(!browser.can_up());
        browser
            .execute(
                6,
                Command::Navigate(Location { source: SourceId(1), path: RelativePath::new("movies/Title").unwrap() }),
            )
            .unwrap();
        settle(&mut browser);
        assert!(browser.view.collection.is_none());
        assert_eq!(browser.view.entries.len(), 2);
        for id in 1..=4 {
            assert_eq!(
                std::fs::read_to_string(root.path().join(format!("disk{id}/movies/Title/movie.mkv"))).unwrap(),
                format!("disk {id}")
            );
        }
    }

    #[test]
    fn unavailable_contributor_is_reported_and_refresh_recovers() {
        let root = tempfile::tempdir().unwrap();
        for id in 1..=3 {
            std::fs::create_dir_all(root.path().join(format!("disk{id}/movies/Title"))).unwrap();
        }
        let mut browser = browser();
        browser.configure(fixture_library(root.path()), root.path().join("library.json")).unwrap();
        browser.execute(1, Command::BrowseCollection(1)).unwrap();
        settle(&mut browser);
        assert!(matches!(browser.view.phase, Phase::Failed(_)));
        assert_eq!(browser.view.errors.len(), 1);
        assert_eq!(browser.view.entries.len(), 1);
        std::fs::create_dir_all(root.path().join("disk4/movies/Title")).unwrap();
        browser.execute(2, Command::Refresh).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.phase, Phase::Ready);
        assert_eq!(browser.view.contributors.values().next().unwrap().len(), 4);
    }

    #[test]
    fn settings_save_is_deduplicated_and_restart_remains_idle() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings/library.json");
        let mut browser = browser();
        browser.configure(LibrarySettings::default(), path.clone()).unwrap();
        let mut commands = FrameCommands::default();
        let settings = fixture_library(root.path());
        commands.dispatch(1, Command::SaveLibrary(settings.clone()), &mut browser).unwrap();
        commands.dispatch(1, Command::SaveLibrary(settings.clone()), &mut browser).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while browser.settings_pending() && Instant::now() < deadline {
            browser.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!browser.settings_pending());
        assert_eq!(browser.library(), &settings);
        let saved = archiv_platform::library::load(&path).unwrap();
        let mut restarted = super::tests::browser();
        restarted.configure(saved, path).unwrap();
        assert_eq!(restarted.view.phase, Phase::Idle);
        assert_eq!(restarted.sources().len(), 4);
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
