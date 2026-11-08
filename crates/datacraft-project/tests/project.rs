use datacraft_project::{Project, ProjectError};
use std::{fs, path::Path};

const SAMPLE: &str = include_str!("../../../examples/pipelines/sample.dcraft");

#[test]
fn preprocessing_examples_round_trip_and_resolve_json_paths() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pipelines");
    for name in [
        "index_categoricals",
        "reference_code",
        "design_matrix",
        "train_test",
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
        let path = root.join(format!("{name}.dcraft"));
        let mut project = Project::load(&path).unwrap();
        project.graph.edges.sort();
        project.validate().unwrap();
        assert_eq!(
            Project::from_json(&project.to_json().unwrap()).unwrap(),
            project
        );
        let resolved = project.resolved(&path).unwrap();
        let datacraft_core::NodeKind::JsonSource { path: source } = &resolved.graph.nodes[0].config
        else {
            panic!("expected JSON source");
        };
        assert!(Path::new(source).is_absolute());
        assert!(Path::new(source).is_file());
        assert!(matches!(&project.graph.nodes[0].config,
            datacraft_core::NodeKind::JsonSource { path } if Path::new(path).is_relative()));
    }
}

#[test]
fn round_trip_and_canonical_order() {
    let project = Project::from_json(SAMPLE).unwrap();
    let canonical = project.to_json().unwrap();
    assert_eq!(Project::from_json(&canonical).unwrap(), project);
    let mut reversed = project.clone();
    reversed.graph.nodes.reverse();
    reversed.graph.edges.reverse();
    assert_eq!(canonical, reversed.to_json().unwrap());
}

#[test]
fn rejects_versions_before_parsing_schema() {
    assert!(matches!(
        Project::from_json(r#"{"format_version":99,"future":true}"#),
        Err(ProjectError::Version(_))
    ));
    assert!(matches!(
        Project::from_json("{}"),
        Err(ProjectError::Version(_))
    ));
    assert!(matches!(
        Project::from_json("{"),
        Err(ProjectError::Json(_))
    ));
}

#[test]
fn save_load_and_protect_unsupported_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.dcraft");
    let project = Project::from_json(SAMPLE).unwrap();
    project.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap(), project);
    project.save(&path).unwrap();
    let future = r#"{"format_version":99,"future":true}"#;
    fs::write(&path, future).unwrap();
    assert!(matches!(project.save(&path), Err(ProjectError::Version(_))));
    assert_eq!(fs::read_to_string(&path).unwrap(), future);
}

#[test]
fn relative_paths_resolve_against_project_not_cwd() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pipelines/sample.dcraft");
    let project = Project::load(&file).unwrap();
    let resolved = project.resolved(&file).unwrap();
    match (
        &project.graph.nodes[0].config,
        &resolved.graph.nodes[0].config,
    ) {
        (
            datacraft_core::NodeKind::CsvSource { path: relative, .. },
            datacraft_core::NodeKind::CsvSource { path: absolute, .. },
        ) => {
            assert!(Path::new(relative).is_relative());
            assert!(Path::new(absolute).is_absolute());
            assert!(Path::new(absolute).exists());
        }
        _ => panic!("sample source must be CSV"),
    }
}

#[test]
fn incomplete_project_round_trips_saves_and_can_be_overwritten() {
    let project =
        Project::from_json(include_str!("../../../examples/pipelines/draft.dcraft")).unwrap();
    assert!(project.validate().is_err());
    project.validate_structure().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.dcraft");
    project.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap(), project);
    project.save(&path).unwrap();
    assert_eq!(
        Project::load(&path).unwrap().to_json().unwrap(),
        project.to_json().unwrap()
    );
}

#[test]
fn draft_placeholders_and_semantically_invalid_values_survive_serialization() {
    let mut project = Project::from_json(SAMPLE).unwrap();
    project.graph.nodes[0].config = datacraft_core::find_node("csv_source")
        .unwrap()
        .create_draft_config()
        .unwrap();
    project.graph.nodes[4].config = datacraft_core::NodeKind::TablePreview { rows: 0 };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.dcraft");
    project.save(&path).unwrap();
    let loaded = Project::load(&path).unwrap();
    assert_eq!(loaded, project);
    assert!(loaded.validate().is_err());
    assert_eq!(
        loaded.resolved(&path).unwrap().graph.nodes[0].config,
        project.graph.nodes[0].config
    );
}

#[test]
fn invalid_save_preserves_existing_file_and_no_nonfinite_value_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("safe.dcraft");
    let mut project = Project::from_json(SAMPLE).unwrap();
    project.save(&path).unwrap();
    let original = fs::read(&path).unwrap();
    project.graph.nodes[0].position[0] = f32::INFINITY;
    assert!(project.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    project.graph.nodes[0].position[0] = 0.0;
    project.graph.nodes[4].config = datacraft_core::NodeKind::LinearRegression {
        label_column: "".into(),
        features_column: "".into(),
        fit_intercept: true,
        max_iter: 0,
        reg_param: f64::NAN,
        elastic_net_param: 0.0,
    };
    assert!(project.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn loading_a_draft_never_accepts_structural_corruption() {
    let original: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
    for change in [
        "missing_node",
        "port",
        "cycle",
        "duplicate",
        "incompatible",
        "output",
    ] {
        let mut value = original.clone();
        match change {
            "missing_node" => {
                value["edges"][0]["from"] = serde_json::json!(uuid::Uuid::from_u128(99))
            }
            "port" => value["edges"][0]["from_port"] = serde_json::json!("invalid"),
            "cycle" => {
                value["nodes"][0]["config"] = serde_json::json!({"kind":"limit","rows":0});
                let edge = serde_json::json!({"from":value["nodes"][1]["id"],"from_port":"table","to":value["nodes"][0]["id"],"to_port":"table"});
                value["edges"].as_array_mut().unwrap().push(edge);
            }
            "duplicate" => {
                let n = value["nodes"][0].clone();
                value["nodes"].as_array_mut().unwrap().push(n);
            }
            "incompatible" => {
                value["nodes"][0]["config"] = serde_json::json!({"kind":"linear_regression","label_column":"","features_column":"","fit_intercept":true,"max_iter":0,"reg_param":0.0,"elastic_net_param":0.0});
                value["edges"][0]["from_port"] = serde_json::json!("model");
            }
            "output" => value["output_node"] = value["nodes"][0]["id"].clone(),
            _ => unreachable!(),
        }
        assert!(Project::from_json(&value.to_string()).is_err(), "{change}");
    }
}
