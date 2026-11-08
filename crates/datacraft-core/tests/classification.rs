use datacraft_core::{find_node, Graph, Node, NodeKind};
use serde_json::{json, Value};
use uuid::Uuid;

fn create(kind: &str, parameters: Value) -> Result<NodeKind, datacraft_core::ParameterError> {
    find_node(kind)
        .unwrap()
        .create_config(parameters.as_object().unwrap().clone())
}

#[test]
fn classifier_parameters_and_metrics_contract_are_validated_without_java() {
    for kind in ["logistic_regression", "linear_svc"] {
        create(kind, json!({"label_column":"target"})).unwrap();
        for bad in [
            json!({"label_column":"features"}),
            json!({"label_column":"y","max_iter":0}),
            json!({"label_column":"y","reg_param":-0.1}),
        ] {
            assert!(create(kind, bad).is_err());
        }
    }
    for threshold in [0.0, 1.0] {
        create(
            "logistic_regression",
            json!({"label_column":"y","threshold":threshold}),
        )
        .unwrap();
    }
    for threshold in [-0.1, 1.1] {
        assert!(create(
            "logistic_regression",
            json!({"label_column":"y","threshold":threshold})
        )
        .is_err());
    }
    create("linear_svc", json!({"label_column":"y","threshold":-20.0})).unwrap();
    assert!(create(
        "logistic_regression",
        json!({"label_column":"y","elastic_net_param":1.1})
    )
    .is_err());
    for kind in ["decision_tree_classifier", "random_forest_classifier"] {
        create(
            kind,
            json!({"label_column":"y","impurity":"entropy","max_depth":0}),
        )
        .unwrap();
        assert!(create(kind, json!({"label_column":"y","impurity":"variance"})).is_err());
        assert!(create(kind, json!({"label_column":"y","max_depth":31})).is_err());
    }
    assert!(create(
        "random_forest_classifier",
        json!({"label_column":"y","num_trees":0})
    )
    .is_err());
    assert!(create(
        "classification_metrics",
        json!({"label_column":"prediction"})
    )
    .is_err());
    assert!(
        find_node("classification_metrics")
            .unwrap()
            .selectable_output
    );
    for kind in [
        "logistic_regression",
        "linear_svc",
        "decision_tree_classifier",
        "random_forest_classifier",
    ] {
        assert!(!find_node(kind).unwrap().selectable_output);
    }
}

#[test]
fn nonfinite_classifier_values_are_rejected_even_in_saved_drafts() {
    for kind in ["logistic_regression", "linear_svc"] {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut config = create(kind, json!({"label_column":"y"})).unwrap();
            match &mut config {
                NodeKind::LogisticRegression { threshold, .. }
                | NodeKind::LinearSvc { threshold, .. } => *threshold = invalid,
                _ => unreachable!(),
            }
            let graph = Graph {
                nodes: vec![Node {
                    id: Uuid::from_u128(1),
                    position: [0.0, 0.0],
                    config,
                }],
                edges: vec![],
            };
            assert!(graph.validate_structure().is_err());
        }
    }
}
