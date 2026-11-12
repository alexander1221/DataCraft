use datacraft_core::{find_node, NodeKind, PortType};
use serde_json::{json, Map, Value};
fn valid(kind: &str) -> NodeKind {
    find_node(kind)
        .unwrap()
        .create_config(serde_json::from_value(json!({"hdfs_path":"/case/данные.tsv"})).unwrap())
        .unwrap()
}
#[test]
fn hdfs_contract_has_exact_ports_explicit_auth_and_no_secrets() {
    let import = valid("hdfs_import_text");
    let export = valid("hdfs_export_text");
    assert!(import.inputs().is_empty());
    assert_eq!(import.outputs(), vec![("table", PortType::Table)]);
    assert_eq!(export.inputs(), vec![("table", PortType::Table)]);
    assert!(export.outputs().is_empty());
    assert!(export.is_output());
    let mut value = serde_json::to_value(import).unwrap();
    assert_eq!(value["protocol"], "webhdfs");
    assert_eq!(value["port"], 9870);
    assert_eq!(value["delimiter"], "\\t");
    assert_eq!(value["record_separator"], "\\n");
    assert!(value.get("password").is_none());
    value["password"] = json!("not-a-real-secret");
    assert!(serde_json::from_value::<NodeKind>(value).is_err());
}
#[test]
fn all_parameters_round_trip_and_schema_matches_the_registry() {
    for kind in ["hdfs_import_text", "hdfs_export_text"] {
        let config = valid(kind);
        let value = serde_json::to_value(&config).unwrap();
        let decoded: NodeKind = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decoded, config);
        for parameter in config.spec().parameters {
            assert!(value.get(parameter.name).is_some(), "{}", parameter.name);
        }
        assert_eq!(
            value.as_object().unwrap().len(),
            config.spec().parameters.len() + 1
        );
    }
}
#[test]
fn connection_paths_delimiters_and_column_overrides_are_validated_before_execution() {
    for (field, value) in [
        ("host", json!("http://host:9870")),
        ("host", json!("user@host")),
        ("port", json!(0)),
        ("hdfs_path", json!("relative.csv")),
        ("hdfs_path", json!("/../file")),
        ("hdfs_path", json!("/file?op=DELETE")),
        ("authentication", json!("kerberos")),
        ("delimiter", json!("\\x")),
        ("delimiter", json!("\\n")),
        ("quote_end", json!("")),
        ("column_options", json!("[{\"column\":0}]")),
        (
            "column_options",
            json!("[{\"column\":1,\"type\":\"unknown\"}]"),
        ),
        (
            "column_options",
            json!("[{\"column\":1,\"omit\":\"true\"}]"),
        ),
    ] {
        // Use an independently specified invalid parameter, not backend output.
        let mut fields: Map<String, Value> =
            serde_json::from_value(serde_json::to_value(valid("hdfs_import_text")).unwrap())
                .unwrap();
        fields.remove("kind");
        fields.insert(field.into(), value);
        assert!(
            find_node("hdfs_import_text")
                .unwrap()
                .create_config(fields)
                .is_err(),
            "{field}"
        );
    }
}
#[test]
fn examples_validate_without_network_and_export_has_no_implicit_overwrite() {
    let export = serde_json::to_value(valid("hdfs_export_text")).unwrap();
    assert_eq!(export["overwrite"], false);
    assert_eq!(export["create_parents"], false);
    for fixture in [
        include_str!("../../../examples/pipelines/hdfs_import_text.dcraft"),
        include_str!("../../../examples/pipelines/hdfs_export_text.dcraft"),
    ] {
        let value: Value = serde_json::from_str(fixture).unwrap();
        for node in value["nodes"].as_array().unwrap() {
            let config: NodeKind = serde_json::from_value(node["config"].clone()).unwrap();
            config.validate_parameters().unwrap();
        }
    }
}
