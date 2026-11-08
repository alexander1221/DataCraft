use datacraft_core::registry::{
    validate_specs, DefaultValue, NodeCategory, NodeSpec, ParameterRule, ParameterSpec,
    ParameterType, PortCardinality, PortSpec,
};
use datacraft_core::{builtin_nodes, find_node, NodeKind, PortType};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

fn values(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn public_catalog_matches_persistent_contract_and_ports() {
    // Independent contract table, not a snapshot generated from the registry.
    let expected = [
        (
            "csv_source",
            vec![],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "parquet_source",
            vec![],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "filter",
            vec![("table", PortType::Table)],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "select_columns",
            vec![("table", PortType::Table)],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "limit",
            vec![("table", PortType::Table)],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "assemble_features",
            vec![("table", PortType::Table)],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "linear_regression",
            vec![("table", PortType::Table)],
            vec![("model", PortType::Model)],
            true,
        ),
        (
            "predict",
            vec![("table", PortType::Table), ("model", PortType::Model)],
            vec![("table", PortType::Table)],
            false,
        ),
        (
            "table_preview",
            vec![("table", PortType::Table)],
            vec![],
            true,
        ),
    ];
    validate_specs(builtin_nodes()).unwrap();
    assert_eq!(builtin_nodes().len(), expected.len());
    for (id, inputs, outputs, selected) in expected {
        let spec = find_node(id).unwrap();
        assert_eq!(
            spec.inputs
                .iter()
                .map(|p| (p.id, p.data_type))
                .collect::<Vec<_>>(),
            inputs
        );
        assert_eq!(
            spec.outputs
                .iter()
                .map(|p| (p.id, p.data_type))
                .collect::<Vec<_>>(),
            outputs
        );
        assert_eq!(spec.selectable_output, selected);
    }
    assert!(find_node("unknown").is_none());
}

#[test]
fn creation_defaults_and_typed_round_trip_cover_every_kind() {
    let cases = [
        (
            "csv_source",
            json!({"path":"sales.csv"}),
            json!({"kind":"csv_source","path":"sales.csv","header":true,"infer_schema":true}),
        ),
        (
            "parquet_source",
            json!({"path":"sales.parquet"}),
            json!({"kind":"parquet_source","path":"sales.parquet"}),
        ),
        (
            "filter",
            json!({"expression":"amount > 0"}),
            json!({"kind":"filter","expression":"amount > 0"}),
        ),
        (
            "select_columns",
            json!({"columns":["amount"]}),
            json!({"kind":"select_columns","columns":["amount"]}),
        ),
        ("limit", json!({}), json!({"kind":"limit","rows":100})),
        (
            "assemble_features",
            json!({"columns":["amount"]}),
            json!({"kind":"assemble_features","columns":["amount"],"output_column":"features"}),
        ),
        (
            "linear_regression",
            json!({"label_column":"amount"}),
            json!({"kind":"linear_regression","label_column":"amount","features_column":"features","fit_intercept":true,"max_iter":100,"reg_param":0.0,"elastic_net_param":0.0}),
        ),
        ("predict", json!({}), json!({"kind":"predict"})),
        (
            "table_preview",
            json!({}),
            json!({"kind":"table_preview","rows":100}),
        ),
    ];
    for (id, input, expected) in cases {
        let spec = find_node(id).unwrap();
        let config = spec.create_config(values(input)).unwrap();
        assert_eq!(config.spec().id, id);
        assert_eq!(serde_json::to_value(&config).unwrap(), expected);
        assert_eq!(
            config.inputs(),
            spec.inputs
                .iter()
                .map(|p| (p.id, p.data_type))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            config.outputs(),
            spec.outputs
                .iter()
                .map(|p| (p.id, p.data_type))
                .collect::<Vec<_>>()
        );
        config.validate_parameters().unwrap();
        let expected: NodeKind = serde_json::from_value(expected).unwrap();
        assert_eq!(config, expected);
    }
}

#[test]
fn metadata_parameters_match_every_serialized_field_in_existing_fixtures() {
    for fixture in [
        include_str!("../../../examples/pipelines/sample.dcraft"),
        include_str!("../../../examples/pipelines/regression.dcraft"),
        include_str!("../../../examples/pipelines/prediction.dcraft"),
    ] {
        let fixture: Value = serde_json::from_str(fixture).unwrap();
        for node in fixture["nodes"].as_array().unwrap() {
            let config = &node["config"];
            let typed: NodeKind = serde_json::from_value(config.clone()).unwrap();
            typed.validate_parameters().unwrap();
            let actual: BTreeSet<_> = config
                .as_object()
                .unwrap()
                .keys()
                .filter(|k| *k != "kind")
                .map(String::as_str)
                .collect();
            let metadata: BTreeSet<_> = typed.spec().parameters.iter().map(|p| p.name).collect();
            assert_eq!(actual, metadata);
            assert_eq!(serde_json::to_value(&typed).unwrap(), *config);
        }
    }
}

#[test]
fn validation_has_structured_parameter_errors_and_no_implicit_load_defaults() {
    let csv = find_node("csv_source").unwrap();
    assert_eq!(csv.create_config(Map::new()).unwrap_err().parameter, "path");
    for input in [
        json!({"path":" "}),
        json!({"path":"https://example.invalid/data.csv"}),
        json!({"path":"data.csv","header":"true"}),
        json!({"path":"data.csv","unknown":1}),
        json!({"path":"data.csv","kind":"predict"}),
    ] {
        assert!(csv.create_config(values(input)).is_err());
    }
    assert!(
        serde_json::from_value::<NodeKind>(json!({"kind":"csv_source","path":"data.csv"})).is_err()
    );
    let config = csv
        .create_config(values(json!({"path":"данные продаж.csv","header":false})))
        .unwrap();
    assert!(matches!(config, NodeKind::CsvSource { header: false, .. }));
    assert!(find_node("limit")
        .unwrap()
        .validate_config(&config)
        .is_err());
}

#[test]
fn numeric_boundaries_and_types() {
    for (id, maximum) in [("limit", i32::MAX as u64), ("table_preview", 1000)] {
        let spec = find_node(id).unwrap();
        for n in [1, maximum] {
            assert!(spec.create_config(values(json!({"rows":n}))).is_ok());
        }
        for v in [
            json!(0),
            json!(maximum + 1),
            json!(-1),
            json!(1.5),
            json!(u64::MAX),
            Value::Null,
        ] {
            let error = spec.create_config(values(json!({"rows":v}))).unwrap_err();
            assert_eq!(error.parameter, "rows");
        }
    }
    let regression = find_node("linear_regression").unwrap();
    for (parameter, bad) in [
        ("max_iter", json!(0)),
        ("max_iter", json!(2147483648u64)),
        ("reg_param", json!(-0.1)),
        ("elastic_net_param", json!(1.01)),
        ("elastic_net_param", json!(-0.01)),
    ] {
        let mut input = values(json!({"label_column":"amount"}));
        input.insert(parameter.into(), bad);
        assert_eq!(
            regression.create_config(input).unwrap_err().parameter,
            parameter
        );
    }
    for value in [0.0, 1.0] {
        regression
            .create_config(values(
                json!({"label_column":"amount","elastic_net_param":value}),
            ))
            .unwrap();
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let config = NodeKind::LinearRegression {
            label_column: "amount".into(),
            features_column: "features".into(),
            fit_intercept: true,
            max_iter: 100,
            reg_param: bad,
            elastic_net_param: 0.0,
        };
        assert_eq!(
            config.validate_parameters().unwrap_err().parameter,
            "reg_param"
        );
    }
}

#[test]
fn list_and_cross_parameter_constraints() {
    for id in ["select_columns", "assemble_features"] {
        for columns in [
            json!([]),
            json!([""]),
            json!(["amount", "amount"]),
            json!([1]),
        ] {
            assert!(find_node(id)
                .unwrap()
                .create_config(values(json!({"columns":columns})))
                .is_err());
        }
    }
    assert_eq!(
        find_node("assemble_features")
            .unwrap()
            .create_config(values(json!({"columns":["features"]})))
            .unwrap_err()
            .parameter,
        "output_column"
    );
    assert_eq!(
        find_node("linear_regression")
            .unwrap()
            .create_config(values(json!({"label_column":"features"})))
            .unwrap_err()
            .parameter,
        "label_column"
    );
}

#[test]
fn registry_checker_rejects_invalid_metadata() {
    let base = *find_node("limit").unwrap();
    assert!(validate_specs(&[&base, &base]).is_err());
    const DUPLICATE_PORTS: &[PortSpec] = &[
        PortSpec {
            id: "table",
            label: "First",
            data_type: PortType::Table,
            cardinality: PortCardinality::RequiredSingle,
        },
        PortSpec {
            id: "table",
            label: "Second",
            data_type: PortType::Table,
            cardinality: PortCardinality::RequiredSingle,
        },
    ];
    let invalid = NodeSpec {
        inputs: DUPLICATE_PORTS,
        ..base
    };
    assert!(validate_specs(&[&invalid]).is_err());
    const BAD_DEFAULT: &[ParameterSpec] = &[ParameterSpec {
        name: "rows",
        label: "Rows",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(0)),
        rules: &[ParameterRule::IntegerRange { min: 1, max: 100 }],
    }];
    assert!(validate_specs(&[&NodeSpec {
        parameters: BAD_DEFAULT,
        ..base
    }])
    .is_err());
    const BAD_REFERENCE: &[ParameterSpec] = &[ParameterSpec {
        name: "x",
        label: "X",
        parameter_type: ParameterType::String,
        default: None,
        rules: &[ParameterRule::DifferentFrom {
            parameter: "missing",
        }],
    }];
    assert!(validate_specs(&[&NodeSpec {
        parameters: BAD_REFERENCE,
        ..base
    }])
    .is_err());
    const GOOD_PARAMETER: ParameterSpec = ParameterSpec {
        default: Some(DefaultValue::UnsignedInteger(1)),
        ..BAD_DEFAULT[0]
    };
    const DUPLICATE_PARAMETERS: &[ParameterSpec] = &[GOOD_PARAMETER, GOOD_PARAMETER];
    assert_eq!(
        validate_specs(&[&NodeSpec {
            id: "bad",
            label: "Bad",
            category: NodeCategory::Transform,
            inputs: &[],
            outputs: &[],
            parameters: DUPLICATE_PARAMETERS,
            selectable_output: false
        }])
        .unwrap_err()
        .message,
        "empty, reserved or duplicate parameter"
    );
}
