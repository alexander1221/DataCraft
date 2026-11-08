use datacraft_core::{builtin_nodes, Edge, Graph, GraphDiagnostic, GraphError, NodeKind};
use uuid::Uuid;

fn draft() -> Graph {
    serde_json::from_str(include_str!("../../../examples/pipelines/draft.dcraft")).unwrap()
}
fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn diagnostics_are_complete_deterministic_and_distinct_from_integrity_errors() {
    let mut graph = draft();
    let issues = graph.diagnostics().unwrap();
    assert_eq!(issues.len(), 2);
    assert!(
        matches!(&issues[0], GraphDiagnostic::Parameters { node, error } if *node==id(6) && error.parameter=="expression")
    );
    assert_eq!(
        issues[1],
        GraphDiagnostic::MissingInput {
            node: id(6),
            port: "table".into()
        }
    );
    graph.nodes.reverse();
    graph.edges.reverse();
    assert_eq!(graph.diagnostics().unwrap(), issues);
    assert!(graph.validate_structure().is_ok());
    assert!(matches!(graph.validate(),Err(GraphError::Parameters { node,.. }) if node==id(6)));
    assert!(serde_json::to_value(&issues).unwrap().is_array());
}

#[test]
fn ready_branch_runs_but_incomplete_selected_branch_does_not() {
    let mut graph = draft();
    assert_eq!(
        graph.execution_order(id(5)).unwrap(),
        vec![id(1), id(2), id(3), id(4), id(5)]
    );
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id(5))
        .unwrap()
        .config = NodeKind::TablePreview { rows: 0 };
    assert!(
        matches!(graph.execution_order(id(5)),Err(GraphError::Parameters { node,.. }) if node==id(5))
    );
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id(5))
        .unwrap()
        .config = NodeKind::TablePreview { rows: 100 };
    graph.edges.retain(|e| e.to != id(5));
    assert!(
        matches!(graph.execution_order(id(5)),Err(GraphError::MissingInput { node,.. }) if node==id(5))
    );
}

#[test]
fn integrity_errors_in_unselected_branch_are_not_ignored() {
    let mut graph = draft();
    graph.edges.push(Edge {
        from: id(6),
        from_port: "table".into(),
        to: id(6),
        to_port: "table".into(),
    });
    assert_eq!(graph.validate_structure(), Err(GraphError::Cycle));
    assert_eq!(graph.execution_order(id(5)), Err(GraphError::Cycle));
    assert!(graph.diagnostics().is_err());
    graph.edges.last_mut().unwrap().from = id(99);
    assert_eq!(
        graph.validate_structure(),
        Err(GraphError::MissingNode(id(99)))
    );
    graph.edges.last_mut().unwrap().from = id(1);
    graph.edges.last_mut().unwrap().from_port = "missing".into();
    assert!(matches!(
        graph.validate_structure(),
        Err(GraphError::Port { .. })
    ));
}

#[test]
fn draft_factory_preserves_types_without_claiming_readiness() {
    for spec in builtin_nodes() {
        let config = spec.create_draft_config().unwrap();
        assert_eq!(config.spec().id, spec.id);
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<NodeKind>(&json).unwrap(), config);
    }
    let csv = datacraft_core::find_node("csv_source")
        .unwrap()
        .create_draft_config()
        .unwrap();
    assert!(
        matches!(csv,NodeKind::CsvSource { ref path,header:true,infer_schema:true } if path.is_empty())
    );
    assert!(csv.validate_parameters().is_err());
}

#[test]
fn empty_graph_can_be_saved_but_has_no_output() {
    let graph = Graph::default();
    assert!(graph.validate_structure().unwrap().is_empty());
    assert_eq!(graph.output(None), Err(GraphError::AmbiguousOutput(0)));
}

#[test]
fn nonfinite_values_are_not_serializable_draft_state() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut graph = draft();
        graph.nodes[5].config = NodeKind::LinearRegression {
            label_column: "".into(),
            features_column: "".into(),
            fit_intercept: true,
            max_iter: 0,
            reg_param: bad,
            elastic_net_param: 0.0,
        };
        assert!(matches!(
            graph.validate_structure(),
            Err(GraphError::Parameters { .. })
        ));
    }
    let mut graph = draft();
    graph.nodes[0].position[0] = f32::NAN;
    assert!(graph.validate_structure().is_err());
}
