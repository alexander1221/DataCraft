use super::*;
fn with_result() -> Session {
    let project =
        Project::from_json(include_str!("../../../examples/pipelines/sample.dcraft")).unwrap();
    let mut session = Session::from_project(project, 100).unwrap();
    // State-machine fixture only; Spark output is verified by the real GUI acceptance.
    session.last_execution = Some(Arc::new(ExecutionRecord {
        project_revision: 1,
        result: ExecutionResult { tables: vec![] },
    }));
    session.result_computation_revision = Some(1);
    session
}
#[test]
fn layout_rename_batches_and_their_undo_preserve_results_but_configuration_does_not() {
    let mut s = with_result();
    assert!(s.result_is_current());
    s.dispatch(Command::Batch {
        commands: vec![
            Command::MoveNode {
                node_id: Uuid::from_u128(1),
                position: [17.0, 31.0],
            },
            Command::Rename {
                name: "Renamed".into(),
            },
        ],
    })
    .unwrap();
    assert!(s.result_is_current());
    assert!(s.is_dirty());
    assert_eq!(s.revision(), 2);
    s.dispatch(Command::Undo).unwrap();
    assert!(s.result_is_current());
    s.dispatch(Command::Redo).unwrap();
    assert!(s.result_is_current());
    s.dispatch(Command::ConfigureNode {
        node_id: Uuid::from_u128(2),
        config: NodeKind::Filter {
            expression: "quantity >= 5".into(),
        },
    })
    .unwrap();
    assert!(!s.result_is_current());
    s.dispatch(Command::Undo).unwrap();
    assert!(
        !s.result_is_current(),
        "computational undo is conservatively invalidating"
    );
}
#[test]
fn saving_and_rebasing_paths_keep_results_and_failed_mutations_are_atomic() {
    let mut s = with_result();
    let dir = tempfile::tempdir().unwrap();
    s.dispatch(Command::SaveAs {
        path: dir.path().join("first.dcraft"),
    })
    .unwrap();
    assert!(s.result_is_current());
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    s.dispatch(Command::SaveAs {
        path: sub.join("second.dcraft"),
    })
    .unwrap();
    assert!(s.result_is_current());
    let before = s.computation_revision;
    assert!(s
        .dispatch(Command::RemoveNode {
            node_id: Uuid::from_u128(999)
        })
        .is_err());
    assert_eq!(s.computation_revision, before);
    assert!(s.result_is_current());
    s.dispatch(Command::SetOutput { output: None }).unwrap();
    assert!(!s.result_is_current());
}
