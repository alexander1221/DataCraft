//! Acceptance against a real persistent JVM, not a simulated runner.
use datacraft_engine::{jobs::JobStatus, node_runtime::NodeStatus};
use datacraft_engine::{Command, CommandPayload, Session, WorkerConfig};
use std::{error::Error, path::PathBuf, time::Duration};
use uuid::Uuid;

fn run(s: &mut Session, command: Command) -> Result<JobStatus, Box<dyn Error>> {
    let CommandPayload::JobStarted { job_id, .. } = s.dispatch(command)?.payload else {
        return Err("missing job".into());
    };
    let info = s.wait_for_execution(job_id, Duration::from_secs(90))?;
    for e in s.poll_execution() {
        println!("{}", serde_json::to_string(&e)?);
    }
    Ok(info.status)
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut s = Session::default();
    s.configure_worker(WorkerConfig {
        java: std::env::var_os("DATACRAFT_JAVA")
            .map(PathBuf::from)
            .unwrap_or_else(|| "java".into()),
        classpath_file: std::env::var_os("DATACRAFT_WORKER_CLASSPATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| "workers/spark/target/classpath.txt".into()),
    });
    s.dispatch(Command::Open {
        path: "examples/pipelines/sample.dcraft".into(),
        discard_changes: false,
    })?;
    assert!(s
        .dispatch(Command::RunNode {
            node_id: Uuid::from_u128(2)
        })
        .is_err());
    assert!(s.current_job().is_none());
    assert_eq!(
        run(
            &mut s,
            Command::RunNode {
                node_id: Uuid::from_u128(1)
            }
        )?,
        JobStatus::Completed
    );
    assert_eq!(s.node_statuses()[&Uuid::from_u128(2)], NodeStatus::Idle);
    assert_eq!(
        run(
            &mut s,
            Command::RunNode {
                node_id: Uuid::from_u128(2)
            }
        )?,
        JobStatus::Completed
    );
    assert_eq!(
        run(
            &mut s,
            Command::RunDownstream {
                node_id: Uuid::from_u128(3)
            }
        )?,
        JobStatus::Completed
    );
    let rows = &s.last_execution().ok_or("missing result")?.result.tables[0].rows;
    assert_eq!(
        rows.iter()
            .map(|r| r["quantity"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![4, 5, 6]
    );
    s.dispatch(Command::MoveNode {
        node_id: Uuid::from_u128(2),
        position: [40.0, 70.0],
    })?;
    assert!(s
        .node_statuses()
        .values()
        .all(|v| *v == NodeStatus::Completed));
    s.dispatch(Command::ConfigureNode {
        node_id: Uuid::from_u128(2),
        config: datacraft_core::NodeKind::Filter {
            expression: "absent > 0".into(),
        },
    })?;
    assert_eq!(
        s.node_statuses()[&Uuid::from_u128(1)],
        NodeStatus::Completed
    );
    assert_eq!(s.node_statuses()[&Uuid::from_u128(5)], NodeStatus::Idle);
    assert_eq!(
        run(
            &mut s,
            Command::RunNode {
                node_id: Uuid::from_u128(2)
            }
        )?,
        JobStatus::Failed
    );
    assert_eq!(s.node_statuses()[&Uuid::from_u128(2)], NodeStatus::Failed);
    s.dispatch(Command::Undo)?;
    assert_eq!(
        run(
            &mut s,
            Command::RunDownstream {
                node_id: Uuid::from_u128(2)
            }
        )?,
        JobStatus::Completed
    );
    let CommandPayload::JobStarted { job_id, .. } = s
        .dispatch(Command::RunDownstream {
            node_id: Uuid::from_u128(1),
        })?
        .payload
    else {
        return Err("missing cancellation job".into());
    };
    s.dispatch(Command::Cancel { job_id })?;
    assert_eq!(
        s.wait_for_execution(job_id, Duration::from_secs(90))?
            .status,
        JobStatus::Cancelled
    );
    assert!(s
        .dispatch(Command::RunNode {
            node_id: Uuid::from_u128(2)
        })
        .is_err());
    assert_eq!(
        run(
            &mut s,
            Command::RunDownstream {
                node_id: Uuid::from_u128(1)
            }
        )?,
        JobStatus::Completed
    );
    println!("PASS: strict inputs, persistent cache, exact rows, invalidation, failure, cancel, recovery");
    Ok(())
}
