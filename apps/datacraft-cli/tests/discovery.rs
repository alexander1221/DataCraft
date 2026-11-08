use std::process::Command;

#[test]
fn command_discovery_uses_engine_registry_without_java() {
    let output = Command::new(env!("CARGO_BIN_EXE_datacraft"))
        .args([
            "--java",
            "nonexistent-datacraft-test-java",
            "commands",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let commands: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(commands.as_array().unwrap().len(), 18);
    let add = commands
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "graph.node.add")
        .unwrap();
    assert_eq!(add["undoable"], true);
    assert!(commands
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "execution.cancel"));
}

#[test]
fn node_discovery_is_machine_readable_and_does_not_require_java() {
    let output = Command::new(env!("CARGO_BIN_EXE_datacraft"))
        .args([
            "--java",
            "nonexistent-datacraft-test-java",
            "nodes",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let nodes: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut ids: Vec<_> = nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "assemble_features",
            "csv_source",
            "filter",
            "limit",
            "linear_regression",
            "parquet_source",
            "predict",
            "select_columns",
            "table_preview"
        ]
    );
    let preview = nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "table_preview")
        .unwrap();
    assert_eq!(preview["inputs"][0]["cardinality"], "required_single");
    assert_eq!(preview["parameters"][0]["default"], 100);
}
