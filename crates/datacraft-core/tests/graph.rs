use datacraft_core::{Edge, Graph, GraphError, Node, NodeKind};
use uuid::Uuid;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn node(n: u128, config: NodeKind) -> Node {
    Node {
        id: id(n),
        position: [0.0, 0.0],
        config,
    }
}
fn edge(a: u128, b: u128) -> Edge {
    Edge {
        from: id(a),
        from_port: "table".into(),
        to: id(b),
        to_port: "table".into(),
    }
}
fn graph() -> Graph {
    Graph {
        nodes: vec![
            node(
                1,
                NodeKind::CsvSource {
                    path: "data.csv".into(),
                    header: true,
                    infer_schema: true,
                },
            ),
            node(
                2,
                NodeKind::Filter {
                    expression: "amount > 10".into(),
                },
            ),
            node(3, NodeKind::TablePreview { rows: 10 }),
        ],
        edges: vec![edge(1, 2), edge(2, 3)],
    }
}

#[test]
fn valid_and_order_independent() {
    let mut g = graph();
    assert_eq!(g.validate().unwrap(), vec![id(1), id(2), id(3)]);
    g.nodes.reverse();
    g.edges.reverse();
    assert_eq!(g.validate().unwrap(), vec![id(1), id(2), id(3)]);
    assert_eq!(g.output(None).unwrap(), id(3));
}

#[test]
fn cycles() {
    let mut g = graph();
    g.nodes[0].config = NodeKind::Limit { rows: 1 };
    g.edges.push(edge(2, 1));
    assert_eq!(g.validate(), Err(GraphError::Cycle));
}

#[test]
fn missing_references() {
    let mut g = graph();
    g.edges[0].from = id(99);
    assert_eq!(g.validate(), Err(GraphError::MissingNode(id(99))));
}

#[test]
fn invalid_ports() {
    let mut g = graph();
    g.edges[0].from_port = "missing".into();
    assert!(matches!(g.validate(), Err(GraphError::Port { .. })));
}

#[test]
fn incompatible_connections() {
    let mut g = graph();
    g.nodes.push(node(
        4,
        NodeKind::LinearRegression {
            label_column: "amount".into(),
            features_column: "features".into(),
            fit_intercept: true,
            max_iter: 100,
            reg_param: 0.0,
            elastic_net_param: 0.0,
        },
    ));
    g.edges.push(edge(1, 4));
    g.edges[1].from = id(4);
    g.edges[1].from_port = "model".into();
    assert!(matches!(g.validate(), Err(GraphError::Incompatible { .. })));
}

#[test]
fn missing_and_duplicate_inputs() {
    let mut g = graph();
    g.edges.pop();
    assert!(matches!(g.validate(), Err(GraphError::MissingInput { .. })));
    g.edges.push(edge(2, 3));
    g.edges.push(edge(2, 3));
    assert!(matches!(
        g.validate(),
        Err(GraphError::DuplicateInput { .. })
    ));
}

#[test]
fn invalid_parameters_and_duplicate_ids() {
    let mut g = graph();
    g.nodes[2].config = NodeKind::TablePreview { rows: 0 };
    assert!(matches!(g.validate(), Err(GraphError::Parameters { .. })));
    g.nodes[2].config = NodeKind::TablePreview { rows: 1001 };
    assert!(matches!(g.validate(), Err(GraphError::Parameters { .. })));
    let mut g = graph();
    g.nodes.push(g.nodes[0].clone());
    assert_eq!(g.validate(), Err(GraphError::DuplicateNode(id(1))));
}

#[test]
fn selected_dependency_closure_and_ambiguous_outputs() {
    let mut g = graph();
    g.nodes.push(node(4, NodeKind::TablePreview { rows: 5 }));
    g.edges.push(edge(1, 4));
    assert_eq!(g.output(None), Err(GraphError::AmbiguousOutput(2)));
    assert_eq!(g.execution_order(id(4)).unwrap(), vec![id(1), id(4)]);
    assert_eq!(g.output(Some(id(1))), Err(GraphError::InvalidOutput(id(1))));
}

#[test]
fn regression_rejects_non_finite_regularization() {
    let mut g = graph();
    g.nodes[2].config = NodeKind::LinearRegression {
        label_column: "amount".into(),
        features_column: "features".into(),
        fit_intercept: true,
        max_iter: 100,
        reg_param: f64::NAN,
        elastic_net_param: 0.0,
    };
    assert!(matches!(g.validate(), Err(GraphError::Parameters { .. })));
}
