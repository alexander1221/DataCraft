use datacraft_core::{find_node, NodeKind, PortType};
use serde_json::{json, Map, Value};

#[test]
fn parquet_contracts_defaults_round_trips_and_invalid_parameters() {
    for kind in [
        "parquet_export",
        "hdfs_import_parquet",
        "hdfs_export_parquet",
    ] {
        let parameters = if kind.starts_with("hdfs_") {
            json!({"hdfs_path":"/data/sales.parquet"})
        } else {
            json!({"path":"data/sales.parquet"})
        };
        let spec = find_node(kind).unwrap();
        let config = spec
            .create_config(serde_json::from_value(parameters).unwrap())
            .unwrap();
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(
            serde_json::from_str::<NodeKind>(&value.to_string()).unwrap(),
            config
        );
        assert_eq!(value.as_object().unwrap().len(), spec.parameters.len() + 1);
        if kind.contains("export") {
            assert_eq!(config.inputs(), vec![("table", PortType::Table)]);
            assert!(config.outputs().is_empty());
            assert!(config.is_output());
            assert_eq!(value["overwrite"], false);
            assert_eq!(value["compression"], "snappy");
            for (field, invalid) in [
                ("compression", json!("unknown")),
                ("row_group_mb", json!(0)),
                ("row_group_mb", json!(1025)),
            ] {
                let mut fields: Map<String, Value> = value.as_object().unwrap().clone();
                fields.remove("kind");
                fields.insert(field.into(), invalid);
                assert!(spec.create_config(fields).is_err(), "{kind}: {field}");
            }
        } else {
            assert!(config.inputs().is_empty());
            assert_eq!(config.outputs(), vec![("table", PortType::Table)]);
        }
        assert!(value.get("delimiter").is_none());
    }
}
