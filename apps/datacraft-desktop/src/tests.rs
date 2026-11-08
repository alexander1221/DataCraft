use super::*;

fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing visible UI text {label}"))
}

fn click(app: &mut Desktop, ctx: &egui::Context, pos: egui::Pos2) {
    frame(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    frame(
        app,
        ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
}

fn click_label(app: &mut Desktop, ctx: &egui::Context, label: &str) {
    // egui Areas use a sizing pass before a newly opened Modal becomes visible.
    frame(app, ctx, Vec::new());
    let output = frame(app, ctx, Vec::new());
    let pos = text_position(&output, label);
    click(app, ctx, pos);
}

fn edit_string(app: &mut Desktop, ctx: &egui::Context, label: &str, value: &str) {
    frame(app, ctx, Vec::new());
    let output = frame(app, ctx, Vec::new());
    let label = text_position(&output, label);
    click(app, ctx, label + egui::vec2(0.0, 26.0));
    frame(
        app,
        ctx,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            egui::Event::Text(value.into()),
        ],
    );
}

#[test]
fn inspector_input_is_guarded_until_apply_and_undo_restores_config() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.select(Uuid::from_u128(2));
    let original = app.session.project().cloned().unwrap();
    edit_string(&mut app, &ctx, "SQL expression", "quantity >= 5");
    assert!(app.has_unapplied());
    assert!(!app.session.is_dirty());
    assert_eq!(app.session.project(), Some(&original));
    app.select(Uuid::from_u128(3));
    assert_eq!(app.selected, Some(Uuid::from_u128(2)));
    assert_eq!(app.pending_select, Some(Uuid::from_u128(3)));
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/pipelines/sample.dcraft"),
        false,
    );
    app.new_project(false);
    assert_eq!(app.session.project(), Some(&original));
    assert!(app.has_unapplied());
    click_label(&mut app, &ctx, "Keep editing");
    click_label(&mut app, &ctx, "Apply");
    assert!(!app.has_unapplied());
    assert!(app.session.is_dirty());
    assert!(
        matches!(&app.session.project().unwrap().graph.nodes[1].config,datacraft_core::NodeKind::Filter {expression} if expression=="quantity >= 5")
    );
    click_label(&mut app, &ctx, "Undo");
    assert_eq!(app.session.project(), Some(&original));
    assert!(!app.has_unapplied());
    assert!(!app.has_unsaved());
}

#[test]
fn palette_add_delete_and_draft_save_round_trip_use_session_history() {
    let mut app = app();
    app.new_project(true);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, Vec::new());
    click_label(&mut app, &ctx, "Filter");
    assert_eq!(app.session.project().unwrap().graph.nodes.len(), 1);
    let id = app.selected.unwrap();
    assert_eq!(app.editor.as_ref().unwrap().node_id, id);
    assert_eq!(
        app.session
            .project()
            .unwrap()
            .graph
            .diagnostics()
            .unwrap()
            .len(),
        2
    );
    let before = app.session.project().cloned().unwrap();
    click_label(&mut app, &ctx, "Delete node");
    assert!(app.session.project().unwrap().graph.nodes.is_empty());
    click_label(&mut app, &ctx, "Undo");
    assert_eq!(app.session.project(), Some(&before));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.dcraft");
    app.path = path.to_string_lossy().into_owned();
    assert!(app.save(true));
    assert!(!app.has_unsaved());
    app.new_project(false);
    app.open(path, true);
    assert_eq!(app.session.project(), Some(&before));
    assert_eq!(
        app.session
            .project()
            .unwrap()
            .graph
            .diagnostics()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn rejected_cycle_and_incompatible_wire_preserve_graph_and_undo() {
    let mut app = app();
    app.new_project(true);
    app.add_node("filter");
    let a = app.selected.unwrap();
    app.add_node("filter");
    let b = app.selected.unwrap();
    let wire = |from, to| datacraft_core::Edge {
        from,
        to,
        from_port: "table".into(),
        to_port: "table".into(),
    };
    assert!(app.dispatch(Command::Connect { edge: wire(a, b) }));
    let before = app.session.project().cloned();
    assert!(!app.dispatch(Command::Connect { edge: wire(b, a) }));
    assert_eq!(app.session.project(), before.as_ref());
    assert!(app.dispatch(Command::Undo));
    assert!(app.session.project().unwrap().graph.edges.is_empty());
    app.add_node("linear_regression");
    let model = app.selected.unwrap();
    app.add_node("table_preview");
    let preview = app.selected.unwrap();
    let before = app.session.project().cloned();
    assert!(!app.dispatch(Command::Connect {
        edge: datacraft_core::Edge {
            from: model,
            from_port: "model".into(),
            to: preview,
            to_port: "table".into()
        }
    }));
    assert_eq!(app.session.project(), before.as_ref());
}

#[test]
fn invalid_numeric_buffer_cannot_save_or_silently_disappear() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.select(Uuid::from_u128(4));
    let before = app.session.project().cloned();
    edit_string(&mut app, &ctx, "Rows", "not a number");
    assert!(app.has_unapplied());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("must-not-exist.dcraft");
    app.path = path.to_string_lossy().into_owned();
    assert!(!app.save(true));
    assert!(!path.exists());
    assert_eq!(app.session.project(), before.as_ref());
    assert!(app.has_unsaved());
    assert!(!app.active_job());
    click_label(&mut app, &ctx, "Revert");
    assert!(!app.has_unapplied());
    assert_eq!(app.session.project(), before.as_ref());
}

#[test]
fn delete_and_disconnect_buttons_are_undoable_with_incident_edges() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.select(Uuid::from_u128(1));
    let before = app.session.project().cloned().unwrap();
    click_label(&mut app, &ctx, "Disconnect");
    assert_eq!(
        app.session.project().unwrap().graph.edges.len(),
        before.graph.edges.len() - 1
    );
    click_label(&mut app, &ctx, "Undo");
    assert_eq!(app.session.project(), Some(&before));
    click_label(&mut app, &ctx, "Delete node");
    assert_eq!(
        app.session.project().unwrap().graph.nodes.len(),
        before.graph.nodes.len() - 1
    );
    assert!(app
        .session
        .project()
        .unwrap()
        .graph
        .edges
        .iter()
        .all(|e| e.from != Uuid::from_u128(1) && e.to != Uuid::from_u128(1)));
    click_label(&mut app, &ctx, "Undo");
    assert_eq!(app.session.project(), Some(&before));
}

fn frame(app: &mut Desktop, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.render(ctx),
    )
}

#[cfg(feature = "spark-integration-tests")]
#[test]
fn run_button_executes_real_spark_and_preview_is_rendered() {
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
    frame(&mut app, &ctx, Vec::new());
    let output = frame(&mut app, &ctx, Vec::new());
    let button = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Run project" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .expect("run button must be rendered");
    frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(button),
            egui::Event::PointerButton {
                pos: button,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: button,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    assert!(
        app.session.current_job().is_some(),
        "click must dispatch an execution job: {:?}",
        app.error
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while app.active_job() {
        assert!(
            std::time::Instant::now() < deadline,
            "GUI preview acceptance timed out"
        );
        frame(&mut app, &ctx, Vec::new());
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = frame(&mut app, &ctx, Vec::new());
    assert_eq!(
        app.session.current_job().unwrap().status,
        datacraft_engine::jobs::JobStatus::Completed
    );
    let result = app.session.last_execution().expect("completed preview");
    assert_eq!(result.result.tables.len(), 1);
    assert_eq!(
        result.result.tables[0].rows,
        vec![
            serde_json::json!({"quantity":4,"amount":11}),
            serde_json::json!({"quantity":5,"amount":13}),
            serde_json::json!({"quantity":6,"amount":15}),
        ]
    );
    assert!(output.shapes.iter().any(|clipped|matches!(&clipped.shape,egui::Shape::Text(t) if t.galley.job.text.contains("3 rows"))));
    let first_job = app.session.current_job().unwrap().id;
    // The selector response is injected; the next Spark job reads this real Unicode path.
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("данные с пробелами.csv");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/data/sales.csv"),
        &source,
    )
    .unwrap();
    app.select(Uuid::from_u128(1));
    app.handle_file_selection(
        FileSelection {
            request: FileRequest {
                purpose: FilePurpose::Source {
                    node_id: Uuid::from_u128(1),
                    parameter: "path",
                    format: file_dialog::SourceFormat::Csv,
                },
                revision: app.session.revision(),
                initial_path: None,
            },
            path: Some(source),
        },
        &ctx,
    );
    assert!(app.has_unapplied());
    click_label(&mut app, &ctx, "Apply");
    app.select(Uuid::from_u128(2));
    edit_string(&mut app, &ctx, "SQL expression", "quantity >= 5");
    assert!(app.has_unapplied());
    click_label(&mut app, &ctx, "Apply");
    click_label(&mut app, &ctx, "Run project");
    assert_ne!(app.session.current_job().unwrap().id, first_job);
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while app.active_job() {
        assert!(
            std::time::Instant::now() < deadline,
            "edited preview acceptance timed out"
        );
        frame(&mut app, &ctx, Vec::new());
        std::thread::sleep(Duration::from_millis(50));
    }
    frame(&mut app, &ctx, Vec::new());
    assert_eq!(
        app.session.current_job().unwrap().status,
        datacraft_engine::jobs::JobStatus::Completed
    );
    assert_eq!(
        app.session.last_execution().unwrap().result.tables[0].rows,
        vec![
            serde_json::json!({"quantity":5,"amount":13}),
            serde_json::json!({"quantity":6,"amount":15}),
            serde_json::json!({"quantity":7,"amount":17}),
        ]
    );
    app.new_project(true);
    frame(&mut app, &ctx, Vec::new());
    assert!(app.session.current_job().is_none());
    assert!(app.session.last_execution().is_none());
}
fn app() -> Desktop {
    Desktop::new(
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/pipelines/sample.dcraft"),
        ),
        WorkerConfig {
            java: "unavailable-java".into(),
            classpath_file: "unavailable-classpath".into(),
        },
        None,
    )
}

#[cfg(feature = "spark-integration-tests")]
#[test]
fn node_run_button_uses_persistent_inputs_and_displays_success_and_failure() {
    use datacraft_engine::{jobs::JobStatus, node_runtime::NodeStatus};
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
    fn press(app: &mut Desktop, ctx: &egui::Context, label: &str, downstream: bool) {
        frame(app, ctx, vec![]);
        let out = frame(app, ctx, vec![]);
        let node = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r)
                    if r.fill == Color32::from_rgb(35, 43, 57) && out.shapes.iter().any(|s| {
                        matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == label && r.rect.contains(t.pos + t.galley.size()*0.5))
                    }) =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .expect("visible idle node body");
        let mut buttons: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r)
                    if (r.fill == Color32::from_rgb(24, 35, 34)
                        || r.fill == Color32::from_rgb(46, 82, 65))
                        && node.contains(r.rect.center()) =>
                {
                    Some(r.rect.center())
                }
                _ => None,
            })
            .collect();
        buttons.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(buttons.len(), 2);
        click(app, ctx, buttons[usize::from(downstream)]);
    }
    fn wait(app: &mut Desktop, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while app.active_job() {
            assert!(
                std::time::Instant::now() < deadline,
                "node job did not terminate"
            );
            frame(app, ctx, vec![]);
            std::thread::sleep(Duration::from_millis(10));
        }
        frame(app, ctx, vec![]);
    }
    let original = app.session.project().cloned();
    press(&mut app, &ctx, "Filter", false);
    assert!(app
        .error
        .as_ref()
        .is_some_and(|e| e.contains("Run input node")));
    assert!(app.session.current_job().is_none());
    press(&mut app, &ctx, "CSV Source", false);
    wait(&mut app, &ctx);
    assert_eq!(
        app.session.current_job().unwrap().status,
        JobStatus::Completed
    );
    assert_eq!(
        app.session.node_statuses()[&Uuid::from_u128(1)],
        NodeStatus::Completed
    );
    assert_eq!(
        app.session.node_statuses()[&Uuid::from_u128(2)],
        NodeStatus::Idle
    );
    press(&mut app, &ctx, "Filter", true);
    wait(&mut app, &ctx);
    assert_eq!(
        app.session.current_job().unwrap().status,
        JobStatus::Completed
    );
    assert_eq!(
        app.session.last_execution().unwrap().result.tables[0]
            .rows
            .iter()
            .map(|r| r["quantity"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![4, 5, 6]
    );
    assert_eq!(
        app.session.project().cloned(),
        original,
        "execution controls must not move nodes or edit the graph"
    );
    app.dispatch(Command::ConfigureNode {
        node_id: Uuid::from_u128(2),
        config: datacraft_core::NodeKind::Filter {
            expression: "absent > 0".into(),
        },
    });
    press(&mut app, &ctx, "Filter", false);
    wait(&mut app, &ctx);
    assert_eq!(
        app.session.node_statuses()[&Uuid::from_u128(2)],
        NodeStatus::Failed
    );
    let out = frame(&mut app, &ctx, vec![]);
    assert!(out.shapes.iter().any(
        |s| matches!(&s.shape,egui::Shape::Rect(r) if r.stroke.color==Color32::from_rgb(245,91,99))
    ));
}
#[test]
fn canvas_and_inspector_render_without_java_or_project_mutation() {
    let mut app = app();
    assert!(app.error.is_none());
    let before = app.session.project().cloned();
    let ctx = egui::Context::default();
    app.selected = Some(Uuid::from_u128(2));
    for _ in 0..3 {
        let output = frame(&mut app, &ctx, Vec::new());
        assert!(!output.shapes.is_empty());
        let path_label = output
            .shapes
            .iter()
            .find_map(|clipped| {
                if let egui::Shape::Text(text) = &clipped.shape {
                    if text.galley.job.text == "Project path" {
                        return Some((
                            clipped.clip_rect,
                            egui::Rect::from_min_size(text.pos, text.galley.size()),
                        ));
                    }
                }
                None
            })
            .expect("project path control must be painted");
        assert!(
            path_label.0.contains_rect(path_label.1),
            "project path row must be visible inside toolbar"
        );
    }
    assert_eq!(app.session.project(), before.as_ref());
    assert!(!app.session.is_dirty());
    assert!(app.session.current_job().is_none());
}
#[test]
fn failed_open_preserves_loaded_project_selection_and_view() {
    let mut app = app();
    let before = app.session.project().cloned();
    app.selected = Some(Uuid::from_u128(2));
    app.canvas.fit = false;
    app.open(PathBuf::from("missing-desktop-fixture.dcraft"), false);
    assert!(app.error.is_some());
    assert_eq!(app.session.project(), before.as_ref());
    assert_eq!(app.selected, Some(Uuid::from_u128(2)));
    assert!(!app.canvas.fit);
}

#[cfg(feature = "spark-integration-tests")]
#[test]
fn regression_metrics_run_button_displays_real_test_metrics_and_undefined_r2() {
    let mut app = app();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/random_forest.dcraft"),
        true,
    );
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
    for single in [false, true] {
        if single {
            app.dispatch(Command::ConfigureNode {
                node_id: Uuid::from_u128(3),
                config: datacraft_core::NodeKind::Filter {
                    expression: "id = 5".into(),
                },
            });
        }
        click_label(&mut app, &ctx, "Run project");
        assert!(
            app.active_job(),
            "Run must create a real job: {:?}",
            app.error
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while app.active_job() {
            assert!(
                std::time::Instant::now() < deadline,
                "regression UI job timed out"
            );
            frame(&mut app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(50));
        }
        let output = frame(&mut app, &ctx, Vec::new());
        assert_eq!(
            app.session.current_job().unwrap().status,
            datacraft_engine::jobs::JobStatus::Completed
        );
        let table = &app.session.last_execution().unwrap().result.tables[0];
        assert_eq!(table.name, "Regression metrics");
        assert_eq!(table.rows.len(), 4);
        assert_eq!(table.schema["fields"][1]["type"], "double");
        assert_eq!(table.rows[0]["value"].as_f64(), Some(0.0));
        if single {
            assert!(table.rows[3]["value"].is_null());
            text_position(&output, "null");
        } else {
            assert_eq!(table.rows[3]["value"].as_f64(), Some(1.0));
        }
        text_position(&output, "rmse");
        assert!(app.session.result_is_current());
    }
}

mod file_dialogs;

#[cfg(feature = "spark-integration-tests")]
#[test]
fn selection_run_button_displays_importance_and_real_transformed_test_vectors() {
    let mut app = app();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/random_forest_pipeline.dcraft"),
        true,
    );
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
    let wait = |app: &mut Desktop| {
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while app.active_job() {
            assert!(
                std::time::Instant::now() < deadline,
                "selection GUI job timed out"
            );
            frame(app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(50));
        }
        frame(app, &ctx, Vec::new());
        assert_eq!(
            app.session.current_job().unwrap().status,
            datacraft_engine::jobs::JobStatus::Completed,
            "{:?}",
            app.error
        );
    };
    click_label(&mut app, &ctx, "Run project");
    assert!(app.active_job());
    wait(&mut app);
    let table = &app.session.last_execution().unwrap().result.tables[0];
    assert_eq!(table.name, "Feature selection");
    assert_eq!(
        table
            .rows
            .iter()
            .map(|r| r["importance"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![0.0, 1.0]
    );
    assert_eq!(table.rows[1]["selected"], true);
    let output = frame(&mut app, &ctx, Vec::new());
    text_position(&output, "signal");
    app.select(Uuid::from_u128(8));
    click_label(&mut app, &ctx, "Run selected");
    assert!(app.active_job());
    wait(&mut app);
    let table = &app.session.last_execution().unwrap().result.tables[0];
    assert_eq!(table.rows.len(), 2);
    assert_eq!(
        table
            .rows
            .iter()
            .map(|r| r["selected_features"]["values"][0].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![1.0, 7.0]
    );
    let selected = table.schema["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "selected_features")
        .unwrap();
    assert_eq!(selected["metadata"]["ml_attr"]["num_attrs"], 1);
    assert!(app.session.result_is_current());
}

#[test]
fn arrange_button_moves_branches_as_one_undo_and_leaves_computation_untouched() {
    let mut app = app();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/logistic_regression.dcraft"),
        true,
    );
    let commands = app
        .session
        .project()
        .unwrap()
        .graph
        .nodes
        .iter()
        .map(|node| Command::MoveNode {
            node_id: node.id,
            position: [0.0, 0.0],
        })
        .collect();
    app.dispatch(Command::Batch { commands });
    let before = app.session.project().cloned().unwrap();
    let ctx = egui::Context::default();
    click_label(&mut app, &ctx, "Arrange");
    let after = app.session.project().cloned().unwrap();
    assert_ne!(before, after);
    assert_eq!(before.graph.edges, after.graph.edges);
    for (a, b) in before.graph.nodes.iter().zip(&after.graph.nodes) {
        assert_eq!(a.config, b.config);
    }
    click_label(&mut app, &ctx, "Undo");
    assert_eq!(app.session.project(), Some(&before));
    click_label(&mut app, &ctx, "Redo");
    assert_eq!(app.session.project(), Some(&after));
}

#[cfg(feature = "spark-integration-tests")]
#[test]
fn readability_run_button_preserves_results_for_layout_and_separates_negative_values() {
    let mut app = app();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/logistic_regression.dcraft"),
        true,
    );
    app.session.configure_worker(WorkerConfig {
        java: std::env::var_os("DATACRAFT_JAVA").map_or_else(|| "java".into(), PathBuf::from),
        classpath_file: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../workers/spark/target/classpath.txt"),
    });
    let ctx = egui::Context::default();
    app.select(Uuid::from_u128(9));
    click_label(&mut app, &ctx, "Run selected");
    assert!(app.active_job());
    app.dispatch(Command::MoveNode {
        node_id: Uuid::from_u128(1),
        position: [33.0, 77.0],
    });
    app.dispatch(Command::Rename {
        name: "Moved during run".into(),
    });
    let wait = |app: &mut Desktop| {
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while app.active_job() {
            assert!(std::time::Instant::now() < deadline, "preview timed out");
            frame(app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(50));
        }
        frame(app, &ctx, Vec::new());
        assert_eq!(
            app.session.current_job().unwrap().status,
            datacraft_engine::jobs::JobStatus::Completed
        );
    };
    wait(&mut app);
    assert!(app.session.result_is_current());
    app.dispatch(Command::MoveNode {
        node_id: Uuid::from_u128(2),
        position: [88.0, 99.0],
    });
    assert!(app.session.result_is_current());
    click_label(&mut app, &ctx, "Undo");
    assert!(app.session.result_is_current());
    let output = frame(&mut app, &ctx, Vec::new());
    let text_rect = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing table cell {label}"))
    };
    let id = text_rect("9");
    let x = text_rect("-2.5");
    assert!((id.top() - x.top()).abs() < 2.0);
    assert!(
        x.left() - id.right() >= 20.0,
        "id and negative x must have visible separation"
    );
    assert!(!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text=="Pipeline changed since this result was computed")));
    app.dispatch(Command::ConfigureNode {
        node_id: Uuid::from_u128(2),
        config: datacraft_core::NodeKind::Filter {
            expression: "id <= 7".into(),
        },
    });
    assert!(!app.session.result_is_current());
    click_label(&mut app, &ctx, "Undo");
    assert!(!app.session.result_is_current());
    click_label(&mut app, &ctx, "Run selected");
    wait(&mut app);
    assert!(
        app.session.result_is_current(),
        "new run must capture the computation epoch after prior visual edits"
    );
}

#[cfg(feature = "spark-integration-tests")]
#[test]
fn classification_metrics_run_button_displays_real_held_out_results() {
    let mut app = app();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/pipelines/logistic_regression.dcraft"),
        true,
    );
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
    click_label(&mut app, &ctx, "Run project");
    assert!(
        app.active_job(),
        "Run must create a real job: {:?}",
        app.error
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while app.active_job() {
        assert!(
            std::time::Instant::now() < deadline,
            "classification GUI job timed out"
        );
        frame(&mut app, &ctx, Vec::new());
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = frame(&mut app, &ctx, Vec::new());
    assert_eq!(
        app.session.current_job().unwrap().status,
        datacraft_engine::jobs::JobStatus::Completed
    );
    let table = &app.session.last_execution().unwrap().result.tables[0];
    assert_eq!(table.name, "Classification metrics");
    assert_eq!(table.schema["fields"][1]["type"], "double");
    assert_eq!(table.rows.len(), 4);
    for row in &table.rows {
        assert_eq!(row["value"].as_f64(), Some(1.0));
    }
    text_position(&output, "accuracy");
    text_position(&output, "weighted_f1");
    assert!(app.session.result_is_current());
}

#[test]
fn preprocessing_graph_renders_split_ports_and_generated_inspectors_without_java() {
    let mut app = app();
    let ctx = egui::Context::default();
    for name in [
        "train_test",
        "reference_code",
        "design_matrix",
        "index_categoricals",
        "glm_regression",
        "decision_tree",
        "random_forest",
        "logistic_regression",
        "linear_svc",
        "decision_tree_classifier",
        "random_forest_classifier",
        "feature_selection",
        "feature_selection_pipeline",
        "random_forest_pipeline",
    ] {
        app.open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../examples/pipelines/{name}.dcraft")),
            true,
        );
        assert!(app.error.is_none());
        let before = app.session.project().cloned().unwrap();
        before.validate().unwrap();
        for node in &before.graph.nodes {
            app.select(node.id);
            assert_eq!(app.editor.as_ref().unwrap().config().unwrap(), node.config);
            let output = frame(&mut app, &ctx, Vec::new());
            assert!(!output.shapes.is_empty());
            if matches!(node.config, datacraft_core::NodeKind::TrainTestSplit { .. }) {
                text_position(&output, "train");
                text_position(&output, "test");
            }
        }
        assert_eq!(app.session.project(), Some(&before));
        assert!(!app.session.is_dirty());
        assert!(app.session.current_job().is_none());
    }
}
