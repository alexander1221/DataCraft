//! Built-in node contracts. See docs/NODES.md for specification and acceptance.
use crate::{NodeKind, PortType, MAX_PREVIEW_ROWS};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeCategory {
    Source,
    Transform,
    MachineLearning,
    Output,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortCardinality {
    RequiredSingle,
    Many,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct PortSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub data_type: PortType,
    pub cardinality: PortCardinality,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    String,
    StringList,
    Boolean,
    UnsignedInteger,
    Float,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum DefaultValue {
    String(&'static str),
    Boolean(bool),
    UnsignedInteger(u32),
    Float(f64),
}

impl DefaultValue {
    fn value(self) -> Value {
        match self {
            Self::String(v) => v.into(),
            Self::Boolean(v) => v.into(),
            Self::UnsignedInteger(v) => v.into(),
            Self::Float(v) => v.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum ParameterRule {
    NonEmpty,
    LocalPath,
    UniqueItems,
    IntegerRange { min: u32, max: u32 },
    NumberRange { min: f64, max: Option<f64> },
    DifferentFrom { parameter: &'static str },
    NotInList { parameter: &'static str },
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ParameterSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub parameter_type: ParameterType,
    pub default: Option<DefaultValue>,
    pub rules: &'static [ParameterRule],
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct NodeSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub category: NodeCategory,
    pub inputs: &'static [PortSpec],
    pub outputs: &'static [PortSpec],
    pub parameters: &'static [ParameterSpec],
    pub selectable_output: bool,
}

#[derive(Debug, Error, Serialize, PartialEq, Eq)]
#[error("{node}.{parameter}: {message}")]
pub struct ParameterError {
    pub node: &'static str,
    pub parameter: String,
    pub message: &'static str,
}

impl NodeSpec {
    /// Typed placeholders for editing, without claiming execution readiness.
    pub fn create_draft_config(&self) -> Result<NodeKind, ParameterError> {
        let mut values = Map::new();
        for p in self.parameters {
            let value =
                p.default
                    .map(DefaultValue::value)
                    .unwrap_or_else(|| match p.parameter_type {
                        ParameterType::String => Value::String(String::new()),
                        ParameterType::StringList => Value::Array(Vec::new()),
                        ParameterType::Boolean => false.into(),
                        ParameterType::UnsignedInteger => 0.into(),
                        ParameterType::Float => 0.0.into(),
                    });
            values.insert(p.name.into(), value);
        }
        values.insert("kind".into(), self.id.into());
        serde_json::from_value(Value::Object(values))
            .map_err(|_| self.error("config", "configuration does not match typed node contract"))
    }
    fn error(&self, parameter: &str, message: &'static str) -> ParameterError {
        ParameterError {
            node: self.id,
            parameter: parameter.into(),
            message,
        }
    }

    fn validate_parameter(
        &self,
        p: &ParameterSpec,
        value: &Value,
        all: &Map<String, Value>,
    ) -> Result<(), ParameterError> {
        let valid_type = match p.parameter_type {
            ParameterType::String => value.is_string(),
            ParameterType::StringList => value
                .as_array()
                .is_some_and(|xs| xs.iter().all(Value::is_string)),
            ParameterType::Boolean => value.is_boolean(),
            ParameterType::UnsignedInteger => {
                value.as_u64().is_some_and(|n| n <= u64::from(u32::MAX))
            }
            ParameterType::Float => value.as_f64().is_some_and(f64::is_finite),
        };
        if !valid_type {
            return Err(self.error(p.name, "invalid parameter type or non-finite number"));
        }
        for rule in p.rules {
            let valid = match rule {
                ParameterRule::NonEmpty => match value {
                    Value::String(s) => !s.trim().is_empty(),
                    Value::Array(xs) => {
                        !xs.is_empty()
                            && xs
                                .iter()
                                .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
                    }
                    _ => false,
                },
                ParameterRule::LocalPath => value.as_str().is_some_and(|s| !s.contains("://")),
                ParameterRule::UniqueItems => value.as_array().is_some_and(|xs| {
                    xs.iter()
                        .filter_map(Value::as_str)
                        .collect::<BTreeSet<_>>()
                        .len()
                        == xs.len()
                }),
                ParameterRule::IntegerRange { min, max } => value
                    .as_u64()
                    .is_some_and(|v| (u64::from(*min)..=u64::from(*max)).contains(&v)),
                ParameterRule::NumberRange { min, max } => value
                    .as_f64()
                    .is_some_and(|v| v >= *min && max.is_none_or(|m| v <= m)),
                ParameterRule::DifferentFrom { parameter } => all.get(*parameter) != Some(value),
                ParameterRule::NotInList { parameter } => !all
                    .get(*parameter)
                    .and_then(Value::as_array)
                    .is_some_and(|xs| xs.contains(value)),
            };
            if !valid {
                return Err(self.error(p.name, "parameter constraint violated"));
            }
        }
        Ok(())
    }

    fn validate_values(&self, values: &Map<String, Value>) -> Result<(), ParameterError> {
        for name in values.keys() {
            if !self.parameters.iter().any(|p| p.name == name) {
                return Err(self.error(name, "unknown parameter"));
            }
        }
        for p in self.parameters {
            let value = values
                .get(p.name)
                .ok_or_else(|| self.error(p.name, "required parameter missing"))?;
            self.validate_parameter(p, value, values)?;
        }
        Ok(())
    }

    /// Apply defaults only during explicit construction; project loading stays strict.
    pub fn create_config(
        &self,
        mut values: Map<String, Value>,
    ) -> Result<NodeKind, ParameterError> {
        for p in self.parameters {
            if let Some(default) = p.default {
                values.entry(p.name).or_insert_with(|| default.value());
            }
        }
        self.validate_values(&values)?;
        values.insert("kind".into(), self.id.into());
        serde_json::from_value(Value::Object(values))
            .map_err(|_| self.error("config", "configuration does not match typed node contract"))
    }

    pub fn validate_config(&self, config: &NodeKind) -> Result<(), ParameterError> {
        if config.spec().id != self.id {
            return Err(self.error("kind", "node kind mismatch"));
        }
        let value = serde_json::to_value(config)
            .map_err(|_| self.error("config", "configuration serialization failed"))?;
        let mut values = value
            .as_object()
            .cloned()
            .ok_or_else(|| self.error("config", "expected object"))?;
        values.remove("kind");
        self.validate_values(&values)
    }
}

const TABLE: &[PortSpec] = &[PortSpec {
    id: "table",
    label: "Table",
    data_type: PortType::Table,
    cardinality: PortCardinality::RequiredSingle,
}];
const MODEL: &[PortSpec] = &[PortSpec {
    id: "model",
    label: "Model",
    data_type: PortType::Model,
    cardinality: PortCardinality::RequiredSingle,
}];
const TABLE_OUTPUT: &[PortSpec] = &[PortSpec {
    cardinality: PortCardinality::Many,
    ..TABLE[0]
}];
const MODEL_OUTPUT: &[PortSpec] = &[PortSpec {
    cardinality: PortCardinality::Many,
    ..MODEL[0]
}];
const PREDICT_INPUTS: &[PortSpec] = &[TABLE[0], MODEL[0]];
const NONEMPTY: &[ParameterRule] = &[ParameterRule::NonEmpty];
const COLUMNS: &[ParameterRule] = &[ParameterRule::NonEmpty, ParameterRule::UniqueItems];
const PATH: ParameterSpec = ParameterSpec {
    name: "path",
    label: "Local path",
    parameter_type: ParameterType::String,
    default: None,
    rules: &[ParameterRule::NonEmpty, ParameterRule::LocalPath],
};
const COLUMN_SELECTION: ParameterSpec = ParameterSpec {
    name: "columns",
    label: "Columns",
    parameter_type: ParameterType::StringList,
    default: None,
    rules: COLUMNS,
};

pub(crate) const CSV: NodeSpec = NodeSpec {
    id: "csv_source",
    label: "CSV Source",
    category: NodeCategory::Source,
    inputs: &[],
    outputs: TABLE_OUTPUT,
    selectable_output: false,
    parameters: &[
        PATH,
        ParameterSpec {
            name: "header",
            label: "Header",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
        ParameterSpec {
            name: "infer_schema",
            label: "Infer schema",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
    ],
};
pub(crate) const PARQUET: NodeSpec = NodeSpec {
    id: "parquet_source",
    label: "Parquet Source",
    category: NodeCategory::Source,
    inputs: &[],
    outputs: TABLE_OUTPUT,
    parameters: &[PATH],
    selectable_output: false,
};
pub(crate) const FILTER: NodeSpec = NodeSpec {
    id: "filter",
    label: "Filter",
    category: NodeCategory::Transform,
    inputs: TABLE,
    outputs: TABLE_OUTPUT,
    selectable_output: false,
    parameters: &[ParameterSpec {
        name: "expression",
        label: "SQL expression",
        parameter_type: ParameterType::String,
        default: None,
        rules: NONEMPTY,
    }],
};
pub(crate) const SELECT: NodeSpec = NodeSpec {
    id: "select_columns",
    label: "Select Columns",
    category: NodeCategory::Transform,
    inputs: TABLE,
    outputs: TABLE_OUTPUT,
    parameters: &[COLUMN_SELECTION],
    selectable_output: false,
};
pub(crate) const LIMIT: NodeSpec = NodeSpec {
    id: "limit",
    label: "Limit",
    category: NodeCategory::Transform,
    inputs: TABLE,
    outputs: TABLE_OUTPUT,
    selectable_output: false,
    parameters: &[ParameterSpec {
        name: "rows",
        label: "Rows",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(100)),
        rules: &[ParameterRule::IntegerRange {
            min: 1,
            max: i32::MAX as u32,
        }],
    }],
};
pub(crate) const ASSEMBLE: NodeSpec = NodeSpec {
    id: "assemble_features",
    label: "Assemble Features",
    category: NodeCategory::MachineLearning,
    inputs: TABLE,
    outputs: TABLE_OUTPUT,
    selectable_output: false,
    parameters: &[
        COLUMN_SELECTION,
        ParameterSpec {
            name: "output_column",
            label: "Feature vector column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("features")),
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::NotInList {
                    parameter: "columns",
                },
            ],
        },
    ],
};
pub(crate) const REGRESSION: NodeSpec = NodeSpec {
    id: "linear_regression",
    label: "Linear Regression",
    category: NodeCategory::MachineLearning,
    inputs: TABLE,
    outputs: MODEL_OUTPUT,
    selectable_output: true,
    parameters: &[
        ParameterSpec {
            name: "label_column",
            label: "Label column",
            parameter_type: ParameterType::String,
            default: None,
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::DifferentFrom {
                    parameter: "features_column",
                },
            ],
        },
        ParameterSpec {
            name: "features_column",
            label: "Features column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("features")),
            rules: NONEMPTY,
        },
        ParameterSpec {
            name: "fit_intercept",
            label: "Fit intercept",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
        ParameterSpec {
            name: "max_iter",
            label: "Maximum iterations",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(100)),
            rules: &[ParameterRule::IntegerRange {
                min: 1,
                max: i32::MAX as u32,
            }],
        },
        ParameterSpec {
            name: "reg_param",
            label: "Regularization",
            parameter_type: ParameterType::Float,
            default: Some(DefaultValue::Float(0.0)),
            rules: &[ParameterRule::NumberRange {
                min: 0.0,
                max: None,
            }],
        },
        ParameterSpec {
            name: "elastic_net_param",
            label: "Elastic net",
            parameter_type: ParameterType::Float,
            default: Some(DefaultValue::Float(0.0)),
            rules: &[ParameterRule::NumberRange {
                min: 0.0,
                max: Some(1.0),
            }],
        },
    ],
};
pub(crate) const PREDICT: NodeSpec = NodeSpec {
    id: "predict",
    label: "Predict",
    category: NodeCategory::MachineLearning,
    inputs: PREDICT_INPUTS,
    outputs: TABLE_OUTPUT,
    parameters: &[],
    selectable_output: false,
};
pub(crate) const PREVIEW: NodeSpec = NodeSpec {
    id: "table_preview",
    label: "Table Preview",
    category: NodeCategory::Output,
    inputs: TABLE,
    outputs: &[],
    selectable_output: true,
    parameters: &[ParameterSpec {
        name: "rows",
        label: "Preview rows",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(100)),
        rules: &[ParameterRule::IntegerRange {
            min: 1,
            max: MAX_PREVIEW_ROWS,
        }],
    }],
};

pub fn builtin_nodes() -> &'static [&'static NodeSpec] {
    &[
        &CSV,
        &PARQUET,
        &FILTER,
        &SELECT,
        &LIMIT,
        &ASSEMBLE,
        &REGRESSION,
        &PREDICT,
        &PREVIEW,
    ]
}

pub fn find_node(id: &str) -> Option<&'static NodeSpec> {
    builtin_nodes().iter().copied().find(|spec| spec.id == id)
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("node registry {node}.{field}: {message}")]
pub struct RegistryError {
    pub node: &'static str,
    pub field: &'static str,
    pub message: &'static str,
}

/// Validate metadata independently of any graph or backend.
pub fn validate_specs(specs: &[&NodeSpec]) -> Result<(), RegistryError> {
    let mut ids = BTreeSet::new();
    for spec in specs {
        let error = |field, message| RegistryError {
            node: spec.id,
            field,
            message,
        };
        if spec.id.is_empty() || !ids.insert(spec.id) {
            return Err(error("id", "empty or duplicate id"));
        }
        if spec.label.trim().is_empty() {
            return Err(error("label", "empty label"));
        }
        for (ports, cardinality) in [
            (spec.inputs, PortCardinality::RequiredSingle),
            (spec.outputs, PortCardinality::Many),
        ] {
            let mut names = BTreeSet::new();
            for port in ports {
                if port.id.is_empty() || port.label.trim().is_empty() || !names.insert(port.id) {
                    return Err(error("ports", "empty or duplicate port"));
                }
                if port.cardinality != cardinality {
                    return Err(error("ports", "unsupported port cardinality"));
                }
            }
        }
        let mut names = BTreeSet::new();
        let defaults: Map<_, _> = spec
            .parameters
            .iter()
            .filter_map(|p| p.default.map(|d| (p.name.into(), d.value())))
            .collect();
        for p in spec.parameters {
            if p.name.is_empty()
                || p.name == "kind"
                || p.label.trim().is_empty()
                || !names.insert(p.name)
            {
                return Err(error(p.name, "empty, reserved or duplicate parameter"));
            }
            for rule in p.rules {
                let valid = match rule {
                    ParameterRule::NonEmpty => matches!(
                        p.parameter_type,
                        ParameterType::String | ParameterType::StringList
                    ),
                    ParameterRule::LocalPath => p.parameter_type == ParameterType::String,
                    ParameterRule::UniqueItems => p.parameter_type == ParameterType::StringList,
                    ParameterRule::IntegerRange { min, max } => {
                        p.parameter_type == ParameterType::UnsignedInteger && min <= max
                    }
                    ParameterRule::NumberRange { min, max } => {
                        p.parameter_type == ParameterType::Float
                            && min.is_finite()
                            && max.is_none_or(|v| v.is_finite() && v >= *min)
                    }
                    ParameterRule::DifferentFrom { parameter } => {
                        *parameter != p.name
                            && spec.parameters.iter().any(|other| {
                                other.name == *parameter && other.parameter_type == p.parameter_type
                            })
                    }
                    ParameterRule::NotInList { parameter } => {
                        p.parameter_type == ParameterType::String
                            && spec.parameters.iter().any(|other| {
                                other.name == *parameter
                                    && other.parameter_type == ParameterType::StringList
                            })
                    }
                };
                if !valid {
                    return Err(error(p.name, "invalid constraint or parameter reference"));
                }
            }
            if let Some(default) = p.default {
                if spec
                    .validate_parameter(p, &default.value(), &defaults)
                    .is_err()
                {
                    return Err(error(p.name, "invalid default"));
                }
            }
        }
    }
    Ok(())
}
