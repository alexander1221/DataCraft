//! Adapter tests inject selector replies; they do not open or certify OS dialogs.
use super::*;
use crate::file_dialog::SourceFormat;

#[test]
fn parent_cannot_close_while_a_picker_or_rename_is_open() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.request_file(FilePurpose::OpenProject, None);
    app.request_close(&ctx);
    assert!(!app.allow_close && !app.closing);
    assert!(app.file_dialogs.is_busy());
    answer(&mut app, &ctx, None);
    app.begin_rename();
    app.request_close(&ctx);
    assert!(!app.allow_close && !app.closing);
    assert!(app.rename.is_some());
    app.rename = None;
    app.request_close(&ctx);
    assert!(app.allow_close);
}

#[test]
fn picked_project_uses_the_existing_unsaved_buffer_guard_before_opening() {
    let mut app = app();
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chosen.dcraft");
    std::fs::write(
        &path,
        r#"{"format_version":1,"name":"Chosen project","nodes":[],"edges":[],"output_node":null}"#,
    )
    .unwrap();
    app.select(Uuid::from_u128(1));
    app.editor
        .as_mut()
        .unwrap()
        .set_local_path("path", &dir.path().join("unsaved.csv"))
        .unwrap();
    let before = app.session.project().cloned();
    click_label(&mut app, &ctx, "Open…");
    answer(&mut app, &ctx, Some(path.clone()));
    assert_eq!(app.pending_open, Some(path.clone()));
    assert_eq!(app.session.project(), before.as_ref());
    click_label(&mut app, &ctx, "Keep editing");
    assert!(app.pending_open.is_none());
    assert!(app.has_unapplied());
    click_label(&mut app, &ctx, "Open…");
    answer(&mut app, &ctx, Some(path.clone()));
    click_label(&mut app, &ctx, "Discard and continue");
    assert_eq!(app.session.project().unwrap().name, "Chosen project");
    assert_eq!(
        app.session.path(),
        Some(path.canonicalize().unwrap().as_path())
    );
    assert!(!app.has_unsaved());
    assert!(app.editor.is_none());
}

fn answer(app: &mut Desktop, ctx: &egui::Context, path: Option<PathBuf>) {
    let request = app
        .file_dialogs
        .take_queued_for_test()
        .expect("picker request");
    app.handle_file_selection(FileSelection { request, path }, ctx);
}

#[test]
fn rename_widget_is_undoable_preserves_path_and_rejects_empty_name() {
    let mut app = app();
    let ctx = egui::Context::default();
    let before = app.session.project().cloned().unwrap();
    let path = app.session.path().map(std::path::Path::to_owned);
    click_label(&mut app, &ctx, "Rename project…");
    edit_string(&mut app, &ctx, "Project name", "Проект продаж");
    click_label(&mut app, &ctx, "Rename");
    assert!(app.rename.is_none());
    assert_eq!(app.session.project().unwrap().name, "Проект продаж");
    assert_eq!(app.session.project().unwrap().graph, before.graph);
    assert_eq!(app.session.path(), path.as_deref());
    assert!(app.session.is_dirty());
    click_label(&mut app, &ctx, "Undo");
    assert_eq!(app.session.project(), Some(&before));
    click_label(&mut app, &ctx, "Redo");
    assert_eq!(app.session.project().unwrap().name, "Проект продаж");
    let revision = app.session.revision();
    click_label(&mut app, &ctx, "Rename project…");
    edit_string(&mut app, &ctx, "Project name", "   ");
    click_label(&mut app, &ctx, "Rename");
    assert!(app.error.is_some());
    assert!(app.rename.is_some());
    assert_eq!(app.session.revision(), revision);
    assert_eq!(app.session.project().unwrap().name, "Проект продаж");
    click_label(&mut app, &ctx, "Cancel rename");
    assert!(app.rename.is_none());
    app.begin_rename();
    assert!(app.apply_rename());
    assert_eq!(
        app.session.revision(),
        revision,
        "renaming to the same name must be a no-op"
    );
}

#[test]
fn cancelling_every_picker_preserves_project_buffers_history_and_close_state() {
    for purpose in [
        FilePurpose::OpenProject,
        FilePurpose::SaveProject { close_after: false },
        FilePurpose::SaveProject { close_after: true },
        FilePurpose::Source {
            node_id: Uuid::from_u128(1),
            parameter: "path",
            format: SourceFormat::Csv,
        },
    ] {
        let mut app = app();
        let ctx = egui::Context::default();
        app.select(Uuid::from_u128(1));
        let dir = tempfile::tempdir().unwrap();
        app.editor
            .as_mut()
            .unwrap()
            .set_local_path("path", &dir.path().join("unsaved.csv"))
            .unwrap();
        let buffer = app.editor.as_ref().unwrap().config().unwrap();
        let before = app.session.project().cloned();
        let revision = app.session.revision();
        let path = app.path.clone();
        app.request_file(purpose, None);
        assert!(app.file_dialogs.is_busy());
        answer(&mut app, &ctx, None);
        assert!(!app.file_dialogs.is_busy());
        assert_eq!(app.session.project(), before.as_ref());
        assert_eq!(app.session.revision(), revision);
        assert!(!app.session.can_undo());
        assert_eq!(app.editor.as_ref().unwrap().config().unwrap(), buffer);
        assert!(app.has_unapplied());
        assert_eq!(app.path, path);
        assert!(!app.allow_close && !app.closing);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[test]
fn browse_uses_source_format_and_only_changes_the_target_inspector_buffer() {
    for (kind, format) in [
        ("csv_source", SourceFormat::Csv),
        ("parquet_source", SourceFormat::Parquet),
        ("json_source", SourceFormat::Json),
    ] {
        let mut app = app();
        let ctx = egui::Context::default();
        app.add_node(kind);
        let node = app.selected.unwrap();
        let before = app.session.project().cloned();
        click_label(&mut app, &ctx, "Browse…");
        let request = app.file_dialogs.take_queued_for_test().unwrap();
        assert_eq!(
            request.purpose,
            FilePurpose::Source {
                node_id: node,
                parameter: "path",
                format
            }
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(match format {
            SourceFormat::Csv => "данные с пробелами.csv",
            SourceFormat::Parquet => "future-dataset.parquet",
            SourceFormat::Json => "данные с пробелами.jsonl",
        });
        app.handle_file_selection(
            FileSelection {
                request,
                path: Some(path.clone()),
            },
            &ctx,
        );
        assert_eq!(app.session.project(), before.as_ref());
        assert_eq!(
            app.editor.as_ref().unwrap().local_path("path"),
            path.to_str()
        );
        assert!(app.has_unapplied());
        assert!(app.apply_parameters());
        let config = &app
            .session
            .project()
            .unwrap()
            .graph
            .nodes
            .iter()
            .find(|n| n.id == node)
            .unwrap()
            .config;
        assert_eq!(
            serde_json::to_value(config).unwrap()["path"],
            path.to_str().unwrap()
        );
    }
}

#[test]
fn stale_revision_and_wrong_node_results_cannot_change_another_project_or_field() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.select(Uuid::from_u128(1));
    app.request_file(
        FilePurpose::Source {
            node_id: Uuid::from_u128(1),
            parameter: "path",
            format: SourceFormat::Csv,
        },
        None,
    );
    let request = app.file_dialogs.take_queued_for_test().unwrap();
    app.dispatch(Command::Rename {
        name: "Changed while choosing".into(),
    });
    let before = app.session.project().cloned();
    let buffer = app.editor.as_ref().unwrap().config().unwrap();
    let dir = tempfile::tempdir().unwrap();
    app.handle_file_selection(
        FileSelection {
            request,
            path: Some(dir.path().join("data.csv")),
        },
        &ctx,
    );
    assert!(app.error.as_ref().unwrap().contains("Project changed"));
    assert_eq!(app.session.project(), before.as_ref());
    assert_eq!(app.editor.as_ref().unwrap().config().unwrap(), buffer);
    app.request_file(
        FilePurpose::Source {
            node_id: Uuid::from_u128(1),
            parameter: "path",
            format: SourceFormat::Csv,
        },
        None,
    );
    let request = app.file_dialogs.take_queued_for_test().unwrap();
    app.set_selection(Uuid::from_u128(2));
    app.handle_file_selection(
        FileSelection {
            request,
            path: Some(dir.path().join("data.csv")),
        },
        &ctx,
    );
    assert!(app.error.as_ref().unwrap().contains("another node"));
    assert!(!app.has_unapplied());
    assert_eq!(app.session.project(), before.as_ref());
    app.begin_rename();
    app.dispatch(Command::Rename {
        name: "External name".into(),
    });
    assert!(!app.apply_rename());
    assert_eq!(app.session.project().unwrap().name, "External name");
}

#[test]
fn save_picker_adds_extension_preserves_source_meaning_and_protects_future_versions() {
    let mut app = app();
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().unwrap();
    app.save_dialog(false);
    let destination = dir.path().join("копия проекта");
    answer(&mut app, &ctx, Some(destination));
    let destination = dir.path().join("копия проекта.dcraft");
    assert_eq!(
        app.session.path(),
        Some(destination.canonicalize().unwrap().as_path())
    );
    assert!(!app.session.is_dirty());
    let resolved = app
        .session
        .project()
        .unwrap()
        .resolved(&destination)
        .unwrap();
    let datacraft_core::NodeKind::CsvSource { path, .. } = &resolved.graph.nodes[0].config else {
        panic!("CSV source");
    };
    let expected = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/data/sales.csv")
        .canonicalize()
        .unwrap();
    assert_eq!(PathBuf::from(path).canonicalize().unwrap(), expected);
    let before = app.session.project().cloned();
    app.open(destination, false);
    assert_eq!(app.session.project(), before.as_ref());
    let unsupported = dir.path().join("future.dcraft");
    let contents = "{\"format_version\":99}";
    std::fs::write(&unsupported, contents).unwrap();
    app.save_dialog(false);
    answer(&mut app, &ctx, Some(unsupported.clone()));
    assert!(app.error.is_some());
    assert_eq!(std::fs::read_to_string(unsupported).unwrap(), contents);
    assert_eq!(app.session.project(), before.as_ref());
}

#[test]
fn save_and_close_for_new_project_waits_for_success_and_cancel_keeps_window_open() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.new_project(true);
    app.close_dialog = true;
    click_label(&mut app, &ctx, "Save and close");
    let request = app.file_dialogs.take_queued_for_test().unwrap();
    assert_eq!(
        request.purpose,
        FilePurpose::SaveProject { close_after: true }
    );
    app.handle_file_selection(
        FileSelection {
            request,
            path: None,
        },
        &ctx,
    );
    assert!(!app.allow_close && !app.closing);
    assert!(app.session.is_dirty());
    assert!(app.session.path().is_none());
    let dir = tempfile::tempdir().unwrap();
    app.save_dialog(true);
    answer(&mut app, &ctx, Some(dir.path().join("new-project.dcraft")));
    assert!(app.allow_close);
    assert!(!app.session.is_dirty());
    assert!(dir.path().join("new-project.dcraft").is_file());
}
