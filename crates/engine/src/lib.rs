//! Headless, bounded browsing and explicit physical file actions.

use archiv_core::actions::{ProtectedPath, WritePolicy};
use archiv_core::library::{LibrarySettings, merge_directories};
use archiv_core::{Entry, EntryId, EntryKind, Location, RelativePath, Source, SourceId, display_name, sort_entries};
use archiv_platform::operations::{self, DeletePlan};
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
    /// Edit an absolute folder within one source, including a remote SFTP source.
    BrowseSourcePath(SourceId, PathBuf),
    /// Browse a configured virtual collection at its root.
    BrowseCollection(u64),
    /// Revisit the previous physical/collection/search location.
    Back,
    /// Revisit the next location after Back.
    Forward,
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
    /// Build an immutable physical deletion preview; never mutate here.
    PreviewDelete(EntryId),
    /// Execute one previously previewed permanent deletion exactly once.
    ConfirmDelete(u64),
    /// Dismiss a preview without executing it.
    DismissDelete,
    /// Remember one explicit physical item in the session copy clipboard.
    Copy(EntryId),
    /// Copy that clipboard item into an explicit physical directory.
    Paste(Location),
    /// Rename one observed physical item in its current parent.
    Rename(EntryId, std::ffi::OsString),
    /// Copy an exact UTF-8 physical path as text (never an operation path).
    CopyPath(EntryId),
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
    Prepare(u64, Source, Entry, WritePolicy, Vec<(Source, ProtectedPath)>),
    Delete(DeletePlan),
    Rename(Source, Entry, std::ffi::OsString, WritePolicy, Vec<(Source, ProtectedPath)>),
    Copy(Source, Entry, Source, RelativePath, WritePolicy, Vec<(Source, ProtectedPath)>),
    Launch(Source, Entry, String),
}
enum ActionResult {
    Preview(Box<DeletePlan>),
    Done(String),
    Changed(String),
}
fn perform(
    action: Action,
    cancel: &AtomicBool,
    children: &mut Vec<operations::Application>,
) -> Result<ActionResult, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Operation cancelled before execution".into());
    }
    match action {
        Action::Prepare(id, source, entry, policy, guards) => {
            operations::prepare_delete(id, source, entry, policy, guards, cancel)
                .map(|plan| ActionResult::Preview(Box::new(plan)))
        }
        Action::Delete(plan) => operations::delete(&plan, cancel)
            .map(|()| ActionResult::Changed("Permanently deleted the confirmed physical items".into())),
        Action::Rename(source, entry, name, policy, guards) => {
            operations::rename(&source, &entry, &name, &policy, &guards)
                .map(|()| ActionResult::Changed("Renamed the physical item".into()))
        }
        Action::Copy(source, entry, destination, folder, policy, guards) => {
            operations::copy(&source, &entry, &destination, &folder, &policy, &guards, cancel)
                .map(|()| ActionResult::Changed("Copy completed; source retained".into()))
        }
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

const NAVIGATION_LIMIT: usize = 128;
#[derive(Clone, PartialEq, Eq)]
struct Navigation {
    locations: Vec<Location>,
    collection: Option<u64>,
    search: Option<String>,
}

/// Two bounded workers shared by browsing, configured application launches and opt-in file actions.
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
    back: std::collections::VecDeque<Navigation>,
    forward: std::collections::VecDeque<Navigation>,
    search_query: Option<String>,
    branches: HashMap<EntryId, Branch>,
    operation_pending: bool,
    operation_status: Option<String>,
    delete_preview: Option<DeletePlan>,
    clipboard: Option<(Source, Entry)>,
    clipboard_text: Option<String>,
    action_cancel: Option<Arc<AtomicBool>>,
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
                            if !send(Update::Action(perform(action, &job.cancel, &mut children))) {
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
            back: Default::default(),
            forward: Default::default(),
            search_query: None,
            branches: HashMap::new(),
            operation_pending: false,
            operation_status: None,
            delete_preview: None,
            clipboard: None,
            clipboard_text: None,
            action_cancel: None,
        })
    }

    /// Restore validated source definitions without touching their filesystems.
    pub fn configure(&mut self, settings: LibrarySettings, path: PathBuf) -> Result<(), EngineError> {
        settings.validate().map_err(EngineError)?;
        self.next_source = settings.sources.iter().map(|s| s.id.0).max().unwrap_or(0).saturating_add(1);
        self.sources = settings.sources.clone();
        self.library = settings;
        self.back.clear();
        self.forward.clear();
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
    /// Whether a previous navigation entry is available.
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    /// Whether a forward navigation entry is available.
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    /// Query belonging to the current recursive-search location.
    pub fn search_query(&self) -> Option<&str> {
        self.search_query.as_deref()
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
                self.visit(Navigation {
                    locations: self.locations.clone(),
                    collection: self.view.collection,
                    search: Some(query),
                })
            }
            Command::Expand(id) => self.expand(id),
            Command::DismissDelete => {
                self.delete_preview = None;
                Ok(())
            }
            Command::Launch(id) => {
                self.file_action_availability(&id).map_err(EngineError)?;
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
            Command::PreviewDelete(id) => {
                self.delete_preview = None;
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
            Command::ConfirmDelete(id) => {
                let plan = self
                    .delete_preview
                    .take()
                    .filter(|p| p.id == id)
                    .ok_or_else(|| EngineError("Deletion preview expired; prepare it again".into()))?;
                self.action(Action::Delete(plan))
            }
            Command::Copy(id) => {
                let (source, entry) = self.physical(&id)?;
                self.clipboard = Some((source, entry));
                self.operation_status = Some("Copied selection; choose a physical folder and Paste".into());
                Ok(())
            }
            Command::CopyPath(id) => {
                let (source, entry) = self.physical(&id)?;
                self.clipboard_text =
                    Some(source.root.join(entry.id.path.as_path()).into_os_string().into_string().map_err(|_| {
                        EngineError("This path is not UTF-8; its display label was not copied as an exact path".into())
                    })?);
                Ok(())
            }
            Command::Paste(location) => {
                self.paste_availability(&location).map_err(EngineError)?;
                let (source, entry) =
                    self.clipboard.clone().ok_or_else(|| EngineError("Copy a physical item first".into()))?;
                let destination = self
                    .source(location.source)
                    .cloned()
                    .ok_or_else(|| EngineError("Unknown destination source".into()))?;
                let policy = self.policy(location.source)?.clone();
                self.action(Action::Copy(source, entry, destination, location.path, policy, self.guards()))
            }
            Command::Rename(id, name) => {
                self.write_availability(&id).map_err(EngineError)?;
                operations::filename(&name).map_err(EngineError)?;
                let (source, entry) = self.physical(&id)?;
                let policy = self.policy(id.source)?.clone();
                self.action(Action::Rename(source, entry, name, policy, self.guards()))
            }
            Command::SaveLibrary(settings) => {
                if self.operation_pending {
                    return Err(EngineError("Wait for the current operation before changing settings".into()));
                }
                self.delete_preview = None;
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
                self.visit(Navigation { locations: members, collection: Some(id), search: None })
            }
            Command::Back => self.travel(false),
            Command::Forward => self.travel(true),
            Command::AddSource(root) => self.add_source(root),
            Command::BrowseSourcePath(id, path) => {
                let source = self.source(id).ok_or_else(|| EngineError("Unknown source".into()))?;
                let relative = path
                    .strip_prefix(&source.root)
                    .map_err(|_| EngineError("Folder must remain inside the configured source root".into()))?;
                let relative = RelativePath::new(relative).map_err(|e| EngineError(e.to_string()))?;
                self.navigate(Location { source: id, path: relative })
            }
            Command::BrowsePath(path) => {
                if !path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                    return Err(EngineError("Use an absolute address without parent traversal".into()));
                }
                if let Some(source) = self
                    .sources
                    .iter()
                    .filter(|s| s.sftp.is_none() && path.starts_with(&s.root))
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
                    self.visit(Navigation { locations, collection: Some(collection), search: None })
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
                if let Some(cancel) = &self.action_cancel {
                    cancel.store(true, Ordering::Release);
                }
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
        if !root.is_absolute() {
            return Err(EngineError("Choose an absolute folder path".into()));
        }
        let existing =
            self.sources.iter().find(|source| source.sftp.is_none() && source.root == root).map(|source| source.id);
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
            self.sources.push(Source { id, root, label, nfs_remote: None, sftp: None });
            id
        };
        self.navigate(Location { source: id, path: RelativePath::default() })
    }
    fn navigate(&mut self, location: Location) -> Result<(), EngineError> {
        self.visit(Navigation { locations: vec![location], collection: None, search: None })
    }

    fn current_navigation(&self) -> Option<Navigation> {
        (!self.locations.is_empty()).then(|| Navigation {
            locations: self.locations.clone(),
            collection: self.view.collection,
            search: self.search_query.clone(),
        })
    }
    fn visit(&mut self, next: Navigation) -> Result<(), EngineError> {
        let previous = self.current_navigation();
        self.start_listing(next.locations.clone(), next.collection, next.search.clone())?;
        if previous.as_ref() != Some(&next) {
            if let Some(previous) = previous {
                self.back.push_back(previous);
                if self.back.len() > NAVIGATION_LIMIT {
                    self.back.pop_front();
                }
            }
            self.forward.clear();
        }
        Ok(())
    }
    fn travel(&mut self, forward: bool) -> Result<(), EngineError> {
        let next = if forward { self.forward.back() } else { self.back.back() }
            .cloned()
            .ok_or_else(|| EngineError(if forward { "No next location" } else { "No previous location" }.into()))?;
        let current = self.current_navigation().ok_or_else(|| EngineError("Open a folder first".into()))?;
        self.start_listing(next.locations, next.collection, next.search)?;
        if forward {
            self.forward.pop_back();
            self.back.push_back(current);
        } else {
            self.back.pop_back();
            self.forward.push_back(current);
        }
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
        self.search_query = search.clone();
        self.view.pending = if search.is_some() { 1 } else { locations.len() };
        self.branches.clear();
        self.delete_preview = None;
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
                        self.delete_preview = Some(*plan);
                        self.operation_status = None;
                    }
                    Ok(ActionResult::Preview(_)) => {
                        self.operation_status = Some("Preview expired after navigation".into())
                    }
                    Ok(ActionResult::Changed(message)) => {
                        self.operation_status = Some(message);
                        if !self.locations.is_empty()
                            && let Err(error) = self.start(self.locations.clone(), self.view.collection)
                        {
                            self.operation_status = Some(format!("Operation completed; refresh needed: {error}"));
                        }
                    }
                    Ok(ActionResult::Done(message)) => {
                        self.operation_status = Some(message);
                    }
                    Err(error) => {
                        let refresh = if !self.locations.is_empty() {
                            self.start(self.locations.clone(), self.view.collection).err()
                        } else {
                            None
                        };
                        self.operation_status = Some(refresh.map_or_else(
                            || error.clone(),
                            |refresh| format!("{error}; refresh also failed: {refresh}"),
                        ));
                    }
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
                        self.delete_preview = None;
                        self.view.phase = Phase::Idle;
                        self.view.location = None;
                        self.view.collection = None;
                        self.view.entries.clear();
                        self.view.contributors.clear();
                        self.view.errors.clear();
                        self.view.summary = None;
                        self.locations.clear();
                        self.back.clear();
                        self.forward.clear();
                        self.search_query = None;
                        self.clipboard = None;
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
    /// Single-use deletion preview.
    pub fn delete_preview(&self) -> Option<&DeletePlan> {
        self.delete_preview.as_ref()
    }
    /// Consume pending clipboard text once; no lossy path can become an operation target.
    pub fn take_clipboard_text(&mut self) -> Option<String> {
        self.clipboard_text.take()
    }
    /// Shared destination availability; execution revalidates paths and policy in the worker.
    pub fn paste_availability(&self, location: &Location) -> Result<(), String> {
        if self.operation_pending || self.settings_pending {
            return Err("Wait for the current operation/settings save".into());
        }
        let (_, entry) = self.clipboard.as_ref().ok_or("Copy a physical item first")?;
        let path = location.path.child(&entry.name).map_err(|e| e.to_string())?;
        self.policy(location.source).map_err(|e| e.to_string())?.permits(&path, entry.kind)
    }
    /// Shared remote-file action boundary for UI availability and engine admission.
    pub fn file_action_availability(&self, id: &EntryId) -> Result<(), String> {
        let source = self.source(id.source).ok_or("Unknown source")?;
        if source.sftp.is_some() {
            return Err("SFTP supports browsing and folder navigation only; file actions and external applications are unavailable".into());
        }
        Ok(())
    }
    /// Physical identity check shared by menu actions and engine admission.
    pub fn physical(&self, id: &EntryId) -> Result<(Source, Entry), EngineError> {
        self.file_action_availability(id).map_err(EngineError)?;
        let entry = self.entry(id).cloned().ok_or_else(|| EngineError("Item is no longer visible".into()))?;
        if self.contributors(id).is_some() || !matches!(entry.kind, EntryKind::File | EntryKind::Directory) {
            return Err(EngineError("Choose an explicit physical file or folder; merged folders, links and special files have no single action target".into()));
        }
        let source = self.source(id.source).cloned().ok_or_else(|| EngineError("Unknown source".into()))?;
        Ok((source, entry))
    }
    /// Shared availability explanation used by menus, shortcuts and direct engine calls.
    pub fn write_availability(&self, id: &EntryId) -> Result<(), String> {
        let (_, entry) = self.physical(id).map_err(|e| e.to_string())?;
        if self.view.collection.is_some() && !self.view.search && entry.kind == EntryKind::Directory {
            return Err("Virtual folders have no delete target. Open an explicit physical source first.".into());
        }
        if self.operation_pending || self.settings_pending {
            return Err("Wait for the current operation/settings save".into());
        }
        self.policy(id.source).map_err(|e| e.to_string())?.permits(&id.path, entry.kind)
    }
    fn guards(&self) -> Vec<(Source, ProtectedPath)> {
        let mut guards = Vec::new();
        for source in self.library.sources.iter().filter(|s| s.sftp.is_none()) {
            guards.push((source.clone(), ProtectedPath { path: RelativePath::default(), subtree: false }));
            if let Some(policy) = self.library.writes.iter().find(|p| p.source == source.id) {
                for protected in &policy.protected {
                    guards.push((source.clone(), protected.clone()));
                }
            }
        }
        for member in self.library.collections.iter().flat_map(|c| c.members.iter()) {
            if let Some(source) = self.source(member.source).filter(|s| s.sftp.is_none()) {
                guards.push((source.clone(), ProtectedPath { path: member.path.clone(), subtree: false }));
            }
        }
        guards
    }
    fn policy(&self, source: SourceId) -> Result<&WritePolicy, EngineError> {
        if self.source(source).is_some_and(|s| s.sftp.is_some()) {
            return Err(EngineError("SFTP sources support browsing only".into()));
        }
        self.library
            .writes
            .iter()
            .find(|p| p.source == source)
            .ok_or_else(|| EngineError("Configure file-change permissions in Preferences → Safeguards first".into()))
    }
    fn action(&mut self, action: Action) -> Result<(), EngineError> {
        if self.operation_pending || self.settings_pending {
            return Err(EngineError("Wait for the current operation/settings save".into()));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            generation: self.generation,
            node: None,
            action: Some(action),
            search: None,
            requests: Vec::new(),
            save: None,
            cancel: Arc::clone(&cancel),
        };
        self.jobs
            .as_ref()
            .ok_or_else(|| EngineError("Workers stopped".into()))?
            .try_send(job)
            .map_err(|_| EngineError("Workers busy; try again shortly".into()))?;
        self.action_cancel = Some(cancel);
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

    struct RemoteFixture;
    impl DirectoryReader for RemoteFixture {
        fn read(
            &self,
            request: &ReadRequest,
            cancel: &AtomicBool,
            emit: &mut dyn FnMut(Vec<Entry>) -> bool,
        ) -> Result<ReadSummary, archiv_platform::ReadError> {
            let mut local = request.clone();
            local.source.sftp = None;
            LocalDirectoryReader.read(&local, cancel, emit)
        }
    }
    #[test]
    fn sftp_navigation_search_history_and_read_only_actions_share_engine_rules() {
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.path().join("folder")).unwrap();
        std::fs::write(fixture.path().join("folder/notes.txt"), b"safe").unwrap();
        let source = Source {
            id: SourceId(1),
            root: fixture.path().to_owned(),
            label: "Remote".into(),
            nfs_remote: None,
            sftp: Some(archiv_core::sftp::SftpConnection { host: "fixture".into(), user: String::new(), port: None }),
        };
        let settings = LibrarySettings { sources: vec![source.clone()], ..Default::default() };
        let mut browser = Browser::new(Arc::new(RemoteFixture), Arc::new(|| {})).unwrap();
        browser.configure(settings, fixture.path().join("config.json")).unwrap();
        assert_eq!(browser.view.phase, Phase::Idle);
        browser.execute(1, Command::Navigate(Location { source: source.id, path: RelativePath::default() })).unwrap();
        settle(&mut browser);
        let folder = browser.view.entries[0].id.clone();
        browser.execute(2, Command::Open(folder)).unwrap();
        settle(&mut browser);
        let file = browser.view.entries[0].id.clone();
        for (request, command) in (3..).zip([
            Command::Launch(file.clone()),
            Command::Copy(file.clone()),
            Command::CopyPath(file.clone()),
            Command::PreviewDelete(file.clone()),
            Command::Rename(file.clone(), "renamed".into()),
            Command::Paste(Location { source: source.id, path: RelativePath::default() }),
        ]) {
            assert!(browser.execute(request, command).is_err());
        }
        assert!(browser.file_action_availability(&file).unwrap_err().contains("browsing"));
        assert!(!browser.operation_pending());
        browser.execute(10, Command::Back).unwrap();
        settle(&mut browser);
        assert!(browser.view.location.as_ref().unwrap().path.as_path().as_os_str().is_empty());
        browser.execute(11, Command::Search("notes".into())).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries[0].id, file);
        assert!(browser.execute(12, Command::BrowseSourcePath(source.id, fixture.path().join("../outside"))).is_err());
        browser.execute(13, Command::BrowseSourcePath(source.id, fixture.path().join("folder"))).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries[0].id, file);
        assert_eq!(std::fs::read(fixture.path().join("folder/notes.txt")).unwrap(), b"safe");
    }

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
            writes: Vec::new(),
            sources: (1..=4)
                .map(|id| Source {
                    id: SourceId(id),
                    root: root.join(format!("disk{id}")),
                    label: format!("Disk {id}"),
                    nfs_remote: None,
                    sftp: None,
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
        assert!(browser.execute(3, Command::PreviewDelete(title.clone())).is_err());
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
        browser.execute(4, Command::Back).unwrap();
        settle(&mut browser);
        assert_eq!(browser.view.entries.len(), 5);
        browser.execute(5, Command::Back).unwrap();
        settle(&mut browser);
        assert!(!browser.can_back());
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
    #[test]
    fn file_actions_share_policy_copy_identity_and_single_use_delete() {
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.path().join("out")).unwrap();
        std::fs::write(fixture.path().join("notes.txt"), b"safe").unwrap();
        let source = Source {
            id: SourceId(1),
            root: fixture.path().to_owned(),
            label: "Fixture".into(),
            nfs_remote: None,
            sftp: None,
        };
        let mut config = LibrarySettings { sources: vec![source], ..Default::default() };
        let mut browser = browser();
        browser.configure(config.clone(), fixture.path().join("config/library.json")).unwrap();
        browser.execute(1, Command::AddSource(fixture.path().to_owned())).unwrap();
        settle_all(&mut browser);
        let id = browser.view.entries.iter().find(|e| e.name == "notes.txt").unwrap().id.clone();
        assert!(browser.execute(2, Command::PreviewDelete(id.clone())).is_err());
        assert!(browser.execute(3, Command::Rename(id.clone(), "new.txt".into())).is_err());
        config.writes.push(WritePolicy {
            source: SourceId(1),
            enabled: true,
            allowed: vec![RelativePath::default()],
            protected: Vec::new(),
        });
        browser.configure(config, fixture.path().join("config/library.json")).unwrap();
        browser.execute(4, Command::Copy(id.clone())).unwrap();
        browser
            .execute(5, Command::Paste(Location { source: SourceId(1), path: RelativePath::new("out").unwrap() }))
            .unwrap();
        settle_all(&mut browser);
        assert_eq!(std::fs::read(fixture.path().join("out/notes.txt")).unwrap(), b"safe");
        assert!(fixture.path().join("notes.txt").exists());
        browser.execute(6, Command::Rename(id, "renamed.txt".into())).unwrap();
        settle_all(&mut browser);
        assert!(fixture.path().join("renamed.txt").exists());
        let renamed = browser.view.entries.iter().find(|e| e.name == "renamed.txt").unwrap().id.clone();
        browser.execute(7, Command::PreviewDelete(renamed.clone())).unwrap();
        settle_all(&mut browser);
        let plan = browser.delete_preview().unwrap().id;
        assert!(fixture.path().join("renamed.txt").exists());
        browser.execute(8, Command::ConfirmDelete(plan)).unwrap();
        browser.execute(8, Command::ConfirmDelete(plan)).unwrap();
        assert!(browser.execute(9, Command::ConfirmDelete(plan)).is_err());
        settle_all(&mut browser);
        assert!(!fixture.path().join("renamed.txt").exists());
        assert!(fixture.path().join("out/notes.txt").exists());
    }
    #[test]
    fn back_forward_restore_locations_search_and_discard_a_branched_future() {
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.path().join("one")).unwrap();
        std::fs::create_dir(fixture.path().join("two")).unwrap();
        std::fs::write(fixture.path().join("one/needle.txt"), b"safe").unwrap();
        let mut browser = browser();
        assert!(!browser.can_back());
        assert!(!browser.can_forward());
        browser.execute(1, Command::AddSource(fixture.path().to_owned())).unwrap();
        settle_all(&mut browser);
        let one = browser.view.entries.iter().find(|e| e.name == "one").unwrap().id.clone();
        browser.execute(2, Command::Open(one)).unwrap();
        settle_all(&mut browser);
        browser.execute(3, Command::BrowsePath(fixture.path().join("two"))).unwrap();
        settle_all(&mut browser);
        browser.execute(4, Command::Back).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.view.location.as_ref().unwrap().path.as_path(), std::path::Path::new("one"));
        browser.execute(5, Command::Forward).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.view.location.as_ref().unwrap().path.as_path(), std::path::Path::new("two"));
        browser.execute(6, Command::Back).unwrap();
        settle_all(&mut browser);
        browser.execute(7, Command::Search("needle".into())).unwrap();
        settle_all(&mut browser);
        assert!(!browser.can_forward());
        assert!(browser.view.search);
        browser.execute(8, Command::Back).unwrap();
        settle_all(&mut browser);
        assert!(!browser.view.search);
        browser.execute(9, Command::Forward).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.search_query(), Some("needle"));
        assert!(browser.view.search);
        let back_count = browser.back.len();
        browser.execute(10, Command::Refresh).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.back.len(), back_count);
        let before = browser.current_navigation();
        assert!(
            browser
                .execute(11, Command::Navigate(Location { source: SourceId(999), path: RelativePath::default() }))
                .is_err()
        );
        assert!(browser.current_navigation() == before);
        assert_eq!(browser.back.len(), back_count);
    }
    #[test]
    fn collection_history_keeps_all_contributors_across_sources_and_branches() {
        let root = tempfile::tempdir().unwrap();
        for id in 1..=4 {
            std::fs::create_dir_all(root.path().join(format!("disk{id}/movies/Title/Extras"))).unwrap();
            std::fs::write(root.path().join(format!("disk{id}/movies/Title/movie.mkv")), b"safe").unwrap();
        }
        let mut browser = browser();
        browser.configure(fixture_library(root.path()), root.path().join("config.json")).unwrap();
        browser.execute(1, Command::BrowseCollection(1)).unwrap();
        settle_all(&mut browser);
        let title = browser.view.entries[0].id.clone();
        browser.execute(2, Command::Open(title)).unwrap();
        settle_all(&mut browser);
        browser.execute(3, Command::Navigate(Location { source: SourceId(1), path: RelativePath::default() })).unwrap();
        settle_all(&mut browser);
        browser.execute(4, Command::Back).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.view.collection, Some(1));
        assert_eq!(browser.locations.len(), 4);
        browser.execute(5, Command::Back).unwrap();
        settle_all(&mut browser);
        assert!(!browser.can_back());
        assert!(browser.can_forward());
        assert_eq!(browser.view.entries.len(), 1);
        let title = browser.view.entries[0].id.clone();
        assert_eq!(browser.contributors(&title).unwrap().len(), 4);
        browser.execute(6, Command::Forward).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.locations.len(), 4);
        assert_eq!(browser.view.entries.len(), 5);
        browser.execute(7, Command::BrowseCollection(1)).unwrap();
        settle_all(&mut browser);
        assert!(!browser.can_forward());
        browser.execute(8, Command::Back).unwrap();
        settle_all(&mut browser);
        assert_eq!(browser.locations.len(), 4);
        assert_eq!(browser.view.entries.len(), 5);
    }
    #[test]
    fn navigation_history_is_bounded_and_layout_replay_steps_only_once() {
        let fixture = tempfile::tempdir().unwrap();
        for name in ["one", "two"] {
            std::fs::create_dir(fixture.path().join(name)).unwrap();
        }
        let mut browser = browser();
        let mut commands = FrameCommands::default();
        commands.dispatch(1, Command::AddSource(fixture.path().to_owned()), &mut browser).unwrap();
        settle_all(&mut browser);
        for (frame, name) in [(2, "one"), (3, "two")] {
            commands.dispatch(frame, Command::BrowsePath(fixture.path().join(name)), &mut browser).unwrap();
            settle_all(&mut browser);
        }
        for _ in 0..3 {
            commands.dispatch(4, Command::Back, &mut browser).unwrap();
        }
        settle_all(&mut browser);
        assert_eq!(browser.view.location.as_ref().unwrap().path.as_path(), std::path::Path::new("one"));
        for frame in 5..150 {
            let name = if frame % 2 == 0 { "one" } else { "two" };
            commands.dispatch(frame, Command::BrowsePath(fixture.path().join(name)), &mut browser).unwrap();
            settle_all(&mut browser);
        }
        assert_eq!(browser.back.len(), NAVIGATION_LIMIT);
        assert!(!browser.can_forward());
        for frame in 150..278 {
            commands.dispatch(frame, Command::Back, &mut browser).unwrap();
            settle_all(&mut browser);
        }
        assert!(!browser.can_back());
        assert_eq!(browser.forward.len(), NAVIGATION_LIMIT);
    }
}
