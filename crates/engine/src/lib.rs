//! Headless, bounded read-only browsing with cancellation and stale-result rejection.

use archiv_core::actions::{ProtectedPath, RecoveryPolicy};
use archiv_core::library::{LibrarySettings, merge_directories};
use archiv_core::{Entry, EntryId, EntryKind, Location, RelativePath, Source, SourceId, display_name, sort_entries};
use archiv_platform::operations::{self, RecoveryPlan, RecoveryRecord};
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
    /// Navigate an absolute address, preserving an existing source identity when possible.
    BrowsePath(PathBuf),
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
    /// Expand/collapse a directory in place, using the existing bounded workers.
    Expand(EntryId),
    /// Open an explicit regular file with its configured external application.
    Launch(EntryId),
    /// Build an immutable physical recovery preview; never mutate here.
    PreviewRecovery(EntryId),
    /// Execute one previously previewed recovery move exactly once.
    ConfirmRecovery(u64),
    /// Dismiss a preview without executing it.
    DismissRecovery,
    /// Explicitly inspect this source's recovery records.
    ListRecovery(SourceId),
    /// Restore one explicitly selected recovery job without overwriting.
    Restore(SourceId, String),
    /// Re-enumerate the current location.
    Refresh,
    /// Explicit bounded recursive name search below the currently opened locations.
    Search(String),
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
    /// Results are explicit physical search hits, not same-parent virtual rows.
    pub search: bool,
    revision: u64,
    pending: usize,
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

enum Action {
    Prepare(u64, Source, Entry, RecoveryPolicy, Vec<(Source, ProtectedPath)>),
    Recover(RecoveryPlan),
    List(Source),
    Restore(Source, RecoveryPolicy, String, Vec<(Source, ProtectedPath)>),
    Launch(Source, Entry, String),
}
enum ActionResult {
    Preview(Box<RecoveryPlan>),
    Done(String),
    Changed(String),
    Records(SourceId, Vec<RecoveryRecord>),
}
fn perform(action: Action, children: &mut Vec<operations::Application>) -> Result<ActionResult, String> {
    match action {
        Action::Prepare(id, source, entry, policy, guards) => {
            operations::prepare(id, source, entry, policy, guards).map(|plan| ActionResult::Preview(Box::new(plan)))
        }
        Action::Recover(plan) => {
            operations::recover(&plan).map(|id| ActionResult::Changed(format!("Moved to recovery: {id}")))
        }
        Action::List(source) => {
            operations::list_recovery(&source).map(|records| ActionResult::Records(source.id, records))
        }
        Action::Restore(source, policy, id, guards) => operations::restore(&source, &policy, &id, &guards)
            .map(|()| ActionResult::Changed("Restored to its original physical path".into())),
        Action::Launch(source, entry, program) => {
            let mut running = Vec::new();
            for mut child in children.drain(..) {
                if !child.poll()? {
                    running.push(child);
                }
            }
            *children = running;
            if children.len() >= 8 {
                return Err("Application limit reached; close an opened application before trying again".into());
            }
            children.push(operations::launch(&source, &entry, &program)?);
            Ok(ActionResult::Done(format!("Launched {program}")))
        }
    }
}
/// A lazily loaded inline directory; its identities remain physical even in a collection.
pub struct Branch {
    /// Cached directory results and errors.
    pub view: View,
    /// Whether rows should be visible. Collapsing never triggers another scan.
    pub expanded: bool,
}
struct Job {
    generation: u64,
    requests: Vec<ReadRequest>,
    node: Option<EntryId>,
    action: Option<Action>,
    search: Option<String>,
    save: Option<(PathBuf, LibrarySettings)>,
    cancel: Arc<AtomicBool>,
}
struct Event {
    generation: u64,
    node: Option<EntryId>,
    result: Update,
}
enum Update {
    Batch(Vec<Entry>),
    Discard(Location),
    Finished(Result<ReadSummary, String>),
    Saved(Result<LibrarySettings, String>),
    Action(Result<ActionResult, String>),
    BackgroundError(String),
}

/// Two bounded workers shared by browsing, configured application launches and opt-in recovery.
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
    branches: HashMap<EntryId, Branch>,
    operation_pending: bool,
    operation_status: Option<String>,
    recovery_preview: Option<RecoveryPlan>,
    recovery_records: Option<(SourceId, Vec<RecoveryRecord>)>,
}

fn search(
    reader: &dyn DirectoryReader,
    roots: Vec<ReadRequest>,
    query: &str,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Vec<Entry>) -> bool,
) -> Result<ReadSummary, String> {
    let query = query.to_lowercase();
    let mut queue: std::collections::VecDeque<_> = roots.into_iter().map(|r| (r, 0usize)).collect();
    let mut total = ReadSummary::default();
    let mut visited = 0usize;
    let mut matches = Vec::new();
    let mut errors = Vec::new();
    while let Some((request, depth)) = queue.pop_front() {
        if cancel.load(Ordering::Acquire) {
            total.cancelled = true;
            break;
        }
        let mut limit = false;
        let result = reader.read(&request, cancel, &mut |batch| {
            for entry in batch {
                if visited >= archiv_platform::MAX_ENTRIES {
                    limit = true;
                    return false;
                }
                visited = visited.saturating_add(1);
                if entry.kind == EntryKind::Directory {
                    if depth < 32 && queue.len() < 10000 {
                        queue.push_back((
                            ReadRequest { source: request.source.clone(), path: entry.id.path.clone() },
                            depth.saturating_add(1),
                        ));
                    } else {
                        total.truncated = true;
                    }
                }
                if display_name(&entry.name).to_lowercase().contains(&query) {
                    matches.push(entry);
                }
                if matches.len() == archiv_platform::BATCH_SIZE && !emit(std::mem::take(&mut matches)) {
                    return false;
                }
            }
            true
        });
        match result {
            Ok(summary) => {
                total.skipped = total.skipped.saturating_add(summary.skipped);
                total.truncated |= summary.truncated || limit;
            }
            Err(error) => {
                if errors.len() < 32 {
                    errors.push(error.to_string());
                }
            }
        }
        if limit {
            break;
        }
    }
    if !matches.is_empty() && !emit(matches) {
        total.cancelled = true;
    }
    total.entries = visited;
    total.cancelled |= cancel.load(Ordering::Acquire);
    if !errors.is_empty() {
        return Err(format!("Search is incomplete: {}", errors.join("; ")));
    }
    Ok(total)
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
                    let mut children: Vec<operations::Application> = Vec::new();
                    loop {
                        let mut failure = None;
                        children.retain_mut(|child| match child.poll() {
                            Ok(false) => true,
                            Ok(true) => false,
                            Err(error) => {
                                failure = Some(error);
                                false
                            }
                        });
                        if let Some(error) = failure {
                            if events
                                .send(Event { generation: 0, node: None, result: Update::BackgroundError(error) })
                                .is_err()
                            {
                                return;
                            }
                            wake();
                        }
                        let job = {
                            let Ok(receiver) = jobs.lock() else { return };
                            receiver.recv_timeout(std::time::Duration::from_millis(500))
                        };
                        let job = match job {
                            Ok(job) => job,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        };
                        let send = |result| {
                            let sent = events
                                .send(Event { generation: job.generation, node: job.node.clone(), result })
                                .is_ok();
                            if sent {
                                wake();
                            }
                            sent
                        };
                        if let Some(action) = job.action {
                            if !send(Update::Action(perform(action, &mut children))) {
                                return;
                            }
                            continue;
                        }
                        if let Some((path, settings)) = job.save {
                            let result = archiv_platform::library::save(&path, &settings).map(|()| settings);
                            if !send(Update::Saved(result)) {
                                return;
                            }
                            continue;
                        }
                        if let Some(query) = job.search {
                            let result = search(&*reader, job.requests, &query, &job.cancel, &mut |batch| {
                                send(Update::Batch(batch))
                            });
                            if !send(Update::Finished(result)) {
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
                search: false,
                revision: 0,
                pending: 0,
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
            branches: HashMap::new(),
            operation_pending: false,
            operation_status: None,
            recovery_preview: None,
            recovery_records: None,
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
    /// Any root or inline directory scan is pending.
    pub fn is_loading(&self) -> bool {
        self.view.phase == Phase::Loading || self.branches.values().any(|b| b.view.phase == Phase::Loading)
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
            Command::Search(query) => {
                if query.is_empty() || query.len() > 4096 || self.locations.is_empty() {
                    return Err(EngineError("Open a folder and enter a search query (up to 4096 bytes)".into()));
                }
                self.start_listing(self.locations.clone(), self.view.collection, Some(query))
            }
            Command::Expand(id) => self.expand(id),
            Command::DismissRecovery => {
                self.recovery_preview = None;
                Ok(())
            }
            Command::Launch(id) => {
                let entry =
                    self.entry(&id).cloned().ok_or_else(|| EngineError("Selected file is no longer visible".into()))?;
                let source = self.source(id.source).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
                let program = self
                    .library
                    .openers
                    .program(id.path.as_path())
                    .ok_or_else(|| {
                        EngineError("Configure an application for this file type in Preferences → Applications".into())
                    })?
                    .to_owned();
                self.action(Action::Launch(source, entry, program))
            }
            Command::PreviewRecovery(id) => {
                self.recovery_preview = None;
                let entry =
                    self.entry(&id).cloned().ok_or_else(|| EngineError("Selected item is no longer visible".into()))?;
                if self.view.collection.is_some() && !self.view.search && entry.kind == EntryKind::Directory {
                    return Err(EngineError(
                        "Virtual folders cannot be deleted. Browse and select an explicit physical location first."
                            .into(),
                    ));
                }
                let source = self.source(id.source).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
                let policy = self.policy(id.source)?.clone();
                policy.permits(&id.path, entry.kind).map_err(EngineError)?;
                self.action(Action::Prepare(request_id, source, entry, policy, self.guards()))
            }
            Command::ConfirmRecovery(id) => {
                let plan = self
                    .recovery_preview
                    .take()
                    .filter(|p| p.id == id)
                    .ok_or_else(|| EngineError("Recovery preview expired; prepare it again".into()))?;
                self.action(Action::Recover(plan))
            }
            Command::ListRecovery(id) => {
                let source = self.source(id).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
                self.action(Action::List(source))
            }
            Command::Restore(id, job) => {
                if !self
                    .recovery_records
                    .as_ref()
                    .is_some_and(|(source, records)| *source == id && records.iter().any(|r| r.id == job))
                {
                    return Err(EngineError("Inspect Recovery and select a current record first".into()));
                }
                let source = self.source(id).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
                let policy = self.policy(id)?.clone();
                self.action(Action::Restore(source, policy, job, self.guards()))
            }
            Command::SaveLibrary(settings) => {
                if self.operation_pending {
                    return Err(EngineError("Wait for the current operation before changing settings".into()));
                }
                self.recovery_preview = None;
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
                    node: None,
                    action: None,
                    search: None,
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
            Command::AddSource(root) => self.add_source(root),
            Command::BrowsePath(path) => {
                if !path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                    return Err(EngineError("Use an absolute address without parent traversal".into()));
                }
                if let Some(source) = self
                    .sources
                    .iter()
                    .filter(|s| path.starts_with(&s.root))
                    .max_by_key(|s| s.root.components().count())
                {
                    let relative = path.strip_prefix(&source.root).map_err(|e| EngineError(e.to_string()))?;
                    self.navigate(Location {
                        source: source.id,
                        path: RelativePath::new(relative).map_err(|e| EngineError(e.to_string()))?,
                    })
                } else {
                    self.add_source(path)
                }
            }
            Command::Navigate(location) => self.navigate(location),
            Command::Open(id) => {
                let entry = self
                    .entry(&id)
                    .ok_or_else(|| EngineError("The selected entry is no longer in this view; refresh it".into()))?;
                if entry.kind != EntryKind::Directory {
                    return Err(EngineError(
                        "Only real folders can be opened in this version; links are not followed".into(),
                    ));
                }
                if let Some(collection) = self.view.collection.filter(|_| !self.view.search) {
                    let ids = self
                        .contributors(&id)
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
                }
                for branch in self.branches.values_mut() {
                    if branch.view.phase == Phase::Loading {
                        branch.view.phase = Phase::Cancelled;
                    }
                }
                self.bump_revision();
                Ok(())
            }
        }
    }

    fn add_source(&mut self, root: PathBuf) -> Result<(), EngineError> {
        if !root.is_absolute() || root.components().any(|c| c.as_os_str() == archiv_core::actions::RECOVERY_DIR) {
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
            self.next_source =
                self.next_source.checked_add(1).ok_or_else(|| EngineError("Source identity limit reached".into()))?;
            let label = display_name(root.file_name().unwrap_or(root.as_os_str()));
            self.sources.push(Source { id, root, label, nfs_remote: None });
            id
        };
        self.navigate(Location { source: id, path: RelativePath::default() })
    }
    fn navigate(&mut self, location: Location) -> Result<(), EngineError> {
        self.start(vec![location], None)?;
        self.history.clear();
        Ok(())
    }

    fn start(&mut self, locations: Vec<Location>, collection: Option<u64>) -> Result<(), EngineError> {
        self.start_listing(locations, collection, None)
    }
    fn start_listing(
        &mut self,
        locations: Vec<Location>,
        collection: Option<u64>,
        search: Option<String>,
    ) -> Result<(), EngineError> {
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
        let job = Job {
            generation,
            requests: requests?,
            node: None,
            action: None,
            search: search.clone(),
            save: None,
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
        self.view.location = if collection.is_none() { locations.first().cloned() } else { None };
        self.view.search = search.is_some();
        self.view.pending = if search.is_some() { 1 } else { locations.len() };
        self.branches.clear();
        self.recovery_preview = None;
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
            if let Update::BackgroundError(error) = event.result {
                self.operation_status = Some(error);
                self.bump_revision();
                continue;
            }
            if let Update::Action(result) = event.result {
                self.operation_pending = false;
                match result {
                    Ok(ActionResult::Preview(plan)) if event.generation == self.generation => {
                        self.recovery_preview = Some(*plan);
                        self.operation_status = None;
                    }
                    Ok(ActionResult::Preview(_)) => {
                        self.operation_status = Some("Preview expired after navigation".into())
                    }
                    Ok(ActionResult::Changed(message)) => {
                        self.operation_status = Some(message);
                        self.recovery_records = None;
                        if !self.locations.is_empty()
                            && let Err(error) = self.start(self.locations.clone(), self.view.collection)
                        {
                            self.operation_status = Some(format!("Operation completed; refresh needed: {error}"));
                        }
                    }
                    Ok(ActionResult::Done(message)) => {
                        self.operation_status = Some(message);
                        self.recovery_records = None;
                    }
                    Ok(ActionResult::Records(source, records)) => {
                        self.recovery_records = Some((source, records));
                        self.operation_status = None;
                    }
                    Err(error) => self.operation_status = Some(error),
                }
                self.bump_revision();
                continue;
            }
            if let Update::Saved(result) = event.result {
                self.settings_pending = false;
                match result {
                    Ok(settings) => {
                        // Invalidate current scans before changing source identities/roots.
                        if let Some(cancel) = &self.cancel {
                            cancel.store(true, Ordering::Release);
                        }
                        self.branches.clear();
                        self.recovery_preview = None;
                        self.recovery_records = None;
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
            let total = self
                .view
                .entries
                .len()
                .saturating_add(self.branches.values().map(|b| b.view.entries.len()).sum::<usize>());
            let remaining = archiv_platform::MAX_ENTRIES.saturating_sub(total);
            let view = if let Some(node) = &event.node {
                let Some(branch) = self.branches.get_mut(node) else { continue };
                &mut branch.view
            } else {
                &mut self.view
            };
            if view.phase == Phase::Cancelled {
                continue;
            }
            match event.result {
                Update::Batch(batch) => {
                    let room = remaining;
                    if batch.len() > room {
                        view.summary.get_or_insert_with(ReadSummary::default).truncated = true;
                    }
                    view.entries.extend(batch.into_iter().take(room));
                }
                Update::Discard(location) => {
                    view.entries.retain(|e| {
                        e.id.source != location.source || e.id.path.parent().as_ref() != Some(&location.path)
                    });
                }
                Update::Finished(result) => {
                    view.pending = view.pending.saturating_sub(1);
                    match result {
                        Ok(summary) => {
                            let total = view.summary.get_or_insert_with(ReadSummary::default);
                            total.entries = total.entries.saturating_add(summary.entries);
                            total.skipped = total.skipped.saturating_add(summary.skipped);
                            total.truncated |= summary.truncated;
                            total.cancelled |= summary.cancelled;
                        }
                        Err(error) => view.errors.push(error),
                    }
                    if view.pending == 0 {
                        sort_entries(&mut view.entries);
                        if view.collection.is_some() && !view.search {
                            view.contributors = merge_directories(&mut view.entries);
                        }
                        view.phase = if view.summary.as_ref().is_some_and(|s| s.cancelled) {
                            Phase::Cancelled
                        } else if view.errors.is_empty() {
                            Phase::Ready
                        } else {
                            Phase::Failed(
                                "Some locations could not be read; results are incomplete. See source errors.".into(),
                            )
                        };
                    }
                }
                Update::Saved(_) | Update::Action(_) | Update::BackgroundError(_) => {}
            }
            self.bump_revision();
        }
        true
    }

    /// Lookup a visible or lazily loaded physical entry by its stable identity.
    pub fn entry(&self, id: &EntryId) -> Option<&Entry> {
        self.view.entries.iter().chain(self.branches.values().flat_map(|b| b.view.entries.iter())).find(|e| &e.id == id)
    }
    /// Every contributing directory for a virtual row.
    pub fn contributors(&self, id: &EntryId) -> Option<&Vec<EntryId>> {
        self.view.contributors.get(id).or_else(|| self.branches.values().find_map(|b| b.view.contributors.get(id)))
    }
    /// Lazily loaded inline directory, if requested.
    pub fn branch(&self, id: &EntryId) -> Option<&Branch> {
        self.branches.get(id)
    }
    /// Background operation progress and failures.
    pub fn operation_status(&self) -> Option<&str> {
        self.operation_status.as_deref()
    }
    /// A single bounded operation is in progress.
    pub fn operation_pending(&self) -> bool {
        self.operation_pending
    }
    /// Single-use recovery preview.
    pub fn recovery_preview(&self) -> Option<&RecoveryPlan> {
        self.recovery_preview.as_ref()
    }
    /// Explicitly inspected recovery records.
    pub fn recovery_records(&self) -> Option<&(SourceId, Vec<RecoveryRecord>)> {
        self.recovery_records.as_ref()
    }
    /// Shared availability explanation used by menus, shortcuts and direct engine calls.
    pub fn recovery_availability(&self, id: &EntryId) -> Result<(), String> {
        let entry = self.entry(id).ok_or("Item is no longer in this view")?;
        if self.view.collection.is_some() && !self.view.search && entry.kind == EntryKind::Directory {
            return Err("Virtual folders have no delete target. Open an explicit physical source first.".into());
        }
        self.policy(id.source).map_err(|e| e.to_string())?.permits(&id.path, entry.kind)
    }
    fn guards(&self) -> Vec<(Source, ProtectedPath)> {
        let mut guards = Vec::new();
        for source in &self.library.sources {
            guards.push((source.clone(), ProtectedPath { path: RelativePath::default(), subtree: false }));
            if let Some(policy) = self.library.recovery.iter().find(|p| p.source == source.id) {
                for protected in &policy.protected {
                    guards.push((source.clone(), protected.clone()));
                }
            }
        }
        for member in self.library.collections.iter().flat_map(|c| c.members.iter()) {
            if let Some(source) = self.source(member.source) {
                guards.push((source.clone(), ProtectedPath { path: member.path.clone(), subtree: false }));
            }
        }
        guards
    }
    fn policy(&self, source: SourceId) -> Result<&RecoveryPolicy, EngineError> {
        self.library
            .recovery
            .iter()
            .find(|p| p.source == source)
            .ok_or_else(|| EngineError("Configure recovery permissions in Preferences → Safeguards first".into()))
    }
    fn action(&mut self, action: Action) -> Result<(), EngineError> {
        if self.operation_pending || self.settings_pending {
            return Err(EngineError("Wait for the current operation/settings save".into()));
        }
        let job = Job {
            generation: self.generation,
            node: None,
            action: Some(action),
            search: None,
            requests: Vec::new(),
            save: None,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        self.jobs
            .as_ref()
            .ok_or_else(|| EngineError("Workers stopped".into()))?
            .try_send(job)
            .map_err(|_| EngineError("Workers busy; try again shortly".into()))?;
        self.operation_pending = true;
        self.operation_status = Some("Working…".into());
        Ok(())
    }
    fn expand(&mut self, id: EntryId) -> Result<(), EngineError> {
        if let Some(branch) = self.branches.get_mut(&id) {
            branch.expanded = !branch.expanded;
            self.bump_revision();
            return Ok(());
        }
        if self.branches.len() >= 32 || id.path.as_path().components().count() >= 64 {
            return Err(EngineError("Inline expansion limit reached; open the folder to continue".into()));
        }
        let entry = self.entry(&id).ok_or_else(|| EngineError("Folder is no longer visible".into()))?;
        if entry.kind != EntryKind::Directory {
            return Err(EngineError("Only folders can expand".into()));
        }
        if self.view.collection.is_some() && !self.view.search && self.contributors(&id).is_none() {
            return Err(EngineError("Wait for the collection folder listing before expanding it".into()));
        }
        let ids = self.contributors(&id).cloned().unwrap_or_else(|| vec![id.clone()]);
        let requests: Result<Vec<_>, _> = ids
            .iter()
            .map(|id| {
                Ok(ReadRequest {
                    source: self.source(id.source).cloned().ok_or_else(|| EngineError("Unknown source".into()))?,
                    path: id.path.clone(),
                })
            })
            .collect();
        if self.cancel.as_ref().is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            return Err(EngineError("Scan was cancelled; refresh before expanding more folders".into()));
        }
        let requests = requests?;
        let pending = requests.len();
        let job = Job {
            generation: self.generation,
            node: Some(id.clone()),
            action: None,
            search: None,
            requests,
            save: None,
            cancel: self.cancel.clone().unwrap_or_else(|| Arc::new(AtomicBool::new(false))),
        };
        self.jobs
            .as_ref()
            .ok_or_else(|| EngineError("Workers stopped".into()))?
            .try_send(job)
            .map_err(|_| EngineError("Workers busy; retry expansion shortly".into()))?;
        self.branches.insert(
            id,
            Branch {
                expanded: true,
                view: View {
                    location: None,
                    entries: Vec::new(),
                    collection: if self.view.search { None } else { self.view.collection },
                    contributors: HashMap::new(),
                    errors: Vec::new(),
                    phase: Phase::Loading,
                    summary: None,
                    search: false,
                    revision: 0,
                    pending,
                },
            },
        );
        self.bump_revision();
        Ok(())
    }
    /// Flatten expanded directories in display order. Search filters cached/expanded names only.
    pub fn rows(&self, query: &str, hidden: bool) -> Vec<(EntryId, usize)> {
        fn visit(
            browser: &Browser,
            entries: &[Entry],
            depth: usize,
            query: &str,
            hidden: bool,
            rows: &mut Vec<(EntryId, usize)>,
        ) {
            if depth >= 64 {
                return;
            }
            for entry in entries {
                let label = display_name(&entry.name);
                if !hidden && label.starts_with('.') {
                    continue;
                }
                let start = rows.len();
                if let Some(branch) = browser.branch(&entry.id).filter(|b| b.expanded) {
                    visit(browser, &branch.view.entries, depth.saturating_add(1), query, hidden, rows);
                }
                if label.to_lowercase().contains(query) || rows.len() > start {
                    rows.insert(start, (entry.id.clone(), depth));
                }
            }
        }
        let mut rows = Vec::new();
        visit(self, &self.view.entries, 0, &query.to_lowercase(), hidden, &mut rows);
        rows
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
            openers: Default::default(),
            recovery: Vec::new(),
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

    fn settle_all(browser: &mut Browser) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            browser.poll();
            if !browser.operation_pending()
                && !browser.settings_pending()
                && browser.view.phase != Phase::Loading
                && !browser.branches.values().any(|b| b.view.phase == Phase::Loading)
            {
                break;
            }
            assert!(Instant::now() < deadline, "workers did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn address_navigation_preserves_registered_source_identity_and_rejects_traversal() {
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fixture.path().join("one/two")).unwrap();
        let mut browser = browser();
        browser.execute(1, Command::AddSource(fixture.path().to_owned())).unwrap();
        settle_all(&mut browser);
        browser.execute(2, Command::BrowsePath(fixture.path().join("one/two"))).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.sources.len(), 1);
        assert_eq!(browser.view.location.as_ref().unwrap().source, SourceId(1));
        assert_eq!(browser.view.location.as_ref().unwrap().path.as_path(), std::path::Path::new("one/two"));
        assert!(browser.execute(3, Command::BrowsePath(fixture.path().join("../outside"))).is_err());
    }
    #[test]
    fn inline_expansion_is_lazy_merged_and_cancelled_by_new_navigation() {
        let root = tempfile::tempdir().unwrap();
        for id in 1..=4 {
            std::fs::create_dir_all(root.path().join(format!("disk{id}/movies/Title/Extras"))).unwrap();
            std::fs::write(root.path().join(format!("disk{id}/movies/Title/file.txt")), b"safe").unwrap();
        }
        let mut browser = browser();
        browser.configure(fixture_library(root.path()), root.path().join("library.json")).unwrap();
        browser.execute(1, Command::BrowseCollection(1)).unwrap();
        settle_all(&mut browser);
        let title = browser.view.entries[0].id.clone();
        assert_eq!(browser.rows("", false).len(), 1);
        browser.execute(2, Command::Expand(title.clone())).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.rows("", false).len(), 6);
        assert_eq!(browser.rows("file", false).len(), 5); // parent plus four physical files
        assert!(browser.execute(3, Command::PreviewRecovery(title.clone())).is_err());
        browser.execute(4, Command::Expand(title.clone())).unwrap();
        assert_eq!(browser.rows("", false).len(), 1);
        browser.execute(5, Command::Expand(title)).unwrap();
        assert_eq!(browser.rows("", false).len(), 6);
        browser.execute(6, Command::Navigate(Location { source: SourceId(1), path: RelativePath::default() })).unwrap();
        settle_all(&mut browser);
        assert!(browser.branches.is_empty());
    }
    #[test]
    fn explicit_recursive_search_preserves_same_name_hits_at_different_depths() {
        let root = tempfile::tempdir().unwrap();
        for folder in ["one", "two/deep"] {
            std::fs::create_dir_all(root.path().join(folder)).unwrap();
            std::fs::write(root.path().join(folder).join("match.txt"), b"safe").unwrap();
        }
        let mut browser = browser();
        browser.execute(1, Command::AddSource(root.path().to_owned())).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.filtered_indices("match", false).len(), 0);
        browser.execute(2, Command::Search("match".into())).unwrap();
        settle_all(&mut browser);
        assert!(browser.view.search);
        assert_eq!(browser.view.entries.len(), 2);
        assert_ne!(browser.view.entries[0].id, browser.view.entries[1].id);
        browser.execute(3, Command::Refresh).unwrap();
        settle_all(&mut browser);
        assert!(!browser.view.search);
    }
    #[test]
    fn recovery_requires_policy_preview_and_single_use_confirmation_and_restores() {
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fixture.path().join("media/Title")).unwrap();
        std::fs::write(fixture.path().join("media/Title/file.txt"), b"safe").unwrap();
        let source =
            Source { id: SourceId(1), root: fixture.path().to_owned(), label: "Fixture".into(), nfs_remote: None };
        let mut config = LibrarySettings { sources: vec![source], ..Default::default() };
        let mut browser = browser();
        browser.configure(config.clone(), fixture.path().join("config/library.json")).unwrap();
        browser.execute(1, Command::AddSource(fixture.path().to_owned())).unwrap();
        settle_all(&mut browser);
        let media = browser.view.entries[0].id.clone();
        assert!(browser.execute(2, Command::PreviewRecovery(media.clone())).is_err());
        config.recovery.push(RecoveryPolicy {
            source: SourceId(1),
            enabled: true,
            allowed: vec![RelativePath::default()],
            protected: vec![ProtectedPath { path: RelativePath::new("media").unwrap(), subtree: false }],
        });
        browser.execute(3, Command::SaveLibrary(config.clone())).unwrap();
        settle_all(&mut browser);
        browser
            .execute(4, Command::Navigate(Location { source: SourceId(1), path: RelativePath::new("media").unwrap() }))
            .unwrap();
        settle_all(&mut browser);
        let title = browser.view.entries[0].id.clone();
        browser.execute(5, Command::PreviewRecovery(title.clone())).unwrap();
        settle_all(&mut browser);
        assert!(fixture.path().join("media/Title").exists());
        let id = browser.recovery_preview().unwrap().id;
        browser.execute(6, Command::ConfirmRecovery(id)).unwrap();
        assert!(browser.execute(7, Command::ConfirmRecovery(id)).is_err());
        settle_all(&mut browser);
        assert!(!fixture.path().join("media/Title").exists());
        browser.execute(8, Command::ListRecovery(SourceId(1))).unwrap();
        settle_all(&mut browser);
        let job = browser.recovery_records().unwrap().1[0].id.clone();
        browser.execute(9, Command::Restore(SourceId(1), job)).unwrap();
        settle_all(&mut browser);
        assert_eq!(std::fs::read(fixture.path().join("media/Title/file.txt")).unwrap(), b"safe");
        let title = browser.view.entries[0].id.clone();
        browser.execute(10, Command::PreviewRecovery(title)).unwrap();
        settle_all(&mut browser);
        let id = browser.recovery_preview().unwrap().id;
        browser.execute(11, Command::SaveLibrary(config)).unwrap();
        assert!(browser.execute(12, Command::ConfirmRecovery(id)).is_err());
        settle_all(&mut browser);
        assert!(fixture.path().join("media/Title").exists());
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
        sender
            .send(Event { generation: 8, node: None, result: Update::Finished(Err("obsolete failure".into())) })
            .unwrap();
        browser.poll();
        assert_eq!(browser.view.phase, Phase::Loading);
        browser.execute(1, Command::Cancel).unwrap();
        sender.send(Event { generation: 9, node: None, result: Update::Finished(Ok(ReadSummary::default())) }).unwrap();
        browser.poll();
        assert_eq!(browser.view.phase, Phase::Cancelled);
    }
}
