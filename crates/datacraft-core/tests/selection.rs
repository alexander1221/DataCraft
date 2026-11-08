use datacraft_core::registry::{
    validate_specs, NodeSpec, ParameterRule, ParameterSpec, ParameterType,
};
use datacraft_core::{find_node, Graph, Node, NodeKind};
use serde_json::{json, Value};
use uuid::Uuid;

fn create(kind: &str, params: Value) -> Result<NodeKind, datacraft_core::ParameterError> {
    find_node(kind)
        .unwrap()
        .create_config(params.as_object().unwrap().clone())
}
#[test]
fn statistical_candidate_lists_subset_and_output_contracts_are_enforced() {
    for kind in ["feature_selection", "feature_selection_pipeline"] {
        create(kind, json!({"label_column":"y","columns":["x"]})).unwrap();
        create(
            kind,
            json!({"label_column":"y","columns":["x","region"],"categorical_columns":["region"]}),
        )
        .unwrap();
        for params in [
            json!({"label_column":"x","columns":["x"]}),
            json!({"label_column":"y","columns":[]}),
            json!({"label_column":"y","columns":["x","x"]}),
            json!({"label_column":"y","columns":["x"],"categorical_columns":["outside"]}),
            json!({"label_column":"y","columns":["x"],"categorical_columns":["x","x"]}),
            json!({"label_column":"y","columns":["x"],"relative_error":-0.1}),
            json!({"label_column":"y","columns":["x"],"num_buckets":1}),
        ] {
            assert!(create(kind, params).is_err());
        }
        let columns: Vec<_> = (0..1001).map(|i| format!("x{i}")).collect();
        assert!(create(kind, json!({"label_column":"y","columns":columns})).is_err());
    }
    for output in ["x", "y", ""] {
        assert!(create(
            "feature_selection_pipeline",
            json!({"label_column":"y","columns":["x"],"output_column":output})
        )
        .is_err());
    }
}
#[test]
fn forest_selection_limits_and_nonfinite_drafts_are_rejected() {
    for task in ["classification", "regression"] {
        create(
            "random_forest_pipeline",
            json!({"label_column":"y","task":task}),
        )
        .unwrap();
    }
    for params in [
        json!({"label_column":"y","task":"arbitrary-code"}),
        json!({"label_column":"y","num_top_features":0}),
        json!({"label_column":"y","output_column":"features"}),
        json!({"label_column":"y","num_trees":0}),
    ] {
        assert!(create("random_forest_pipeline", params).is_err());
    }
    for kind in ["feature_selection", "feature_selection_pipeline"] {
        let mut config = create(kind, json!({"label_column":"y","columns":["x"]})).unwrap();
        match &mut config {
            NodeKind::FeatureSelection { relative_error, .. }
            | NodeKind::FeatureSelectionPipeline { relative_error, .. } => {
                *relative_error = f64::NAN
            }
            _ => unreachable!(),
        }
        assert!(Graph {
            nodes: vec![Node {
                id: Uuid::from_u128(1),
                position: [0.0, 0.0],
                config
            }],
            edges: vec![]
        }
        .validate_structure()
        .is_err());
    }
}
#[test]
fn list_metadata_checks_reject_invalid_caps_and_dangling_subset_references() {
    const BAD_MAX: &[ParameterSpec] = &[ParameterSpec {
        name: "columns",
        label: "Columns",
        parameter_type: ParameterType::StringList,
        default: None,
        rules: &[ParameterRule::MaxItems { max: 0 }],
    }];
    const BAD_SUBSET: &[ParameterSpec] = &[ParameterSpec {
        name: "columns",
        label: "Columns",
        parameter_type: ParameterType::StringList,
        default: None,
        rules: &[ParameterRule::ItemsIn {
            parameter: "missing",
        }],
    }];
    for parameters in [BAD_MAX, BAD_SUBSET] {
        let spec = NodeSpec {
            parameters,
            ..*find_node("feature_selection").unwrap()
        };
        assert!(validate_specs(&[&spec]).is_err());
    }
}
