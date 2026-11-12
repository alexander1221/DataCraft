use super::*;

fn run_wait(app: &mut Desktop, ctx: &egui::Context) {
    click_label(app, ctx, "Run project");
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while app.active_job() {
        assert!(std::time::Instant::now() < deadline, "Parquet job timeout");
        frame(app, ctx, vec![]);
        std::thread::sleep(Duration::from_millis(10));
    }
    frame(app, ctx, vec![]);
    assert_eq!(
        app.session.current_job().unwrap().status,
        datacraft_engine::jobs::JobStatus::Completed,
        "{:?}",
        app.error
    );
}
fn acceptance(remote: bool) {
    let mut app = app();
    app.session.configure_worker(WorkerConfig {
        java: std::env::var_os("DATACRAFT_JAVA").map_or_else(|| "java".into(), PathBuf::from),
        classpath_file: std::env::var_os("DATACRAFT_WORKER_CLASSPATH").map_or_else(
            || {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../workers/spark/target/classpath.txt")
            },
            PathBuf::from,
        ),
    });
    let directory = tempfile::tempdir().unwrap();
    let target = if remote {
        format!("/datacraft-gui/{}/sales.parquet", Uuid::new_v4())
    } else {
        directory
            .path()
            .join("sales.parquet")
            .to_str()
            .unwrap()
            .to_owned()
    };
    let ctx = egui::Context::default();
    for (operation, node) in [("export", 2u128), ("import", 1u128)] {
        let name = format!(
            "{}parquet_{operation}.dcraft",
            if remote { "hdfs_" } else { "" }
        );
        // HDFS fixture names preserve the established import/export prefix.
        let name = if remote {
            format!("hdfs_{operation}_parquet.dcraft")
        } else {
            name
        };
        app.open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/pipelines")
                .join(name),
            true,
        );
        let id = Uuid::from_u128(node);
        let mut config = serde_json::to_value(
            &app.session
                .project()
                .unwrap()
                .graph
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .config,
        )
        .unwrap();
        config[if remote { "hdfs_path" } else { "path" }] = target.clone().into();
        assert!(app.dispatch(Command::ConfigureNode {
            node_id: id,
            config: serde_json::from_value(config).unwrap()
        }));
        run_wait(&mut app, &ctx);
        let result = &app.session.last_execution().unwrap().result;
        if operation == "export" {
            assert!(result.tables.is_empty());
            if !remote {
                assert!(directory.path().join("sales.parquet").is_file());
            }
        } else {
            assert_eq!(result.tables.len(), 1);
            let rows = &result.tables[0].rows;
            assert_eq!(rows.len(), 12);
            assert_eq!(rows[0]["quantity"], 1);
            assert_eq!(rows[11]["amount"], 27);
        }
    }
}
#[test]
fn parquet_run_button_round_trips_a_local_single_file() {
    acceptance(false);
}
#[cfg(feature = "hdfs-integration-tests")]
#[test]
fn parquet_hdfs_run_button_round_trips_a_remote_single_file() {
    acceptance(true);
}
