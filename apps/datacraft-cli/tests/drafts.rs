use datacraft_core::NodeKind;
use datacraft_project::Project;
use std::{path::Path, process::Command};

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_datacraft"));
    command.args(["--java", "nonexistent-datacraft-test-java"]);
    command
}

#[test]
fn draft_validation_succeeds_with_diagnostics_but_strict_validation_fails() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pipelines/draft.dcraft");
    let output = cli()
        .arg("validate")
        .arg(&path)
        .arg("--draft")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("2 readiness issues"));
    assert!(text.contains("expression"));
    assert!(text.contains("required input missing"));
    let strict = cli().arg("validate").arg(&path).output().unwrap();
    assert!(!strict.status.success());
}

#[test]
fn invalid_selected_output_fails_before_attempting_java_start() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pipelines/draft.dcraft");
    let mut project = Project::load(&path).unwrap();
    let output = project.output_node.unwrap();
    project
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == output)
        .unwrap()
        .config = NodeKind::TablePreview { rows: 0 };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("draft.dcraft");
    project.save(&path).unwrap();
    let result = cli().arg("run").arg(&path).output().unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8(result.stderr).unwrap();
    assert!(error.contains(&output.to_string()));
    assert!(error.contains("constraint violated"));
    assert!(!error.contains("worker I/O"));
}
