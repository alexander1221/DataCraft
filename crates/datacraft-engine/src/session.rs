//! Transactional project edits and lifecycle. Full contract: docs/COMMANDS.md.
use crate::jobs::{ExecutionEvent, JobError, JobInfo, JobManager, JobOptions};
use crate::{Command, EngineError, ExecutionResult};
use datacraft_core::{GraphError, NodeKind};
use datacraft_project::{Project, ProjectError};
use serde::Serialize;
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use uuid::Uuid;

pub const DEFAULT_HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub java: PathBuf,
    pub classpath_file: PathBuf,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("command {command} disabled: {reason}")]
    Disabled {
        command: &'static str,
        reason: &'static str,
    },
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error(transparent)]
    Execution(#[from] EngineError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("edge does not exist")]
    MissingEdge,
    #[error("batch may contain only project edits")]
    InvalidBatch,
    #[error("save the project before executing relative source paths")]
    RelativeSourceWithoutProject,
    #[error("revision counter exhausted")]
    RevisionExhausted,
    #[error("save path must name a file")]
    InvalidSavePath,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CommandAvailability {
    pub enabled: bool,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExecutionRecord {
    pub project_revision: u64,
    pub result: ExecutionResult,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandPayload {
    None,
    Created,
    Opened { path: PathBuf },
    Saved { path: PathBuf },
    JobStarted { job_id: Uuid, project_revision: u64 },
    CancellationRequested { job_id: Uuid, accepted: bool },
    JobStatus { info: JobInfo },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEvent {
    Created,
    ProjectChanged { revision: u64 },
    Opened { path: PathBuf },
    Saved { path: PathBuf },
}

#[derive(Debug, Serialize)]
pub struct CommandResult {
    pub command_id: &'static str,
    pub revision: u64,
    pub dirty: bool,
    pub changed: bool,
    pub payload: CommandPayload,
    pub events: Vec<SessionEvent>,
}

pub struct Session {
    project: Option<Project>,
    path: Option<PathBuf>,
    saved: Option<Project>,
    undo: VecDeque<Project>,
    redo: VecDeque<Project>,
    history_limit: usize,
    revision: u64,
    computation_revision: u64,
    pending_execution_revision: Option<(u64, u64)>,
    result_computation_revision: Option<u64>,
    worker_config: Option<WorkerConfig>,
    pub(crate) jobs: JobManager,
    job_options: JobOptions,
    last_execution: Option<Arc<ExecutionRecord>>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_LIMIT)
    }
}

impl Session {
    pub fn new(history_limit: usize) -> Self {
        Self {
            project: None,
            path: None,
            saved: None,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            history_limit,
            revision: 0,
            computation_revision: 0,
            pending_execution_revision: None,
            result_computation_revision: None,
            worker_config: None,
            jobs: JobManager::default(),
            job_options: JobOptions::default(),
            last_execution: None,
        }
    }
    pub fn from_project(mut project: Project, history_limit: usize) -> Result<Self, SessionError> {
        project.validate_structure()?;
        canonicalize(&mut project);
        Ok(Self {
            project: Some(project),
            revision: 1,
            computation_revision: 1,
            ..Self::new(history_limit)
        })
    }
    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_dirty(&self) -> bool {
        self.project.as_ref() != self.saved.as_ref()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn last_execution(&self) -> Option<&ExecutionRecord> {
        self.last_execution.as_deref()
    }
    pub fn result_is_current(&self) -> bool {
        self.last_execution
            .as_ref()
            .is_some_and(|_| self.result_computation_revision == Some(self.computation_revision))
    }
    pub fn configure_worker(&mut self, config: WorkerConfig) {
        self.worker_config = Some(config);
    }

    pub fn configure_job_options(&mut self, options: JobOptions) {
        self.job_options = options;
    }
    pub fn current_job(&self) -> Option<JobInfo> {
        self.jobs.current_info()
    }
    pub fn job_status(&self, id: Uuid) -> Result<JobInfo, SessionError> {
        Ok(self.jobs.info(id)?)
    }
    pub fn job_result(&self, id: Uuid) -> Result<Option<Arc<ExecutionRecord>>, SessionError> {
        Ok(self.jobs.result(id)?)
    }
    pub fn poll_execution(&mut self) -> Vec<ExecutionEvent> {
        self.refresh_execution();
        self.jobs.drain_events()
    }
    fn refresh_execution(&mut self) {
        if let Some(info) = self.jobs.current_info() {
            if let Ok(Some(record)) = self.jobs.result(info.id) {
                if !self
                    .last_execution
                    .as_ref()
                    .is_some_and(|previous| Arc::ptr_eq(previous, &record))
                {
                    self.result_computation_revision = self
                        .pending_execution_revision
                        .filter(|(project, _)| *project == record.project_revision)
                        .map(|(_, computation)| computation);
                    self.last_execution = Some(record);
                }
            }
        }
    }
    /// Blocking CLI/host convenience. GUI must poll instead.
    pub fn wait_for_execution(
        &mut self,
        id: Uuid,
        timeout: Duration,
    ) -> Result<JobInfo, SessionError> {
        let info = self.jobs.wait(id, timeout)?;
        self.refresh_execution();
        Ok(info)
    }

    pub fn availability(&self, command: &Command) -> CommandAvailability {
        let reason = match command {
            Command::Open { .. } | Command::New { .. } if self.jobs.is_busy() => {
                Some("execution is active; cancel or wait before opening another project")
            }
            Command::Open {
                discard_changes: false,
                ..
            }
            | Command::New {
                discard_changes: false,
                ..
            } if self.is_dirty() => Some("unsaved changes; save or explicitly discard"),
            Command::Open { .. } | Command::New { .. } => None,
            Command::Cancel { .. } | Command::JobStatus { .. } => None,
            _ if self.project.is_none() => Some("no project open"),
            Command::Undo if !self.can_undo() => Some("undo history is empty"),
            Command::Redo if !self.can_redo() => Some("redo history is empty"),
            Command::Save if self.path.is_none() => Some("no save path; use save_as"),
            Command::Run { .. } if self.jobs.is_busy() => Some("another execution job is active"),
            Command::Run { .. } if self.worker_config.is_none() => Some("worker is not configured"),
            _ => None,
        };
        CommandAvailability {
            enabled: reason.is_none(),
            reason,
        }
    }

    fn next_revision(&self) -> Result<u64, SessionError> {
        self.revision
            .checked_add(1)
            .ok_or(SessionError::RevisionExhausted)
    }
    fn current(&self) -> Result<&Project, SessionError> {
        self.project.as_ref().ok_or(SessionError::Disabled {
            command: "project",
            reason: "no project open",
        })
    }
    fn push_history(history: &mut VecDeque<Project>, project: Project, limit: usize) {
        if limit == 0 {
            return;
        }
        if history.len() == limit {
            history.pop_front();
        }
        history.push_back(project);
    }

    pub fn dispatch(&mut self, command: Command) -> Result<CommandResult, SessionError> {
        self.refresh_execution();
        let id = command.spec().id;
        if let Some(reason) = self.availability(&command).reason {
            return Err(SessionError::Disabled {
                command: id,
                reason,
            });
        }
        let before_revision = self.revision;
        let mut events = Vec::new();
        let payload = if command.spec().undoable {
            let before = self.current()?.clone();
            let mut candidate = before.clone();
            apply_edit(&mut candidate, command, 0)?;
            candidate.validate_structure()?;
            canonicalize(&mut candidate);
            if candidate != before {
                let revision = self.next_revision()?;
                let computation = self
                    .computation_revision
                    .checked_add(u64::from(!same_computation(&before, &candidate)))
                    .ok_or(SessionError::RevisionExhausted)?;
                Self::push_history(&mut self.undo, before, self.history_limit);
                self.redo.clear();
                self.project = Some(candidate);
                self.revision = revision;
                self.computation_revision = computation;
            }
            CommandPayload::None
        } else {
            match command {
                Command::Undo | Command::Redo => {
                    let revision = self.next_revision()?;
                    let before = self.current()?.clone();
                    let (from, to) = if matches!(command, Command::Undo) {
                        (&mut self.undo, &mut self.redo)
                    } else {
                        (&mut self.redo, &mut self.undo)
                    };
                    let restored = from.back().ok_or(SessionError::Disabled {
                        command: id,
                        reason: "history is empty",
                    })?;
                    let computation = self
                        .computation_revision
                        .checked_add(u64::from(!same_computation(&before, restored)))
                        .ok_or(SessionError::RevisionExhausted)?;
                    let restored = from.pop_back().ok_or(SessionError::InvalidBatch)?;
                    Self::push_history(to, before, self.history_limit);
                    self.project = Some(restored);
                    self.revision = revision;
                    self.computation_revision = computation;
                    CommandPayload::None
                }
                Command::New { name, .. } => {
                    let project = Project {
                        format_version: datacraft_project::FORMAT_VERSION,
                        name,
                        graph: Default::default(),
                        output_node: None,
                    };
                    project.validate_structure()?;
                    let revision = self.next_revision()?;
                    self.project = Some(project);
                    self.saved = None;
                    self.path = None;
                    self.undo.clear();
                    self.redo.clear();
                    self.last_execution = None;
                    self.jobs = JobManager::default();
                    self.revision = revision;
                    events.push(SessionEvent::Created);
                    self.computation_revision = revision;
                    self.pending_execution_revision = None;
                    self.result_computation_revision = None;
                    CommandPayload::Created
                }
                Command::Open { path, .. } => {
                    let path = std::fs::canonicalize(path)?;
                    let mut project = Project::load(&path)?;
                    canonicalize(&mut project);
                    let revision = self.next_revision()?;
                    self.saved = Some(project.clone());
                    self.project = Some(project);
                    self.path = Some(path.clone());
                    self.undo.clear();
                    self.redo.clear();
                    self.last_execution = None;
                    self.jobs = JobManager::default();
                    self.revision = revision;
                    events.push(SessionEvent::Opened { path: path.clone() });
                    self.computation_revision = revision;
                    self.pending_execution_revision = None;
                    self.result_computation_revision = None;
                    CommandPayload::Opened { path }
                }
                Command::Save => {
                    let path = self.path.clone().ok_or(SessionError::InvalidSavePath)?;
                    self.save_as(path, &mut events)?
                }
                Command::SaveAs { path } => self.save_as(path, &mut events)?,
                Command::Run { output } => {
                    let project = self.current()?;
                    let output = project.graph.output(output.or(project.output_node))?;
                    let order = project.graph.execution_order(output)?;
                    let runtime = if let Some(path) = &self.path {
                        project.resolved(path)?
                    } else {
                        for node in project.graph.nodes.iter().filter(|n| order.contains(&n.id)) {
                            let source = match &node.config {
                                NodeKind::CsvSource { path, .. }
                                | NodeKind::ParquetSource { path }
                                | NodeKind::JsonSource { path } => Some(path),
                                _ => None,
                            };
                            if source.is_some_and(|path| !Path::new(path).is_absolute()) {
                                return Err(SessionError::RelativeSourceWithoutProject);
                            }
                        }
                        project.clone()
                    };
                    let config = self.worker_config.clone().ok_or(SessionError::Disabled {
                        command: id,
                        reason: "worker is not configured",
                    })?;
                    let info = self.jobs.start(
                        runtime,
                        output,
                        self.revision,
                        config,
                        self.job_options,
                    )?;
                    self.pending_execution_revision =
                        Some((self.revision, self.computation_revision));
                    CommandPayload::JobStarted {
                        job_id: info.id,
                        project_revision: info.project_revision,
                    }
                }
                Command::Cancel { job_id } => CommandPayload::CancellationRequested {
                    job_id,
                    accepted: self.jobs.cancel(job_id)?,
                },
                Command::JobStatus { job_id } => CommandPayload::JobStatus {
                    info: self.jobs.info(job_id)?,
                },
                _ => return Err(SessionError::InvalidBatch),
            }
        };
        let changed = before_revision != self.revision;
        if changed {
            events.push(SessionEvent::ProjectChanged {
                revision: self.revision,
            });
        }
        Ok(CommandResult {
            command_id: id,
            revision: self.revision,
            dirty: self.is_dirty(),
            changed,
            payload,
            events,
        })
    }

    fn save_as(
        &mut self,
        path: PathBuf,
        events: &mut Vec<SessionEvent>,
    ) -> Result<CommandPayload, SessionError> {
        let filename = path.file_name().ok_or(SessionError::InvalidSavePath)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let directory = std::fs::canonicalize(parent)?;
        let path = directory.join(filename);
        let mut candidate = self.current()?.clone();
        let previous_base = self.path.as_ref().and_then(|p| p.parent());
        if let Some(base) = previous_base {
            rebase(&mut candidate, base, &directory);
        }
        let changed = candidate != *self.current()?;
        let revision = if changed {
            self.next_revision()?
        } else {
            self.revision
        };
        candidate.save(&path)?;
        if let Some(base) = previous_base {
            for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
                rebase(snapshot, base, &directory);
            }
        }
        self.project = Some(candidate.clone());
        self.saved = Some(candidate);
        self.path = Some(path.clone());
        self.revision = revision;
        events.push(SessionEvent::Saved { path: path.clone() });
        Ok(CommandPayload::Saved { path })
    }
}

fn same_computation(a: &Project, b: &Project) -> bool {
    a.format_version == b.format_version
        && a.output_node == b.output_node
        && a.graph.edges == b.graph.edges
        && a.graph.nodes.iter().map(|n| (&n.id, &n.config)).eq(b
            .graph
            .nodes
            .iter()
            .map(|n| (&n.id, &n.config)))
}

fn apply_edit(project: &mut Project, command: Command, depth: usize) -> Result<(), SessionError> {
    if depth > 64 {
        return Err(SessionError::InvalidBatch);
    }
    match command {
        Command::AddNode { node } => project.graph.nodes.push(node),
        Command::RemoveNode { node_id } => {
            let index = project
                .graph
                .nodes
                .iter()
                .position(|n| n.id == node_id)
                .ok_or(GraphError::MissingNode(node_id))?;
            project.graph.nodes.remove(index);
            project
                .graph
                .edges
                .retain(|e| e.from != node_id && e.to != node_id);
            if project.output_node == Some(node_id) {
                project.output_node = None;
            }
        }
        Command::ConfigureNode { node_id, config } => {
            project
                .graph
                .nodes
                .iter_mut()
                .find(|n| n.id == node_id)
                .ok_or(GraphError::MissingNode(node_id))?
                .config = config;
        }
        Command::MoveNode { node_id, position } => {
            project
                .graph
                .nodes
                .iter_mut()
                .find(|n| n.id == node_id)
                .ok_or(GraphError::MissingNode(node_id))?
                .position = position;
        }
        Command::Connect { edge } => project.graph.edges.push(edge),
        Command::Disconnect { edge } => {
            let index = project
                .graph
                .edges
                .iter()
                .position(|e| *e == edge)
                .ok_or(SessionError::MissingEdge)?;
            project.graph.edges.remove(index);
        }
        Command::SetOutput { output } => project.output_node = output,
        Command::Rename { name } => project.name = name,
        Command::Batch { commands } => {
            for command in commands {
                if !command.spec().undoable {
                    return Err(SessionError::InvalidBatch);
                }
                apply_edit(project, command, depth + 1)?;
            }
        }
        _ => return Err(SessionError::InvalidBatch),
    }
    Ok(())
}

fn canonicalize(project: &mut Project) {
    project.graph.nodes.sort_by_key(|node| node.id);
    project.graph.edges.sort();
}

/// Lexical normalization: sources need not exist while editing a draft.
fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(part.as_os_str()),
        }
    }
    out
}

fn rebase(project: &mut Project, old: &Path, new: &Path) {
    if old == new {
        return;
    }
    for node in &mut project.graph.nodes {
        let path = match &mut node.config {
            NodeKind::CsvSource { path, .. }
            | NodeKind::ParquetSource { path }
            | NodeKind::JsonSource { path } => path,
            _ => continue,
        };
        if path.trim().is_empty() || path.contains("://") || Path::new(path).is_absolute() {
            continue;
        }
        let absolute = normalized(&old.join(&*path));
        let target: Vec<_> = absolute.components().collect();
        let base: Vec<_> = new.components().collect();
        let common = target.iter().zip(&base).take_while(|(a, b)| a == b).count();
        let relative = if common == 0 {
            absolute.clone()
        } else {
            let mut out = PathBuf::new();
            for _ in common..base.len() {
                out.push("..");
            }
            for part in &target[common..] {
                out.push(part.as_os_str());
            }
            out
        };
        *path = relative.to_string_lossy().into_owned();
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod computation_tests;
