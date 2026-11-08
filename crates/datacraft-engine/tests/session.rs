use datacraft_core::{Edge, Node, NodeKind};
use datacraft_engine::{Command, Session, SessionError, WorkerConfig};
use datacraft_project::Project;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn sample() -> Project {
    Project::from_json(include_str!("../../../examples/pipelines/sample.dcraft")).unwrap()
}
fn session() -> Session {
    Session::from_project(sample(), 100).unwrap()
}
fn snapshot(s: &Session) -> (Option<Project>, u64, bool, bool, bool, Option<PathBuf>) {
    (
        s.project().cloned(),
        s.revision(),
        s.is_dirty(),
        s.can_undo(),
        s.can_redo(),
        s.path().map(Path::to_path_buf),
    )
}
fn rejects_without_change(s: &mut Session, c: Command) {
    let before = snapshot(s);
    assert!(s.dispatch(c).is_err());
    assert_eq!(snapshot(s), before);
}
fn move_node() -> Command {
    Command::MoveNode {
        node_id: id(1),
        position: [42.0, 12.0],
    }
}

#[test]
fn add_remove_move_configure_and_restore_exact_project() {
    let mut s = session();
    let original = s.project().unwrap().clone();
    let node = Node {
        id: id(9),
        position: [0.0, 0.0],
        config: NodeKind::Filter {
            expression: String::new(),
        },
    };
    let result = s.dispatch(Command::AddNode { node: node.clone() }).unwrap();
    assert!(result.changed && result.dirty);
    assert_eq!(s.project().unwrap().graph.nodes.len(), 6);
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &original);
    s.dispatch(Command::Redo).unwrap();
    assert_eq!(s.project().unwrap().graph.nodes.last(), Some(&node));
    s.dispatch(Command::RemoveNode { node_id: id(9) }).unwrap();
    assert_eq!(s.project().unwrap(), &original);
    s.dispatch(move_node()).unwrap();
    assert_eq!(s.project().unwrap().graph.nodes[0].position, [42.0, 12.0]);
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &original);
    s.dispatch(Command::ConfigureNode {
        node_id: id(2),
        config: NodeKind::Filter {
            expression: "quantity > 8".into(),
        },
    })
    .unwrap();
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &original);
}

#[test]
fn removal_restores_incident_edges_and_designated_output() {
    let mut s = session();
    let original = sample();
    s.dispatch(Command::RemoveNode { node_id: id(5) }).unwrap();
    let project = s.project().unwrap();
    assert_eq!(project.output_node, None);
    assert_eq!(project.graph.edges.len(), 3);
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &original);
    s.dispatch(Command::RemoveNode { node_id: id(1) }).unwrap();
    assert!(s
        .project()
        .unwrap()
        .graph
        .edges
        .iter()
        .all(|e| e.from != id(1) && e.to != id(1)));
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &original);
}

#[test]
fn connection_commands_and_failed_validation_are_atomic() {
    let mut s = session();
    let edge = sample().graph.edges[0].clone();
    rejects_without_change(&mut s, Command::Connect { edge: edge.clone() });
    s.dispatch(Command::Disconnect { edge: edge.clone() })
        .unwrap();
    rejects_without_change(&mut s, Command::Disconnect { edge: edge.clone() });
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &sample());
    s.dispatch(Command::Redo).unwrap();
    s.dispatch(Command::Connect { edge }).unwrap();
    assert_eq!(s.project().unwrap(), &sample());
    rejects_without_change(
        &mut s,
        Command::Connect {
            edge: Edge {
                from: id(4),
                from_port: "table".into(),
                to: id(2),
                to_port: "table".into(),
            },
        },
    );
    rejects_without_change(
        &mut s,
        Command::ConfigureNode {
            node_id: id(2),
            config: NodeKind::ParquetSource {
                path: String::new(),
            },
        },
    );
}

#[test]
fn invalid_edits_preserve_state_history_and_redo() {
    let mut s = session();
    s.dispatch(move_node()).unwrap();
    s.dispatch(Command::Undo).unwrap();
    rejects_without_change(
        &mut s,
        Command::MoveNode {
            node_id: id(1),
            position: [f32::NAN, 0.0],
        },
    );
    rejects_without_change(
        &mut s,
        Command::MoveNode {
            node_id: id(99),
            position: [0.0, 0.0],
        },
    );
    rejects_without_change(
        &mut s,
        Command::AddNode {
            node: sample().graph.nodes[0].clone(),
        },
    );
    rejects_without_change(&mut s, Command::Rename { name: " ".into() });
    rejects_without_change(
        &mut s,
        Command::SetOutput {
            output: Some(id(1)),
        },
    );
    let before = snapshot(&s);
    let result = s
        .dispatch(Command::MoveNode {
            node_id: id(1),
            position: [0.0, 0.0],
        })
        .unwrap();
    assert!(!result.changed);
    assert!(result.events.is_empty());
    assert_eq!(snapshot(&s), before);
    s.dispatch(Command::Redo).unwrap();
}

#[test]
fn batch_is_one_undo_and_rejects_lifecycle_even_when_nested() {
    let mut s = session();
    let original = sample();
    s.dispatch(Command::Batch {
        commands: vec![
            move_node(),
            Command::Rename {
                name: "Changed".into(),
            },
        ],
    })
    .unwrap();
    assert_eq!(s.revision(), 2);
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap(), &original);
    assert!(!s.can_undo());
    rejects_without_change(
        &mut s,
        Command::Batch {
            commands: vec![
                Command::Rename {
                    name: "Wrong".into(),
                },
                Command::Batch {
                    commands: vec![Command::Save],
                },
            ],
        },
    );
    rejects_without_change(
        &mut s,
        Command::Batch {
            commands: vec![move_node(), Command::RemoveNode { node_id: id(99) }],
        },
    );
}

#[test]
fn revisions_increase_and_new_edits_clear_redo() {
    let mut s = session();
    for (command, revision) in [
        (move_node(), 2),
        (Command::Undo, 3),
        (Command::Redo, 4),
        (Command::Undo, 5),
    ] {
        assert_eq!(s.dispatch(command).unwrap().revision, revision);
    }
    s.dispatch(Command::Rename {
        name: "Different branch".into(),
    })
    .unwrap();
    assert!(!s.can_redo());
    rejects_without_change(&mut s, Command::Redo);
}

#[test]
fn history_limit_is_enforced_and_zero_disables_history() {
    let mut s = Session::from_project(sample(), 1).unwrap();
    s.dispatch(Command::Rename { name: "A".into() }).unwrap();
    s.dispatch(Command::Rename { name: "B".into() }).unwrap();
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap().name, "A");
    assert!(!s.can_undo());
    s.dispatch(Command::Redo).unwrap();
    assert_eq!(s.project().unwrap().name, "B");
    let mut s = Session::from_project(sample(), 0).unwrap();
    s.dispatch(move_node()).unwrap();
    assert!(!s.can_undo());
}

#[test]
fn save_checkpoint_tracks_content_not_revision_and_save_is_not_undoable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.dcraft");
    let mut s = session();
    assert!(s.is_dirty());
    s.dispatch(Command::SaveAs { path }).unwrap();
    assert!(!s.is_dirty());
    assert!(!s.can_undo());
    s.dispatch(move_node()).unwrap();
    assert!(s.is_dirty());
    s.dispatch(Command::Undo).unwrap();
    assert!(!s.is_dirty());
    s.dispatch(Command::Redo).unwrap();
    s.dispatch(Command::Save).unwrap();
    assert!(!s.is_dirty());
    s.dispatch(Command::Undo).unwrap();
    assert!(s.is_dirty());
    s.dispatch(Command::Redo).unwrap();
    assert!(!s.is_dirty());
    assert!(s.revision() > 2);
}

#[test]
fn output_changes_are_undoable_and_can_be_cleared() {
    let mut s = session();
    s.dispatch(Command::SetOutput { output: None }).unwrap();
    assert_eq!(s.project().unwrap().output_node, None);
    s.dispatch(Command::Undo).unwrap();
    assert_eq!(s.project().unwrap().output_node, Some(id(5)));
}

#[test]
fn open_protects_unsaved_changes_and_failures_keep_previous_state() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.dcraft");
    sample().save(&good).unwrap();
    let mut s = session();
    rejects_without_change(
        &mut s,
        Command::Open {
            path: good.clone(),
            discard_changes: false,
        },
    );
    s.dispatch(move_node()).unwrap();
    let bad = dir.path().join("future.dcraft");
    std::fs::write(&bad, r#"{"format_version":999}"#).unwrap();
    rejects_without_change(
        &mut s,
        Command::Open {
            path: bad.clone(),
            discard_changes: true,
        },
    );
    rejects_without_change(&mut s, Command::SaveAs { path: bad.clone() });
    rejects_without_change(
        &mut s,
        Command::SaveAs {
            path: dir.path().join("absent/project.dcraft"),
        },
    );
    assert_eq!(
        std::fs::read_to_string(&bad).unwrap(),
        r#"{"format_version":999}"#
    );
    s.dispatch(Command::Open {
        path: good,
        discard_changes: true,
    })
    .unwrap();
    assert!(!s.is_dirty());
    assert!(!s.can_undo());
    assert!(!s.can_redo());
}

#[test]
fn save_as_rebases_current_and_history_without_retargeting_sources() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(&new).unwrap();
    std::fs::write(old.join("sales.csv"), "quantity,amount\n1,5\n").unwrap();
    let mut project = sample();
    if let NodeKind::CsvSource { path, .. } = &mut project.graph.nodes[0].config {
        *path = "sales.csv".into();
    }
    let initial = old.join("project.dcraft");
    project.save(&initial).unwrap();
    let mut s = Session::default();
    s.dispatch(Command::Open {
        path: initial,
        discard_changes: false,
    })
    .unwrap();
    s.dispatch(move_node()).unwrap();
    s.dispatch(Command::Undo).unwrap();
    let next = new.join("project.dcraft");
    s.dispatch(Command::SaveAs { path: next.clone() }).unwrap();
    assert!(!s.is_dirty());
    assert!(s.can_redo());
    for command in [None, Some(Command::Redo), Some(Command::Undo)] {
        if let Some(command) = command {
            s.dispatch(command).unwrap();
        }
        let runtime = s.project().unwrap().resolved(&next).unwrap();
        let NodeKind::CsvSource { path, .. } = &runtime.graph.nodes[0].config else {
            panic!("expected CSV")
        };
        assert_eq!(
            Path::new(path).canonicalize().unwrap(),
            old.join("sales.csv").canonicalize().unwrap()
        );
    }
    assert_eq!(Project::load(&next).unwrap(), *s.project().unwrap());
}

#[test]
fn availability_and_preflight_do_not_launch_java_or_edit_history() {
    let mut s = Session::default();
    assert!(matches!(
        s.dispatch(Command::Cancel { job_id: id(99) }),
        Err(SessionError::Job(
            datacraft_engine::jobs::JobError::UnknownJob(_)
        ))
    ));
    assert!(matches!(
        s.dispatch(Command::JobStatus { job_id: id(99) }),
        Err(SessionError::Job(
            datacraft_engine::jobs::JobError::UnknownJob(_)
        ))
    ));
    assert!(!s.availability(&Command::Undo).enabled);
    rejects_without_change(&mut s, Command::Undo);
    let mut s = session();
    rejects_without_change(&mut s, Command::Save);
    rejects_without_change(&mut s, Command::Run { output: None });
    s.configure_worker(WorkerConfig {
        java: "absent-java".into(),
        classpath_file: "absent-classpath".into(),
    });
    assert!(matches!(
        s.dispatch(Command::Run { output: None }),
        Err(SessionError::RelativeSourceWithoutProject)
    ));
    assert!(!s.can_undo());
    assert!(s.last_execution().is_none());
}

#[test]
fn canonical_noop_preserves_checkpoint_and_nested_batch_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = session();
    s.dispatch(Command::SaveAs {
        path: dir.path().join("project.dcraft"),
    })
    .unwrap();
    let before = snapshot(&s);
    let edge = sample().graph.edges[0].clone();
    let result = s
        .dispatch(Command::Batch {
            commands: vec![
                Command::Disconnect { edge: edge.clone() },
                Command::Connect { edge },
            ],
        })
        .unwrap();
    assert!(!result.changed);
    assert_eq!(snapshot(&s), before);
    let mut deep = Command::Rename {
        name: "Too deep".into(),
    };
    for _ in 0..70 {
        deep = Command::Batch {
            commands: vec![deep],
        };
    }
    rejects_without_change(&mut s, deep);
}

#[test]
fn save_as_rebases_existing_undo_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(&new).unwrap();
    std::fs::write(old.join("sales.csv"), "quantity,amount\n1,5\n").unwrap();
    let mut project = sample();
    if let NodeKind::CsvSource { path, .. } = &mut project.graph.nodes[0].config {
        *path = "sales.csv".into();
    }
    let initial = old.join("project.dcraft");
    project.save(&initial).unwrap();
    let mut s = Session::default();
    s.dispatch(Command::Open {
        path: initial,
        discard_changes: false,
    })
    .unwrap();
    s.dispatch(move_node()).unwrap();
    let next = new.join("project.dcraft");
    s.dispatch(Command::SaveAs { path: next.clone() }).unwrap();
    s.dispatch(Command::Undo).unwrap();
    let runtime = s.project().unwrap().resolved(&next).unwrap();
    let NodeKind::CsvSource { path, .. } = &runtime.graph.nodes[0].config else {
        panic!("expected CSV")
    };
    assert_eq!(
        Path::new(path).canonicalize().unwrap(),
        old.join("sales.csv").canonicalize().unwrap()
    );
    assert_eq!(s.project().unwrap().graph.nodes[0].position, [0.0, 0.0]);
    assert!(s.is_dirty());
}
