use super::*;

#[test]
fn hdfs_run_button_exports_imports_and_remote_picker_preserves_the_project_and_results() {
    use datacraft_engine::jobs::JobStatus;
    let root = format!("/datacraft-gui/{}", Uuid::new_v4());
    let target = format!("{root}/sales.tsv");
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
    let ctx = egui::Context::default();
    fn wait(app: &mut Desktop, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while app.active_job() {
            assert!(
                std::time::Instant::now() < deadline,
                "HDFS GUI job timed out: {:?}",
                app.error
            );
            frame(app, ctx, vec![]);
            std::thread::sleep(Duration::from_millis(10));
        }
        frame(app, ctx, vec![]);
        assert_eq!(
            app.session.current_job().unwrap().status,
            JobStatus::Completed,
            "{:?}",
            app.error
        );
    }
    fn set_path(app: &mut Desktop, id: u128, target: &str) {
        let mut config = serde_json::to_value(
            &app.session
                .project()
                .unwrap()
                .graph
                .nodes
                .iter()
                .find(|n| n.id == Uuid::from_u128(id))
                .unwrap()
                .config,
        )
        .unwrap();
        config["hdfs_path"] = target.into();
        if let Ok(host) = std::env::var("DATACRAFT_HDFS_HOST") {
            config["host"] = host.into();
        }
        if let Ok(port) = std::env::var("DATACRAFT_HDFS_PORT") {
            config["port"] = port.parse::<u32>().unwrap().into();
        }
        assert!(app.dispatch(Command::ConfigureNode {
            node_id: Uuid::from_u128(id),
            config: serde_json::from_value(config).unwrap()
        }));
    }
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/hdfs_export_text.dcraft"),
        true,
    );
    set_path(&mut app, 2, &target);
    click_label(&mut app, &ctx, "Run project");
    wait(&mut app, &ctx);
    assert!(app
        .session
        .last_execution()
        .unwrap()
        .result
        .tables
        .is_empty());
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/hdfs_import_text.dcraft"),
        true,
    );
    set_path(&mut app, 1, &target);
    app.select(Uuid::from_u128(1));
    click_label(&mut app, &ctx, "Run project");
    wait(&mut app, &ctx);
    let rows = app.session.last_execution().unwrap().result.tables[0]
        .rows
        .clone();
    assert_eq!(rows.len(), 12);
    assert_eq!(rows[0]["quantity"], 1);
    assert_eq!(rows[11]["amount"], 27);
    let project = app.session.project().cloned().unwrap();
    let revision = app.session.revision();
    app.editor
        .as_mut()
        .unwrap()
        .set_hdfs_path("/unapplied-placeholder.tsv".into())
        .unwrap();
    assert!(app.has_unapplied());
    click_label(&mut app, &ctx, "Browse HDFS…");
    wait(&mut app, &ctx);
    assert!(app.hdfs_browser.is_some());
    edit_string(&mut app, &ctx, "Directory or file path", &root);
    click_label(&mut app, &ctx, "List");
    wait(&mut app, &ctx);
    let out = frame(&mut app, &ctx, vec![]);
    let file = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(text) if text.galley.job.text.starts_with(&target) => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .expect("remote HDFS file must be visible in the picker");
    click(&mut app, &ctx, file);
    click_label(&mut app, &ctx, "Use path");
    assert!(app.hdfs_browser.is_none());
    assert!(!app.has_unapplied());
    assert_eq!(app.session.project(), Some(&project));
    assert_eq!(app.session.revision(), revision);
    assert_eq!(
        app.session.last_execution().unwrap().result.tables[0].rows,
        rows
    );
    println!("Real GUI HDFS demo retained at {target}");
}
