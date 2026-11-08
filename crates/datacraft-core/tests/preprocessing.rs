use datacraft_core::{find_node, Edge, Graph, Node, NodeKind};
use serde_json::{json, Value};
use uuid::Uuid;

fn node(id: u128, kind: &str, parameters: Value) -> Node {
    Node {
        id: Uuid::from_u128(id),
        position: [id as f32 * 100.0, 0.0],
        config: find_node(kind)
            .unwrap()
            .create_config(parameters.as_object().unwrap().clone())
            .unwrap(),
    }
}
fn edge(from: u128, port: &str, to: u128, input: &str) -> Edge {
    Edge {
        from: Uuid::from_u128(from),
        from_port: port.into(),
        to: Uuid::from_u128(to),
        to_port: input.into(),
    }
}
fn graph() -> Graph {
    Graph {
        nodes: vec![
            node(1, "json_source", json!({"path":"data.jsonl"})),
            node(2, "train_test_split", json!({"key_columns":["id"]})),
            node(3, "index_categoricals", json!({"columns":["category"]})),
            node(4, "transform", json!({})),
            node(5, "table_preview", json!({})),
        ],
        edges: vec![
            edge(1, "table", 2, "table"),
            edge(2, "train", 3, "table"),
            edge(2, "test", 4, "table"),
            edge(3, "model", 4, "model"),
            edge(4, "table", 5, "table"),
        ],
    }
}

#[test]
fn train_test_and_model_ports_survive_serialization_and_have_deterministic_dependencies() {
    let graph = graph();
    let order = graph.validate().unwrap();
    assert_eq!(order, (1..=5).map(Uuid::from_u128).collect::<Vec<_>>());
    assert_eq!(graph.execution_order(Uuid::from_u128(5)).unwrap(), order);
    let restored: Graph = serde_json::from_str(&serde_json::to_string(&graph).unwrap()).unwrap();
    assert_eq!(restored, graph);
    assert_eq!(restored.edges[1].from_port, "train");
    assert_eq!(restored.edges[2].from_port, "test");
    let mut invalid = graph.clone();
    invalid.edges[1].from_port = "table".into();
    assert!(invalid.validate_structure().is_err());
    let mut invalid = graph.clone();
    invalid.edges.push(edge(2, "test", 3, "table"));
    assert!(invalid.validate_structure().is_err());
    let mut invalid = graph;
    invalid.edges[1].from_port = "model".into();
    assert!(invalid.validate_structure().is_err());
}

#[test]
fn fraction_bounds_nonfinite_drafts_and_distinct_output_parameters_are_validated() {
    let split = find_node("train_test_split").unwrap();
    for fraction in [0.0, 1.0, -0.1, 1.1] {
        assert!(split
            .create_config(
                json!({"key_columns":["id"],"train_fraction":fraction})
                    .as_object()
                    .unwrap()
                    .clone()
            )
            .is_err());
    }
    assert!(split
        .create_config(
            json!({"key_columns":["id","id"]})
                .as_object()
                .unwrap()
                .clone()
        )
        .is_err());
    let mut graph = graph();
    graph.nodes[1].config = NodeKind::TrainTestSplit {
        key_columns: vec!["id".into()],
        train_fraction: f64::NAN,
        seed: 0,
    };
    assert!(graph.validate_structure().is_err());
    let design = find_node("make_design_matrix").unwrap();
    assert!(design
        .create_config(
            json!({"formula":"y ~ x","features_column":"same","label_column":"same"})
                .as_object()
                .unwrap()
                .clone()
        )
        .is_err());
    assert!(serde_json::from_value::<NodeKind>(json!({"kind":"train_test_split","key_columns":["id"],"train_fraction":0.8,"seed":4294967296_u64})).is_err());
}
