use datacraft_project::{Project, ProjectError};
use std::{fs, path::Path};

const SAMPLE: &str = include_str!("../../../examples/pipelines/sample.dcraft");

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
