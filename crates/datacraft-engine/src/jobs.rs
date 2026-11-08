//! Single-active-job execution. Contracts and race semantics: docs/EXECUTION.md.
use crate::{
    session::{ExecutionRecord, WorkerConfig},
    EngineError, Worker, WorkerCancellation,
};
use datacraft_project::Project;
use serde::Serialize;
use std::{
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub struct JobOptions {
    pub timeout: Option<Duration>,
}
impl Default for JobOptions {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(300)),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}
impl JobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Io,
    Protocol,
    Query,
    InvalidProject,
    Internal,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct JobFailure {
    pub kind: FailureKind,
    pub node: Option<Uuid>,
    pub message: String,
}
impl From<EngineError> for JobFailure {
    fn from(error: EngineError) -> Self {
        let (kind, node) = match &error {
            EngineError::Execution { node, .. } => (FailureKind::Query, *node),
            EngineError::Io(_) => (FailureKind::Io, None),
            EngineError::Json(_) | EngineError::Protocol(_) => (FailureKind::Protocol, None),
            EngineError::Graph(_) | EngineError::Project(_) => (FailureKind::InvalidProject, None),
        };
        Self {
            kind,
            node,
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JobInfo {
    pub id: Uuid,
    pub project_revision: u64,
    pub status: JobStatus,
    pub stop_reason: Option<StopReason>,
    pub error: Option<JobFailure>,
    pub termination_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStage {
    StartingWorker,
    Executing,
    StoppingWorker,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionEvent {
    Queued { job_id: Uuid, project_revision: u64 },
    Started { job_id: Uuid, project_revision: u64 },
    Stage { job_id: Uuid, stage: ExecutionStage },
    CancellationRequested { job_id: Uuid, reason: StopReason },
    Completed { job_id: Uuid, project_revision: u64 },
    Failed { job_id: Uuid, error: JobFailure },
    Cancelled { job_id: Uuid },
    TimedOut { job_id: Uuid },
}

#[derive(Debug, Error)]
pub enum JobError {
    #[error("another execution job is active")]
    Busy,
    #[error("unknown or replaced job {0}")]
    UnknownJob(Uuid),
    #[error("job timeout must be positive and fit a monotonic deadline")]
    InvalidTimeout,
    #[error("background thread could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("waiting for job {0} timed out; the job may still be active")]
    WaitTimeout(Uuid),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

struct JobState {
    info: JobInfo,
    events: VecDeque<ExecutionEvent>,
    result: Option<Arc<ExecutionRecord>>,
}
struct Observable {
    state: Mutex<JobState>,
    changed: Condvar,
}
impl JobState {
    fn push(&mut self, event: ExecutionEvent) {
        if self.events.len() == 64 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}
struct ControlState {
    stop: Option<StopReason>,
    done: bool,
    worker: Option<WorkerCancellation>,
    kill_attempted: bool,
}
struct Control {
    state: Mutex<ControlState>,
    wake: Condvar,
    observable: Arc<Observable>,
}
impl Control {
    fn request_stop(&self, reason: StopReason) -> bool {
        let mut state = lock(&self.state);
        if state.done || state.stop.is_some() {
            return false;
        }
        state.stop = Some(reason);
        let mut shared = lock(&self.observable.state);
        shared.info.status = JobStatus::Cancelling;
        shared.info.stop_reason = Some(reason);
        let id = shared.info.id;
        shared.push(ExecutionEvent::CancellationRequested { job_id: id, reason });
        self.wake.notify_all();
        self.observable.changed.notify_all();
        true
    }
    fn install_worker(&self, worker: WorkerCancellation) {
        lock(&self.state).worker = Some(worker);
        self.wake.notify_all();
    }
    fn stop_reason(&self) -> Option<StopReason> {
        lock(&self.state).stop
    }
    fn started(&self) {
        let mut shared = lock(&self.observable.state);
        if shared.info.status == JobStatus::Queued {
            shared.info.status = JobStatus::Running;
        }
        let id = shared.info.id;
        let revision = shared.info.project_revision;
        shared.push(ExecutionEvent::Started {
            job_id: id,
            project_revision: revision,
        });
        self.observable.changed.notify_all();
    }
    fn stage(&self, stage: ExecutionStage) {
        let mut shared = lock(&self.observable.state);
        let id = shared.info.id;
        shared.push(ExecutionEvent::Stage { job_id: id, stage });
        self.observable.changed.notify_all();
    }
    fn finish(&self, outcome: Result<ExecutionRecord, JobFailure>) {
        let mut control = lock(&self.state);
        control.done = true;
        let mut shared = lock(&self.observable.state);
        let id = shared.info.id;
        let revision = shared.info.project_revision;
        let event = match control.stop {
            Some(StopReason::Cancelled) => {
                shared.info.status = JobStatus::Cancelled;
                ExecutionEvent::Cancelled { job_id: id }
            }
            Some(StopReason::TimedOut) => {
                shared.info.status = JobStatus::TimedOut;
                ExecutionEvent::TimedOut { job_id: id }
            }
            None => match outcome {
                Ok(record) => {
                    shared.info.status = JobStatus::Completed;
                    shared.result = Some(Arc::new(record));
                    ExecutionEvent::Completed {
                        job_id: id,
                        project_revision: revision,
                    }
                }
                Err(error) => {
                    shared.info.status = JobStatus::Failed;
                    shared.info.error = Some(error.clone());
                    ExecutionEvent::Failed { job_id: id, error }
                }
            },
        };
        shared.push(event);
        self.wake.notify_all();
        self.observable.changed.notify_all();
    }
    fn supervise(&self, deadline: Option<Instant>) {
        loop {
            let mut state = lock(&self.state);
            if state.done {
                return;
            }
            if state.stop.is_none() && deadline.is_some_and(|d| Instant::now() >= d) {
                drop(state);
                self.request_stop(StopReason::TimedOut);
                continue;
            }
            if state.stop.is_some() && !state.kill_attempted {
                if let Some(worker) = state.worker.clone() {
                    state.kill_attempted = true;
                    drop(state);
                    if let Err(error) = worker.terminate() {
                        lock(&self.observable.state).info.termination_error =
                            Some(error.to_string());
                        self.observable.changed.notify_all();
                    }
                    continue;
                }
            }
            if state.stop.is_none() {
                if let Some(deadline) = deadline {
                    let duration = deadline.saturating_duration_since(Instant::now());
                    let (next, _) = self
                        .wake
                        .wait_timeout(state, duration)
                        .unwrap_or_else(|p| p.into_inner());
                    drop(next);
                    continue;
                }
            }
            drop(self.wake.wait(state).unwrap_or_else(|p| p.into_inner()));
        }
    }
}

struct Job {
    control: Arc<Control>,
}
#[derive(Default)]
pub struct JobManager {
    current: Option<Job>,
}

impl JobManager {
    pub fn is_busy(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|j| !lock(&j.control.observable.state).info.status.is_terminal())
    }
    pub fn current_info(&self) -> Option<JobInfo> {
        self.current
            .as_ref()
            .map(|j| lock(&j.control.observable.state).info.clone())
    }
    fn job(&self, id: Uuid) -> Result<&Job, JobError> {
        self.current
            .as_ref()
            .filter(|j| lock(&j.control.observable.state).info.id == id)
            .ok_or(JobError::UnknownJob(id))
    }
    pub fn info(&self, id: Uuid) -> Result<JobInfo, JobError> {
        Ok(lock(&self.job(id)?.control.observable.state).info.clone())
    }
    pub fn cancel(&self, id: Uuid) -> Result<bool, JobError> {
        Ok(self.job(id)?.control.request_stop(StopReason::Cancelled))
    }
    pub fn result(&self, id: Uuid) -> Result<Option<Arc<ExecutionRecord>>, JobError> {
        Ok(lock(&self.job(id)?.control.observable.state).result.clone())
    }
    pub fn drain_events(&mut self) -> Vec<ExecutionEvent> {
        self.current
            .as_ref()
            .map(|j| lock(&j.control.observable.state).events.drain(..).collect())
            .unwrap_or_default()
    }
    /// Blocking consumer convenience; GUI should use info/drain_events.
    pub fn wait(&self, id: Uuid, timeout: Duration) -> Result<JobInfo, JobError> {
        if Instant::now().checked_add(timeout).is_none() {
            return Err(JobError::InvalidTimeout);
        }
        let observable = &self.job(id)?.control.observable;
        let state = lock(&observable.state);
        let (state, _) = observable
            .changed
            .wait_timeout_while(state, timeout, |s| !s.info.status.is_terminal())
            .unwrap_or_else(|p| p.into_inner());
        if state.info.status.is_terminal() {
            Ok(state.info.clone())
        } else {
            Err(JobError::WaitTimeout(id))
        }
    }

    pub fn start(
        &mut self,
        project: Project,
        output: Uuid,
        revision: u64,
        config: WorkerConfig,
        options: JobOptions,
    ) -> Result<JobInfo, JobError> {
        self.spawn(revision, options, move |control| {
            if control.stop_reason().is_some() {
                return Err(EngineError::Protocol("cancelled before startup".into()).into());
            }
            control.stage(ExecutionStage::StartingWorker);
            let mut worker = Worker::start(&config.java, &config.classpath_file)?;
            control.install_worker(worker.cancellation_handle());
            let outcome = if control.stop_reason().is_none() {
                control.stage(ExecutionStage::Executing);
                worker.run(&project, Some(output))
            } else {
                Err(EngineError::Protocol("cancelled before execution".into()))
            };
            control.stage(ExecutionStage::StoppingWorker);
            let cleanup = worker.cancel();
            drop(worker);
            cleanup?;
            Ok(ExecutionRecord {
                project_revision: revision,
                result: outcome?,
            })
        })
    }

    fn spawn<F>(
        &mut self,
        revision: u64,
        options: JobOptions,
        runner: F,
    ) -> Result<JobInfo, JobError>
    where
        F: FnOnce(Arc<Control>) -> Result<ExecutionRecord, JobFailure> + Send + 'static,
    {
        if self.is_busy() {
            return Err(JobError::Busy);
        }
        let deadline = match options.timeout {
            Some(duration) if duration.is_zero() => return Err(JobError::InvalidTimeout),
            Some(duration) => Some(
                Instant::now()
                    .checked_add(duration)
                    .ok_or(JobError::InvalidTimeout)?,
            ),
            None => None,
        };
        let info = JobInfo {
            id: Uuid::new_v4(),
            project_revision: revision,
            status: JobStatus::Queued,
            stop_reason: None,
            error: None,
            termination_error: None,
        };
        let observable = Arc::new(Observable {
            state: Mutex::new(JobState {
                info: info.clone(),
                events: VecDeque::from([ExecutionEvent::Queued {
                    job_id: info.id,
                    project_revision: revision,
                }]),
                result: None,
            }),
            changed: Condvar::new(),
        });
        let control = Arc::new(Control {
            state: Mutex::new(ControlState {
                stop: None,
                done: false,
                worker: None,
                kill_attempted: false,
            }),
            wake: Condvar::new(),
            observable,
        });
        let supervisor = Arc::clone(&control);
        thread::Builder::new()
            .name("datacraft-job-supervisor".into())
            .spawn(move || supervisor.supervise(deadline))?;
        let active = Arc::clone(&control);
        if let Err(error) = thread::Builder::new()
            .name("datacraft-job-runner".into())
            .spawn(move || {
                active.started();
                let outcome = catch_unwind(AssertUnwindSafe(|| runner(Arc::clone(&active))))
                    .unwrap_or_else(|_| {
                        Err(JobFailure {
                            kind: FailureKind::Internal,
                            node: None,
                            message: "execution runner panicked".into(),
                        })
                    });
                active.finish(outcome);
            })
        {
            control.finish(Err(JobFailure {
                kind: FailureKind::Io,
                node: None,
                message: error.to_string(),
            }));
            return Err(error.into());
        }
        self.current = Some(Job { control });
        Ok(info)
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        if let Some(job) = &self.current {
            job.control.request_stop(StopReason::Cancelled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session::Session, ExecutionResult};
    use std::{
        io::{BufReader, Read},
        process::{Command as ProcessCommand, Stdio},
        sync::mpsc,
    };

    // Synthetic results below test only scheduling/state contracts. Actual query
    // correctness is checked by the real CLI/Spark fixtures, not by these values.
    fn record(revision: u64) -> ExecutionRecord {
        ExecutionRecord {
            project_revision: revision,
            result: ExecutionResult { tables: vec![] },
        }
    }
    fn no_deadline() -> JobOptions {
        JobOptions { timeout: None }
    }
    fn wait_for_stop(control: &Control) {
        let state = lock(&control.state);
        let (state, timeout) = control
            .wake
            .wait_timeout_while(state, Duration::from_secs(5), |s| s.stop.is_none())
            .unwrap_or_else(|p| p.into_inner());
        assert!(!timeout.timed_out(), "test runner did not receive stop");
        assert!(state.stop.is_some());
    }

    #[test]
    fn success_busy_events_and_unknown_ids() {
        let mut manager = JobManager::default();
        let (started, ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let info = manager
            .spawn(7, no_deadline(), move |_| {
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(record(7))
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(manager.info(info.id).unwrap().status, JobStatus::Running);
        assert!(matches!(
            manager.spawn(8, no_deadline(), |_| panic!("must not run")),
            Err(JobError::Busy)
        ));
        assert!(matches!(
            manager.info(Uuid::new_v4()),
            Err(JobError::UnknownJob(_))
        ));
        assert!(matches!(
            manager.wait(info.id, Duration::ZERO),
            Err(JobError::WaitTimeout(_))
        ));
        release.send(()).unwrap();
        assert_eq!(
            manager
                .wait(info.id, Duration::from_secs(5))
                .unwrap()
                .status,
            JobStatus::Completed
        );
        assert_eq!(
            manager.result(info.id).unwrap().unwrap().project_revision,
            7
        );
        assert!(!manager.cancel(info.id).unwrap());
        let events = manager.drain_events();
        assert!(matches!(
            events.first(),
            Some(ExecutionEvent::Queued { .. })
        ));
        assert!(matches!(
            events.last(),
            Some(ExecutionEvent::Completed { .. })
        ));
        assert!(manager.drain_events().is_empty());
        let next = manager.spawn(8, no_deadline(), |_| Ok(record(8))).unwrap();
        assert!(matches!(
            manager.info(info.id),
            Err(JobError::UnknownJob(_))
        ));
        manager.wait(next.id, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn cancel_before_worker_registration_waits_for_cleanup_and_first_reason_wins() {
        let mut manager = JobManager::default();
        let (started, ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let info = manager
            .spawn(1, no_deadline(), move |control| {
                started.send(()).unwrap();
                wait_for_stop(&control);
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(record(1))
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(manager.cancel(info.id).unwrap());
        assert!(!manager.cancel(info.id).unwrap());
        assert!(!manager
            .current
            .as_ref()
            .unwrap()
            .control
            .request_stop(StopReason::TimedOut));
        assert_eq!(manager.info(info.id).unwrap().status, JobStatus::Cancelling);
        assert!(manager.is_busy());
        assert!(manager.result(info.id).unwrap().is_none());
        release.send(()).unwrap();
        let done = manager.wait(info.id, Duration::from_secs(5)).unwrap();
        assert_eq!(done.status, JobStatus::Cancelled);
        assert_eq!(done.stop_reason, Some(StopReason::Cancelled));
        assert!(manager.result(info.id).unwrap().is_none());
        let events = manager.drain_events();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, ExecutionEvent::Cancelled { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn deadline_is_enforced_without_polling_or_consumer_requests() {
        let mut manager = JobManager::default();
        let info = manager
            .spawn(
                1,
                JobOptions {
                    timeout: Some(Duration::from_millis(20)),
                },
                |control| {
                    wait_for_stop(&control);
                    Ok(record(1))
                },
            )
            .unwrap();
        let done = manager.wait(info.id, Duration::from_secs(5)).unwrap();
        assert_eq!(done.status, JobStatus::TimedOut);
        assert_eq!(done.stop_reason, Some(StopReason::TimedOut));
        assert!(manager.result(info.id).unwrap().is_none());
        assert!(!manager.cancel(info.id).unwrap());
    }

    #[test]
    fn failure_panic_and_invalid_options_are_structured() {
        let mut manager = JobManager::default();
        assert!(matches!(
            manager.spawn(
                1,
                JobOptions {
                    timeout: Some(Duration::ZERO)
                },
                |_| Ok(record(1))
            ),
            Err(JobError::InvalidTimeout)
        ));
        assert!(manager.current_info().is_none());
        let node = Uuid::new_v4();
        let info = manager
            .spawn(1, no_deadline(), move |_| {
                Err(JobFailure {
                    kind: FailureKind::Query,
                    node: Some(node),
                    message: "known test error".into(),
                })
            })
            .unwrap();
        let done = manager.wait(info.id, Duration::from_secs(5)).unwrap();
        assert_eq!(done.status, JobStatus::Failed);
        assert_eq!(done.error.unwrap().node, Some(node));
        let info = manager
            .spawn(2, no_deadline(), |_| panic!("test runner panic"))
            .unwrap();
        assert_eq!(
            manager
                .wait(info.id, Duration::from_secs(5))
                .unwrap()
                .error
                .unwrap()
                .kind,
            FailureKind::Internal
        );
    }

    #[test]
    fn event_retention_is_bounded_and_keeps_terminal_event() {
        let mut manager = JobManager::default();
        let info = manager
            .spawn(1, no_deadline(), |control| {
                for _ in 0..100 {
                    control.stage(ExecutionStage::Executing);
                }
                Ok(record(1))
            })
            .unwrap();
        manager.wait(info.id, Duration::from_secs(5)).unwrap();
        let events = manager.drain_events();
        assert_eq!(events.len(), 64);
        assert!(matches!(
            events.last(),
            Some(ExecutionEvent::Completed { .. })
        ));
    }

    #[test]
    fn dropping_manager_requests_stop() {
        let (completed, cleanup) = mpsc::channel();
        let mut manager = JobManager::default();
        manager
            .spawn(1, no_deadline(), move |control| {
                wait_for_stop(&control);
                completed.send(()).unwrap();
                Ok(record(1))
            })
            .unwrap();
        drop(manager);
        cleanup.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    #[test]
    #[ignore = "subprocess fixture, invoked explicitly by cancel_terminates_real_child_and_unblocks_reader"]
    fn child_process_fixture() {
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn cancel_terminates_real_child_and_unblocks_reader() {
        let (registered, ready) = mpsc::channel();
        let mut manager = JobManager::default();
        let info = manager
            .spawn(1, no_deadline(), move |control| {
                let mut child = ProcessCommand::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "jobs::tests::child_process_fixture",
                        "--nocapture",
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                let input = child.stdin.take();
                let stdout = child.stdout.take().unwrap();
                let child = Arc::new(Mutex::new(child));
                let mut worker = Worker {
                    child: Arc::clone(&child),
                    input,
                    output: BufReader::new(stdout),
                };
                control.install_worker(worker.cancellation_handle());
                registered.send(()).unwrap();
                let mut text = String::new();
                worker.output.read_to_string(&mut text).unwrap();
                worker.cancel().unwrap();
                assert!(lock(&child).try_wait().unwrap().is_some());
                Ok(record(1))
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        manager.cancel(info.id).unwrap();
        assert_eq!(
            manager
                .wait(info.id, Duration::from_secs(5))
                .unwrap()
                .status,
            JobStatus::Cancelled
        );
    }

    #[test]
    fn session_can_edit_while_running_and_marks_old_result_stale() {
        let project =
            Project::from_json(include_str!("../../../examples/pipelines/sample.dcraft")).unwrap();
        let mut session = Session::from_project(project, 100).unwrap();
        // Test-only controlled scheduler exercises Session state, not Spark math.
        let (release, gate) = mpsc::channel();
        let info = session
            .jobs
            .spawn(session.revision(), no_deadline(), move |_| {
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(record(1))
            })
            .unwrap();
        session
            .dispatch(crate::Command::Rename {
                name: "While running".into(),
            })
            .unwrap();
        assert!(
            !session
                .availability(&crate::Command::New {
                    name: "Blocked".into(),
                    discard_changes: true
                })
                .enabled
        );
        assert!(
            !session
                .availability(&crate::Command::Open {
                    path: "irrelevant".into(),
                    discard_changes: true
                })
                .enabled
        );
        assert!(
            !session
                .availability(&crate::Command::Run { output: None })
                .enabled
        );
        release.send(()).unwrap();
        session
            .wait_for_execution(info.id, Duration::from_secs(5))
            .unwrap();
        assert_eq!(session.last_execution().unwrap().project_revision, 1);
        assert_eq!(session.revision(), 2);
        assert!(!session.result_is_current());
        assert!(session.can_undo());
        session.dispatch(crate::Command::Undo).unwrap();
        assert_eq!(session.revision(), 3);
        assert!(!session.result_is_current());
    }

    #[test]
    fn project_lifecycle_clears_completed_jobs_and_does_not_resurrect_old_results() {
        for new_project in [true, false] {
            let mut session = Session::default();
            let info = session
                .jobs
                .spawn(0, no_deadline(), |_| Ok(record(0)))
                .unwrap();
            session
                .wait_for_execution(info.id, Duration::from_secs(5))
                .unwrap();
            assert!(session.last_execution().is_some());
            let command = if new_project {
                crate::Command::New {
                    name: "Empty".into(),
                    discard_changes: false,
                }
            } else {
                crate::Command::Open {
                    path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../examples/pipelines/sample.dcraft"),
                    discard_changes: false,
                }
            };
            session.dispatch(command).unwrap();
            session.poll_execution();
            assert!(session.last_execution().is_none());
            assert!(session.current_job().is_none());
            assert!(session.job_result(info.id).is_err());
        }
    }
}
