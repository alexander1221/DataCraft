//! Real JVM cancellation/recovery smoke example. Run from workspace root.
use datacraft_engine::jobs::{ExecutionEvent, ExecutionStage, JobError, JobOptions, JobStatus};
use datacraft_engine::{Command, CommandPayload, Session, WorkerConfig};
use std::{error::Error, path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let project = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: job_lifecycle <project.dcraft>")?;
    let mut session = Session::default();
    session.configure_worker(WorkerConfig {
        java: std::env::var_os("DATACRAFT_JAVA")
            .map(PathBuf::from)
            .unwrap_or_else(|| "java".into()),
        classpath_file: std::env::var_os("DATACRAFT_WORKER_CLASSPATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| "workers/spark/target/classpath.txt".into()),
    });
    session.configure_job_options(JobOptions {
        timeout: Some(Duration::from_secs(60)),
    });
    session.dispatch(Command::Open {
        path: project,
        discard_changes: false,
    })?;
    let initial_revision = session.revision();
    let CommandPayload::JobStarted { job_id, .. } =
        session.dispatch(Command::Run { output: None })?.payload
    else {
        return Err("expected job handle".into());
    };
    let mut cancellation_sent = false;
    let terminal = loop {
        for event in session.poll_execution() {
            println!("{}", serde_json::to_string(&event)?);
            if matches!(
                event,
                ExecutionEvent::Stage {
                    stage: ExecutionStage::Executing,
                    ..
                }
            ) && !cancellation_sent
            {
                session.dispatch(Command::Cancel { job_id })?;
                cancellation_sent = true;
            }
        }
        match session.wait_for_execution(job_id, Duration::from_millis(20)) {
            Ok(info) => break info,
            Err(datacraft_engine::SessionError::Job(JobError::WaitTimeout(_))) => {}
            Err(error) => return Err(error.into()),
        }
    };
    if !cancellation_sent || terminal.status != JobStatus::Cancelled {
        return Err(format!("expected confirmed cancellation, got {:?}", terminal.status).into());
    }
    if session.revision() != initial_revision || session.can_undo() || session.is_dirty() {
        return Err("execution modified persistent state".into());
    }
    println!("Cancellation confirmed after JVM cleanup");

    let CommandPayload::JobStarted { job_id, .. } =
        session.dispatch(Command::Run { output: None })?.payload
    else {
        return Err("expected recovery job".into());
    };
    session.dispatch(Command::Rename {
        name: "Edited while the snapshot runs".into(),
    })?;
    let terminal = session.wait_for_execution(job_id, Duration::from_secs(70))?;
    if terminal.status != JobStatus::Completed {
        return Err(format!("recovery failed: {:?}", terminal.error).into());
    }
    let result = session.job_result(job_id)?.ok_or("missing result")?;
    if result.project_revision != initial_revision || session.result_is_current() {
        return Err("snapshot revision tracking failed".into());
    }
    println!("Recovery completed: {} result tables; snapshot revision {}, current revision {}; stale result detected",
        result.result.tables.len(),result.project_revision,session.revision());
    Ok(())
}
