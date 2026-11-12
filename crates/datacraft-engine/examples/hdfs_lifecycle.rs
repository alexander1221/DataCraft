//! Real HDFS acceptance; creates a named demo artifact and prints its path.
use datacraft_core::NodeKind;
use datacraft_engine::{jobs::JobStatus, Command, CommandPayload, Session, WorkerConfig};
use std::{error::Error, path::PathBuf, time::Duration};
use uuid::Uuid;

fn run(session: &mut Session, command: Command) -> Result<Uuid, Box<dyn Error>> {
    let CommandPayload::JobStarted { job_id, .. } = session.dispatch(command)?.payload else {
        return Err("missing job".into());
    };
    let info = session.wait_for_execution(job_id, Duration::from_secs(120))?;
    if info.status != JobStatus::Completed {
        return Err(format!("HDFS job failed: {:?}", info.error).into());
    }
    Ok(job_id)
}
fn config(session: &Session, id: Uuid, path: &str) -> Result<NodeKind, Box<dyn Error>> {
    let node = session
        .project()
        .ok_or("missing project")?
        .graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or("missing node")?;
    let mut value = serde_json::to_value(&node.config)?;
    value["hdfs_path"] = path.into();
    if let Ok(host) = std::env::var("DATACRAFT_HDFS_HOST") {
        value["host"] = host.into();
    }
    if let Ok(port) = std::env::var("DATACRAFT_HDFS_PORT") {
        value["port"] = port.parse::<u32>()?.into();
    }
    Ok(serde_json::from_value(value)?)
}
fn main() -> Result<(), Box<dyn Error>> {
    let root = format!("/datacraft-examples/{}", Uuid::new_v4());
    let path = format!("{root}/sales.tsv");
    let worker = WorkerConfig {
        java: std::env::var_os("DATACRAFT_JAVA")
            .map(PathBuf::from)
            .unwrap_or_else(|| "java".into()),
        classpath_file: std::env::var_os("DATACRAFT_WORKER_CLASSPATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| "workers/spark/target/classpath.txt".into()),
    };
    let mut export = Session::default();
    export.configure_worker(worker.clone());
    export.dispatch(Command::Open {
        path: "examples/pipelines/hdfs_export_text.dcraft".into(),
        discard_changes: false,
    })?;
    let export_config = config(&export, Uuid::from_u128(2), &path)?;
    export.dispatch(Command::ConfigureNode {
        node_id: Uuid::from_u128(2),
        config: export_config.clone(),
    })?;
    run(&mut export, Command::Run { output: None })?;
    assert!(
        export
            .last_execution()
            .ok_or("missing export completion")?
            .result
            .tables
            .is_empty(),
        "export must not invent a table output"
    );
    let revision = export.revision();
    let job = run(
        &mut export,
        Command::HdfsList {
            node_id: Uuid::from_u128(2),
            config: export_config,
            path: root.clone(),
        },
    )?;
    let listing = export.job_result(job)?.ok_or("missing directory listing")?;
    assert_eq!(listing.result.tables[0].rows[0]["path"], path);
    assert_eq!(export.revision(), revision);
    assert!(export
        .last_execution()
        .ok_or("export result replaced")?
        .result
        .tables
        .is_empty());
    let mut import = Session::default();
    import.configure_worker(worker);
    import.dispatch(Command::Open {
        path: "examples/pipelines/hdfs_import_text.dcraft".into(),
        discard_changes: false,
    })?;
    let import_config = config(&import, Uuid::from_u128(1), &path)?;
    import.dispatch(Command::ConfigureNode {
        node_id: Uuid::from_u128(1),
        config: import_config.clone(),
    })?;
    run(
        &mut import,
        Command::RunNode {
            node_id: Uuid::from_u128(1),
        },
    )?;
    let before = import.node_statuses();
    run(
        &mut import,
        Command::HdfsList {
            node_id: Uuid::from_u128(1),
            config: import_config.clone(),
            path: root.clone(),
        },
    )?;
    assert_eq!(
        import.node_statuses(),
        before,
        "browsing must not alter cached node readiness"
    );
    let CommandPayload::JobStarted { job_id, .. } = import
        .dispatch(Command::HdfsList {
            node_id: Uuid::from_u128(1),
            config: import_config,
            path: root,
        })?
        .payload
    else {
        return Err("missing browse job".into());
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if import.poll_execution().iter().any(|e| {
            matches!(
                e,
                datacraft_engine::jobs::ExecutionEvent::Stage {
                    stage: datacraft_engine::jobs::ExecutionStage::Executing,
                    ..
                }
            )
        }) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("browse worker did not start".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    import.dispatch(Command::Cancel { job_id })?;
    assert_eq!(
        import
            .wait_for_execution(job_id, Duration::from_secs(60))?
            .status,
        JobStatus::Cancelled
    );
    assert_eq!(
        import.node_statuses(),
        before,
        "cancelling a read-only browser must preserve the idle cached worker"
    );
    run(
        &mut import,
        Command::RunNode {
            node_id: Uuid::from_u128(2),
        },
    )?;
    let rows = &import
        .last_execution()
        .ok_or("missing imported preview")?
        .result
        .tables[0]
        .rows;
    assert_eq!(rows.len(), 12);
    for (index, row) in rows.iter().enumerate() {
        let q = index as i64 + 1;
        assert_eq!(row["quantity"], q);
        assert_eq!(row["amount"], 2 * q + 3);
        assert_eq!(row["region"], "EU");
    }
    println!(
        "PASS: real HDFS export/import, exact 12 rows, read-only browse and persistent-cache reuse"
    );
    println!("Demo artifact retained in HDFS: {path}");
    Ok(())
}
