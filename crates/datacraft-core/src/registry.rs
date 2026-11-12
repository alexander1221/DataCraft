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
    StringList(&'static [&'static str]),
}

impl DefaultValue {
    fn value(self) -> Value {
        match self {
            Self::String(v) => v.into(),
            Self::Boolean(v) => v.into(),
            Self::UnsignedInteger(v) => v.into(),
            Self::Float(v) => v.into(),
            Self::StringList(v) => v
                .iter()
                .map(|s| Value::String((*s).into()))
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum ParameterRule {
    NonEmpty,
    LocalPath,
    UniqueItems,
    MaxItems { max: u32 },
    ItemsIn { parameter: &'static str },
    IntegerRange { min: u32, max: u32 },
    NumberRange { min: f64, max: Option<f64> },
    OpenNumberRange { min: f64, max: f64 },
    OneOf { values: &'static [&'static str] },
    GlmLink { family_parameter: &'static str },
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
                ParameterRule::MaxItems { max } => {
                    value.as_array().is_some_and(|v| v.len() <= *max as usize)
                }
                ParameterRule::ItemsIn { parameter } => value.as_array().is_some_and(|xs| {
                    xs.iter().all(|v| {
                        all.get(*parameter)
                            .and_then(Value::as_array)
                            .is_some_and(|ys| ys.contains(v))
                    })
                }),
                ParameterRule::NumberRange { min, max } => value
                    .as_f64()
                    .is_some_and(|v| v >= *min && max.is_none_or(|m| v <= m)),
                ParameterRule::OpenNumberRange { min, max } => {
                    value.as_f64().is_some_and(|v| v > *min && v < *max)
                }
                ParameterRule::DifferentFrom { parameter } => all.get(*parameter) != Some(value),
                ParameterRule::OneOf { values } => {
                    value.as_str().is_some_and(|s| values.contains(&s))
                }
                ParameterRule::GlmLink { family_parameter } => {
                    let family = all.get(*family_parameter).and_then(Value::as_str);
                    matches!(
                        (family, value.as_str()),
                        (
                            Some("gaussian"),
                            Some("default" | "identity" | "log" | "inverse")
                        ) | (
                            Some("binomial"),
                            Some("default" | "logit" | "probit" | "cloglog")
                        ) | (
                            Some("poisson"),
                            Some("default" | "log" | "identity" | "sqrt")
                        ) | (
                            Some("gamma"),
                            Some("default" | "inverse" | "identity" | "log")
                        )
                    )
                }
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
        if self.id.starts_with("hdfs_") {
            crate::hdfs::validate(self.id, values)
                .map_err(|(field, message)| self.error(&field, message))?;
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

const HDFS_COMMON: &[ParameterSpec] = &[
    ParameterSpec {
        name: "connection_name",
        label: "Connection name",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("Local HDFS")),
        rules: &[],
    },
    ParameterSpec {
        name: "host",
        label: "Host",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("localhost")),
        rules: &[ParameterRule::NonEmpty],
    },
    ParameterSpec {
        name: "port",
        label: "Port",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(9870)),
        rules: &[ParameterRule::IntegerRange { min: 1, max: 65535 }],
    },
    ParameterSpec {
        name: "protocol",
        label: "Protocol",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("webhdfs")),
        rules: &[ParameterRule::OneOf {
            values: &["webhdfs", "swebhdfs"],
        }],
    },
    ParameterSpec {
        name: "authentication",
        label: "Authentication",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("none")),
        rules: &[ParameterRule::OneOf {
            values: &["none", "simple"],
        }],
    },
    ParameterSpec {
        name: "username",
        label: "Hadoop username (Simple)",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("hadoop")),
        rules: &[],
    },
    ParameterSpec {
        name: "timeout_seconds",
        label: "Network timeout (seconds)",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(30)),
        rules: &[ParameterRule::IntegerRange { min: 1, max: 3600 }],
    },
    ParameterSpec {
        name: "hdfs_path",
        label: "HDFS absolute path",
        parameter_type: ParameterType::String,
        default: None,
        rules: &[ParameterRule::NonEmpty],
    },
    ParameterSpec {
        name: "delimiter",
        label: "Field separator (escapes: \\\\t)",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("\\t")),
        rules: &[ParameterRule::NonEmpty],
    },
    ParameterSpec {
        name: "delimiter_match_all",
        label: "Match entire field separator",
        parameter_type: ParameterType::Boolean,
        default: Some(DefaultValue::Boolean(true)),
        rules: &[],
    },
    ParameterSpec {
        name: "ignore_consecutive_delimiters",
        label: "Ignore consecutive field separators",
        parameter_type: ParameterType::Boolean,
        default: Some(DefaultValue::Boolean(false)),
        rules: &[],
    },
    ParameterSpec {
        name: "record_separator",
        label: "Row separator (\\\\n, \\\\r\\\\n, \\\\r, custom)",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("\\n")),
        rules: &[ParameterRule::NonEmpty],
    },
    ParameterSpec {
        name: "record_match_all",
        label: "Match entire row separator",
        parameter_type: ParameterType::Boolean,
        default: Some(DefaultValue::Boolean(true)),
        rules: &[],
    },
    ParameterSpec {
        name: "header",
        label: "Variable names in first row",
        parameter_type: ParameterType::Boolean,
        default: Some(DefaultValue::Boolean(true)),
        rules: &[],
    },
    ParameterSpec {
        name: "encoding",
        label: "Encoding (explicit charset)",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("UTF-8")),
        rules: &[ParameterRule::OneOf {
            values: &[
                "UTF-8",
                "windows-1251",
                "windows-1252",
                "US-ASCII",
                "UTF-16LE",
                "UTF-16BE",
            ],
        }],
    },
    ParameterSpec {
        name: "decimal_separator",
        label: "Decimal separator",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String(".")),
        rules: &[ParameterRule::OneOf {
            values: &[".", ","],
        }],
    },
    ParameterSpec {
        name: "null_value",
        label: "Missing value text",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("")),
        rules: &[],
    },
    ParameterSpec {
        name: "quote_start",
        label: "String start character",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("\"")),
        rules: &[],
    },
    ParameterSpec {
        name: "quote_end",
        label: "String end character",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("\"")),
        rules: &[],
    },
    ParameterSpec {
        name: "record_escape",
        label: "Row separator escape character",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("")),
        rules: &[],
    },
    ParameterSpec {
        name: "delimiter_escape",
        label: "Field separator escape character",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("")),
        rules: &[],
    },
    ParameterSpec {
        name: "quote_start_escape",
        label: "Open quote escape character",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("")),
        rules: &[],
    },
    ParameterSpec {
        name: "quote_end_escape",
        label: "Close quote escape character",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("\"")),
        rules: &[],
    },
    ParameterSpec {
        name: "max_field_chars",
        label: "Maximum characters per field",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(1048576)),
        rules: &[ParameterRule::IntegerRange {
            min: 1,
            max: 16777216,
        }],
    },
];
pub(crate) const HDFS_IMPORT: NodeSpec = NodeSpec {
    id: "hdfs_import_text",
    label: "Import HDFS Text",
    category: NodeCategory::Source,
    inputs: &[],
    outputs: TABLE_OUTPUT,
    selectable_output: false,
    parameters: &[
        HDFS_COMMON[0],
        HDFS_COMMON[1],
        HDFS_COMMON[2],
        HDFS_COMMON[3],
        HDFS_COMMON[4],
        HDFS_COMMON[5],
        HDFS_COMMON[6],
        HDFS_COMMON[7],
        HDFS_COMMON[8],
        HDFS_COMMON[9],
        HDFS_COMMON[10],
        HDFS_COMMON[11],
        HDFS_COMMON[12],
        HDFS_COMMON[13],
        HDFS_COMMON[14],
        HDFS_COMMON[15],
        HDFS_COMMON[16],
        HDFS_COMMON[17],
        HDFS_COMMON[18],
        HDFS_COMMON[19],
        HDFS_COMMON[20],
        HDFS_COMMON[21],
        HDFS_COMMON[22],
        HDFS_COMMON[23],
        ParameterSpec {
            name: "skip_rows",
            label: "Leading records to skip",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(0)),
            rules: &[],
        },
        ParameterSpec {
            name: "max_rows",
            label: "Maximum imported rows (0 = unlimited)",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(0)),
            rules: &[],
        },
        ParameterSpec {
            name: "skip_blank_lines",
            label: "Skip blank records",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
        ParameterSpec {
            name: "infer_schema",
            label: "Infer column types",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
        ParameterSpec {
            name: "column_options",
            label: "Column options (JSON; 1-based column)",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("[]")),
            rules: &[],
        },
    ],
};
pub(crate) const HDFS_EXPORT: NodeSpec = NodeSpec {
    id: "hdfs_export_text",
    label: "HDFS Export Text",
    category: NodeCategory::Output,
    inputs: TABLE,
    outputs: &[],
    selectable_output: true,
    parameters: &[
        HDFS_COMMON[0],
        HDFS_COMMON[1],
        HDFS_COMMON[2],
        HDFS_COMMON[3],
        HDFS_COMMON[4],
        HDFS_COMMON[5],
        HDFS_COMMON[6],
        HDFS_COMMON[7],
        HDFS_COMMON[8],
        HDFS_COMMON[9],
        HDFS_COMMON[10],
        HDFS_COMMON[11],
        HDFS_COMMON[12],
        HDFS_COMMON[13],
        HDFS_COMMON[14],
        HDFS_COMMON[15],
        HDFS_COMMON[16],
        HDFS_COMMON[17],
        HDFS_COMMON[18],
        HDFS_COMMON[19],
        HDFS_COMMON[20],
        HDFS_COMMON[21],
        HDFS_COMMON[22],
        HDFS_COMMON[23],
        ParameterSpec {
            name: "overwrite",
            label: "Replace existing file (atomic rename)",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(false)),
            rules: &[],
        },
        ParameterSpec {
            name: "create_parents",
            label: "Create parent directories",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(false)),
            rules: &[],
        },
        ParameterSpec {
            name: "use_english_numbers",
            label: "English (USA) number formatting",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
    ],
};
const PARQUET_WRITE_PARAMS: &[ParameterSpec] = &[
    ParameterSpec {
        name: "overwrite",
        label: "Replace existing file",
        parameter_type: ParameterType::Boolean,
        default: Some(DefaultValue::Boolean(false)),
        rules: &[],
    },
    ParameterSpec {
        name: "create_parents",
        label: "Create parent directories",
        parameter_type: ParameterType::Boolean,
        default: Some(DefaultValue::Boolean(false)),
        rules: &[],
    },
    ParameterSpec {
        name: "compression",
        label: "Parquet compression",
        parameter_type: ParameterType::String,
        default: Some(DefaultValue::String("snappy")),
        rules: &[ParameterRule::OneOf {
            values: &["uncompressed", "snappy", "gzip", "zstd"],
        }],
    },
    ParameterSpec {
        name: "row_group_mb",
        label: "Row group target (MiB)",
        parameter_type: ParameterType::UnsignedInteger,
        default: Some(DefaultValue::UnsignedInteger(64)),
        rules: &[ParameterRule::IntegerRange { min: 1, max: 1024 }],
    },
];
pub(crate) const PARQUET_EXPORT: NodeSpec = NodeSpec {
    id: "parquet_export",
    label: "Parquet Export",
    category: NodeCategory::Output,
    inputs: TABLE,
    outputs: &[],
    selectable_output: true,
    parameters: &[
        PATH,
        PARQUET_WRITE_PARAMS[0],
        PARQUET_WRITE_PARAMS[1],
        PARQUET_WRITE_PARAMS[2],
        PARQUET_WRITE_PARAMS[3],
    ],
};
pub(crate) const HDFS_PARQUET_IMPORT: NodeSpec = NodeSpec {
    id: "hdfs_import_parquet",
    label: "Import HDFS Parquet",
    category: NodeCategory::Source,
    inputs: &[],
    outputs: TABLE_OUTPUT,
    selectable_output: false,
    parameters: &[
        HDFS_COMMON[0],
        HDFS_COMMON[1],
        HDFS_COMMON[2],
        HDFS_COMMON[3],
        HDFS_COMMON[4],
        HDFS_COMMON[5],
        HDFS_COMMON[6],
        HDFS_COMMON[7],
    ],
};
pub(crate) const HDFS_PARQUET_EXPORT: NodeSpec = NodeSpec {
    id: "hdfs_export_parquet",
    label: "HDFS Export Parquet",
    category: NodeCategory::Output,
    inputs: TABLE,
    outputs: &[],
    selectable_output: true,
    parameters: &[
        HDFS_COMMON[0],
        HDFS_COMMON[1],
        HDFS_COMMON[2],
        HDFS_COMMON[3],
        HDFS_COMMON[4],
        HDFS_COMMON[5],
        HDFS_COMMON[6],
        HDFS_COMMON[7],
        PARQUET_WRITE_PARAMS[0],
        PARQUET_WRITE_PARAMS[1],
        PARQUET_WRITE_PARAMS[2],
        PARQUET_WRITE_PARAMS[3],
    ],
};
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
pub(crate) const JSON: NodeSpec = NodeSpec {
    id: "json_source",
    label: "JSON Source",
    ..PARQUET
};
const FIT_OUTPUTS: &[PortSpec] = &[TABLE_OUTPUT[0], MODEL_OUTPUT[0]];
const KEEP_UNKNOWN: ParameterSpec = ParameterSpec {
    name: "keep_unknown",
    label: "Keep unknown categories",
    parameter_type: ParameterType::Boolean,
    default: Some(DefaultValue::Boolean(false)),
    rules: &[],
};
pub(crate) const SPLIT: NodeSpec = NodeSpec {
    id: "train_test_split",
    label: "Train/Test Split",
    category: NodeCategory::Transform,
    inputs: TABLE,
    outputs: &[
        PortSpec {
            id: "train",
            label: "Train",
            ..TABLE_OUTPUT[0]
        },
        PortSpec {
            id: "test",
            label: "Test",
            ..TABLE_OUTPUT[0]
        },
    ],
    selectable_output: false,
    parameters: &[
        ParameterSpec {
            name: "key_columns",
            label: "Split key columns (stable IDs)",
            ..COLUMN_SELECTION
        },
        ParameterSpec {
            name: "train_fraction",
            label: "Train fraction",
            parameter_type: ParameterType::Float,
            default: Some(DefaultValue::Float(0.8)),
            rules: &[ParameterRule::OpenNumberRange { min: 0.0, max: 1.0 }],
        },
        ParameterSpec {
            name: "seed",
            label: "Seed",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(42)),
            rules: &[],
        },
    ],
};
pub(crate) const INDEX: NodeSpec = NodeSpec {
    id: "index_categoricals",
    label: "Index Categoricals",
    category: NodeCategory::MachineLearning,
    inputs: TABLE,
    outputs: FIT_OUTPUTS,
    selectable_output: false,
    parameters: &[
        COLUMN_SELECTION,
        ParameterSpec {
            default: Some(DefaultValue::Boolean(true)),
            ..KEEP_UNKNOWN
        },
    ],
};
pub(crate) const REFERENCE: NodeSpec = NodeSpec {
    id: "reference_code_categoricals",
    label: "Reference Code Categoricals",
    parameters: &[
        COLUMN_SELECTION,
        KEEP_UNKNOWN,
        ParameterSpec {
            name: "drop_last",
            label: "Drop last category",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
    ],
    ..INDEX
};
pub(crate) const DESIGN: NodeSpec = NodeSpec {
    id: "make_design_matrix",
    label: "Make Design Matrix",
    parameters: &[
        ParameterSpec {
            name: "formula",
            label: "R formula",
            parameter_type: ParameterType::String,
            default: None,
            rules: NONEMPTY,
        },
        ParameterSpec {
            name: "features_column",
            label: "Features column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("features")),
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::DifferentFrom {
                    parameter: "label_column",
                },
            ],
        },
        ParameterSpec {
            name: "label_column",
            label: "Label column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("label")),
            rules: NONEMPTY,
        },
        KEEP_UNKNOWN,
    ],
    ..INDEX
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
pub(crate) const GLM: NodeSpec = NodeSpec {
    id: "generalized_linear_regression",
    label: "Generalized Linear Regression",
    selectable_output: false,
    parameters: &[
        REGRESSION.parameters[0],
        REGRESSION.parameters[1],
        ParameterSpec {
            name: "family",
            label: "Family",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("gaussian")),
            rules: &[ParameterRule::OneOf {
                values: &["gaussian", "binomial", "poisson", "gamma"],
            }],
        },
        ParameterSpec {
            name: "link",
            label: "Link (default uses family default)",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("default")),
            rules: &[ParameterRule::GlmLink {
                family_parameter: "family",
            }],
        },
        REGRESSION.parameters[2],
        ParameterSpec {
            default: Some(DefaultValue::UnsignedInteger(25)),
            ..REGRESSION.parameters[3]
        },
        REGRESSION.parameters[4],
    ],
    ..REGRESSION
};
pub(crate) const TREE: NodeSpec = NodeSpec {
    id: "decision_tree_regressor",
    label: "Decision Tree Regressor",
    selectable_output: false,
    parameters: &[
        REGRESSION.parameters[0],
        REGRESSION.parameters[1],
        ParameterSpec {
            name: "max_depth",
            label: "Maximum depth",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(5)),
            rules: &[ParameterRule::IntegerRange { min: 0, max: 30 }],
        },
        ParameterSpec {
            name: "max_bins",
            label: "Maximum bins",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(32)),
            rules: &[ParameterRule::IntegerRange {
                min: 2,
                max: i32::MAX as u32,
            }],
        },
        ParameterSpec {
            name: "min_instances_per_node",
            label: "Minimum instances per node",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(1)),
            rules: &[ParameterRule::IntegerRange {
                min: 1,
                max: i32::MAX as u32,
            }],
        },
        SPLIT.parameters[2],
    ],
    ..REGRESSION
};
pub(crate) const FOREST: NodeSpec = NodeSpec {
    id: "random_forest_regressor",
    label: "Random Forest Regressor",
    parameters: &[
        TREE.parameters[0],
        TREE.parameters[1],
        TREE.parameters[2],
        TREE.parameters[3],
        TREE.parameters[4],
        TREE.parameters[5],
        ParameterSpec {
            name: "num_trees",
            label: "Number of trees",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(20)),
            rules: &[ParameterRule::IntegerRange {
                min: 1,
                max: i32::MAX as u32,
            }],
        },
        ParameterSpec {
            name: "bootstrap",
            label: "Bootstrap",
            parameter_type: ParameterType::Boolean,
            default: Some(DefaultValue::Boolean(true)),
            rules: &[],
        },
        ParameterSpec {
            name: "feature_subset_strategy",
            label: "Feature subset strategy",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("auto")),
            rules: &[ParameterRule::OneOf {
                values: &["auto", "all", "sqrt", "log2", "onethird"],
            }],
        },
    ],
    ..TREE
};
pub(crate) const METRICS: NodeSpec = NodeSpec {
    id: "regression_metrics",
    label: "Regression Metrics",
    category: NodeCategory::Output,
    inputs: TABLE,
    outputs: &[],
    selectable_output: true,
    parameters: &[
        ParameterSpec {
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::DifferentFrom {
                    parameter: "prediction_column",
                },
            ],
            ..REGRESSION.parameters[0]
        },
        ParameterSpec {
            name: "prediction_column",
            label: "Prediction column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("prediction")),
            rules: NONEMPTY,
        },
    ],
};
pub(crate) const LOGISTIC: NodeSpec = NodeSpec {
    id: "logistic_regression",
    label: "Logistic Regression",
    selectable_output: false,
    parameters: &[
        REGRESSION.parameters[0],
        REGRESSION.parameters[1],
        REGRESSION.parameters[2],
        REGRESSION.parameters[3],
        REGRESSION.parameters[4],
        REGRESSION.parameters[5],
        ParameterSpec {
            name: "threshold",
            label: "Probability threshold (class 1)",
            parameter_type: ParameterType::Float,
            default: Some(DefaultValue::Float(0.5)),
            rules: &[ParameterRule::NumberRange {
                min: 0.0,
                max: Some(1.0),
            }],
        },
    ],
    ..REGRESSION
};
pub(crate) const SVC: NodeSpec = NodeSpec {
    id: "linear_svc",
    label: "Linear SVM",
    parameters: &[
        REGRESSION.parameters[0],
        REGRESSION.parameters[1],
        REGRESSION.parameters[2],
        REGRESSION.parameters[3],
        REGRESSION.parameters[4],
        ParameterSpec {
            name: "threshold",
            label: "Raw margin threshold",
            parameter_type: ParameterType::Float,
            default: Some(DefaultValue::Float(0.0)),
            rules: &[],
        },
    ],
    ..LOGISTIC
};
const IMPURITY: ParameterSpec = ParameterSpec {
    name: "impurity",
    label: "Impurity",
    parameter_type: ParameterType::String,
    default: Some(DefaultValue::String("gini")),
    rules: &[ParameterRule::OneOf {
        values: &["gini", "entropy"],
    }],
};
pub(crate) const CLASSIFIER_TREE: NodeSpec = NodeSpec {
    id: "decision_tree_classifier",
    label: "Decision Tree Classifier",
    parameters: &[
        TREE.parameters[0],
        TREE.parameters[1],
        TREE.parameters[2],
        TREE.parameters[3],
        TREE.parameters[4],
        TREE.parameters[5],
        IMPURITY,
    ],
    ..TREE
};
pub(crate) const CLASSIFIER_FOREST: NodeSpec = NodeSpec {
    id: "random_forest_classifier",
    label: "Random Forest Classifier",
    parameters: &[
        FOREST.parameters[0],
        FOREST.parameters[1],
        FOREST.parameters[2],
        FOREST.parameters[3],
        FOREST.parameters[4],
        FOREST.parameters[5],
        FOREST.parameters[6],
        FOREST.parameters[7],
        FOREST.parameters[8],
        IMPURITY,
    ],
    ..FOREST
};
pub(crate) const CLASSIFIER_METRICS: NodeSpec = NodeSpec {
    id: "classification_metrics",
    label: "Classification Metrics",
    ..METRICS
};
pub(crate) const FEATURE_SELECTION: NodeSpec = NodeSpec {
    id: "feature_selection",
    label: "Feature Selection",
    category: NodeCategory::MachineLearning,
    inputs: TABLE,
    outputs: &[],
    selectable_output: true,
    parameters: &[
        ParameterSpec {
            name: "label_column",
            label: "Categorical target",
            parameter_type: ParameterType::String,
            default: None,
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::NotInList {
                    parameter: "columns",
                },
            ],
        },
        ParameterSpec {
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::UniqueItems,
                ParameterRule::MaxItems { max: 1000 },
            ],
            ..COLUMN_SELECTION
        },
        ParameterSpec {
            name: "categorical_columns",
            label: "Categorical candidates (subset)",
            parameter_type: ParameterType::StringList,
            default: Some(DefaultValue::StringList(&[])),
            rules: &[
                ParameterRule::UniqueItems,
                ParameterRule::ItemsIn {
                    parameter: "columns",
                },
            ],
        },
        ParameterSpec {
            name: "num_buckets",
            label: "Continuous bins",
            parameter_type: ParameterType::UnsignedInteger,
            default: Some(DefaultValue::UnsignedInteger(10)),
            rules: &[ParameterRule::IntegerRange { min: 2, max: 1000 }],
        },
        ParameterSpec {
            name: "relative_error",
            label: "Quantile relative rank error",
            parameter_type: ParameterType::Float,
            default: Some(DefaultValue::Float(0.005)),
            rules: &[ParameterRule::NumberRange {
                min: 0.0,
                max: Some(1.0),
            }],
        },
    ],
};
const TOP_FEATURES: ParameterSpec = ParameterSpec {
    name: "num_top_features",
    label: "Selected feature count",
    parameter_type: ParameterType::UnsignedInteger,
    default: Some(DefaultValue::UnsignedInteger(1)),
    rules: &[ParameterRule::IntegerRange { min: 1, max: 1000 }],
};
pub(crate) const FEATURE_PIPELINE: NodeSpec = NodeSpec {
    id: "feature_selection_pipeline",
    label: "Feature Selection Pipeline",
    outputs: FIT_OUTPUTS,
    parameters: &[
        FEATURE_SELECTION.parameters[0],
        FEATURE_SELECTION.parameters[1],
        FEATURE_SELECTION.parameters[2],
        FEATURE_SELECTION.parameters[3],
        FEATURE_SELECTION.parameters[4],
        TOP_FEATURES,
        ParameterSpec {
            name: "output_column",
            label: "Selected vector column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("selected_features")),
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::NotInList {
                    parameter: "columns",
                },
                ParameterRule::DifferentFrom {
                    parameter: "label_column",
                },
            ],
        },
        ParameterSpec {
            default: Some(DefaultValue::Boolean(true)),
            ..KEEP_UNKNOWN
        },
    ],
    ..FEATURE_SELECTION
};
pub(crate) const FOREST_PIPELINE: NodeSpec = NodeSpec {
    id: "random_forest_pipeline",
    label: "Random Forest Pipeline",
    outputs: FIT_OUTPUTS,
    selectable_output: true,
    parameters: &[
        FOREST.parameters[0],
        FOREST.parameters[1],
        FOREST.parameters[2],
        FOREST.parameters[3],
        FOREST.parameters[4],
        FOREST.parameters[5],
        FOREST.parameters[6],
        FOREST.parameters[7],
        FOREST.parameters[8],
        TOP_FEATURES,
        ParameterSpec {
            name: "task",
            label: "Selection task",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("classification")),
            rules: &[ParameterRule::OneOf {
                values: &["classification", "regression"],
            }],
        },
        ParameterSpec {
            name: "output_column",
            label: "Selected vector column",
            parameter_type: ParameterType::String,
            default: Some(DefaultValue::String("selected_features")),
            rules: &[
                ParameterRule::NonEmpty,
                ParameterRule::DifferentFrom {
                    parameter: "features_column",
                },
                ParameterRule::DifferentFrom {
                    parameter: "label_column",
                },
            ],
        },
    ],
    ..FOREST
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
pub(crate) const TRANSFORM: NodeSpec = NodeSpec {
    id: "transform",
    label: "Transform",
    ..PREDICT
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
        &PARQUET_EXPORT,
        &HDFS_PARQUET_IMPORT,
        &HDFS_PARQUET_EXPORT,
        &HDFS_IMPORT,
        &HDFS_EXPORT,
        &CSV,
        &PARQUET,
        &JSON,
        &SPLIT,
        &DESIGN,
        &INDEX,
        &REFERENCE,
        &TRANSFORM,
        &FILTER,
        &SELECT,
        &LIMIT,
        &ASSEMBLE,
        &REGRESSION,
        &GLM,
        &TREE,
        &FOREST,
        &METRICS,
        &LOGISTIC,
        &SVC,
        &CLASSIFIER_TREE,
        &CLASSIFIER_FOREST,
        &CLASSIFIER_METRICS,
        &FEATURE_SELECTION,
        &FEATURE_PIPELINE,
        &FOREST_PIPELINE,
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
                    ParameterRule::MaxItems { max } => {
                        p.parameter_type == ParameterType::StringList && *max > 0
                    }
                    ParameterRule::ItemsIn { parameter } => {
                        p.parameter_type == ParameterType::StringList
                            && *parameter != p.name
                            && spec.parameters.iter().any(|other| {
                                other.name == *parameter
                                    && other.parameter_type == ParameterType::StringList
                            })
                    }
                    ParameterRule::IntegerRange { min, max } => {
                        p.parameter_type == ParameterType::UnsignedInteger && min <= max
                    }
                    ParameterRule::NumberRange { min, max } => {
                        p.parameter_type == ParameterType::Float
                            && min.is_finite()
                            && max.is_none_or(|v| v.is_finite() && v >= *min)
                    }
                    ParameterRule::OpenNumberRange { min, max } => {
                        p.parameter_type == ParameterType::Float
                            && min.is_finite()
                            && max.is_finite()
                            && min < max
                    }
                    ParameterRule::DifferentFrom { parameter } => {
                        *parameter != p.name
                            && spec.parameters.iter().any(|other| {
                                other.name == *parameter && other.parameter_type == p.parameter_type
                            })
                    }
                    ParameterRule::OneOf { values } => {
                        p.parameter_type == ParameterType::String
                            && !values.is_empty()
                            && values.iter().all(|s| !s.trim().is_empty())
                            && values.iter().collect::<BTreeSet<_>>().len() == values.len()
                    }
                    ParameterRule::GlmLink { family_parameter } => {
                        p.parameter_type == ParameterType::String
                            && *family_parameter != p.name
                            && spec.parameters.iter().any(|other| {
                                other.name == *family_parameter
                                    && other.parameter_type == ParameterType::String
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
