use datacraft_core::{find_node, Graph, Node, NodeKind};
use serde_json::{json, Value};
use uuid::Uuid;

fn create(kind: &str, parameters: Value) -> Result<NodeKind, datacraft_core::ParameterError> {
    find_node(kind)
        .unwrap()
        .create_config(parameters.as_object().unwrap().clone())
}

#[test]
fn glm_family_link_pairs_are_validated_before_execution() {
    for (family, links) in [
        ("gaussian", vec!["identity", "log", "inverse"]),
        ("binomial", vec!["logit", "probit", "cloglog"]),
        ("poisson", vec!["log", "identity", "sqrt"]),
        ("gamma", vec!["inverse", "identity", "log"]),
    ] {
        for link in links.into_iter().chain(["default"]) {
            create(
                "generalized_linear_regression",
                json!({"label_column":"y","family":family,"link":link}),
            )
            .unwrap();
        }
    }
    for parameters in [
        json!({"label_column":"y","family":"tweedie"}),
        json!({"label_column":"y","family":"poisson","link":"logit"}),
        json!({"label_column":"y","reg_param":-1.0}),
        json!({"label_column":"features"}),
        json!({"label_column":"y","max_iter":0}),
    ] {
        assert!(create("generalized_linear_regression", parameters).is_err());
    }
}

#[test]
fn tree_forest_and_metrics_reject_invalid_parameters_and_accept_boundaries() {
    for kind in ["decision_tree_regressor", "random_forest_regressor"] {
        for depth in [0, 30] {
            create(
                kind,
                json!({"label_column":"y","max_depth":depth,"seed":4294967295_u64}),
            )
            .unwrap();
        }
        for parameters in [
            json!({"label_column":"y","max_depth":31}),
            json!({"label_column":"y","max_bins":1}),
            json!({"label_column":"y","min_instances_per_node":0}),
            json!({"label_column":"y","seed":4294967296_u64}),
        ] {
            assert!(create(kind, parameters).is_err());
        }
    }
    for parameters in [
        json!({"label_column":"y","num_trees":0}),
        json!({"label_column":"y","feature_subset_strategy":"arbitrary-code"}),
    ] {
        assert!(create("random_forest_regressor", parameters).is_err());
    }
    assert!(create("regression_metrics", json!({"label_column":"prediction"})).is_err());
    assert!(create(
        "regression_metrics",
        json!({"label_column":"y","prediction_column":""})
    )
    .is_err());
}

#[test]
fn nonfinite_glm_configuration_cannot_be_saved_even_as_a_draft() {
    let mut config = create("generalized_linear_regression", json!({"label_column":"y"})).unwrap();
    if let NodeKind::GeneralizedLinearRegression { reg_param, .. } = &mut config {
        *reg_param = f64::INFINITY;
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
