//! Persistent graph contracts, independent of Spark and the UI.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use uuid::Uuid;

pub mod hdfs;
pub mod registry;
pub use registry::{builtin_nodes, find_node, NodeSpec, ParameterError};

pub const MAX_PREVIEW_ROWS: u32 = 1000;

// Tagged enums buffer values through serde Content. Decode JSON's lossless Number
// representation explicitly before converting algorithm parameters to f64.
fn deserialize_parameter_number<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<f64, D::Error> {
    let number = serde_json::Number::deserialize(deserializer)?;
    number
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| serde::de::Error::custom("expected a finite floating-point parameter"))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    ParquetExport {
        path: String,
        overwrite: bool,
        create_parents: bool,
        compression: String,
        row_group_mb: u32,
    },
    HdfsImportParquet {
        connection_name: String,
        host: String,
        port: u32,
        protocol: String,
        authentication: String,
        username: String,
        timeout_seconds: u32,
        hdfs_path: String,
    },
    HdfsExportParquet {
        connection_name: String,
        host: String,
        port: u32,
        protocol: String,
        authentication: String,
        username: String,
        timeout_seconds: u32,
        hdfs_path: String,
        overwrite: bool,
        create_parents: bool,
        compression: String,
        row_group_mb: u32,
    },
    HdfsImportText {
        connection_name: String,
        host: String,
        port: u32,
        protocol: String,
        authentication: String,
        username: String,
        timeout_seconds: u32,
        hdfs_path: String,
        delimiter: String,
        delimiter_match_all: bool,
        ignore_consecutive_delimiters: bool,
        record_separator: String,
        record_match_all: bool,
        header: bool,
        encoding: String,
        decimal_separator: String,
        null_value: String,
        quote_start: String,
        quote_end: String,
        record_escape: String,
        delimiter_escape: String,
        quote_start_escape: String,
        quote_end_escape: String,
        max_field_chars: u32,
        skip_rows: u32,
        max_rows: u32,
        skip_blank_lines: bool,
        infer_schema: bool,
        column_options: String,
    },
    HdfsExportText {
        connection_name: String,
        host: String,
        port: u32,
        protocol: String,
        authentication: String,
        username: String,
        timeout_seconds: u32,
        hdfs_path: String,
        delimiter: String,
        delimiter_match_all: bool,
        ignore_consecutive_delimiters: bool,
        record_separator: String,
        record_match_all: bool,
        header: bool,
        encoding: String,
        decimal_separator: String,
        null_value: String,
        quote_start: String,
        quote_end: String,
        record_escape: String,
        delimiter_escape: String,
        quote_start_escape: String,
        quote_end_escape: String,
        max_field_chars: u32,
        overwrite: bool,
        create_parents: bool,
        use_english_numbers: bool,
    },
    CsvSource {
        path: String,
        header: bool,
        infer_schema: bool,
    },
    ParquetSource {
        path: String,
    },
    JsonSource {
        path: String,
    },
    TrainTestSplit {
        key_columns: Vec<String>,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        train_fraction: f64,
        seed: u32,
    },
    MakeDesignMatrix {
        formula: String,
        features_column: String,
        label_column: String,
        keep_unknown: bool,
    },
    IndexCategoricals {
        columns: Vec<String>,
        keep_unknown: bool,
    },
    ReferenceCodeCategoricals {
        columns: Vec<String>,
        keep_unknown: bool,
        drop_last: bool,
    },
    Transform,
    Filter {
        expression: String,
    },
    SelectColumns {
        columns: Vec<String>,
    },
    Limit {
        rows: u32,
    },
    AssembleFeatures {
        columns: Vec<String>,
        output_column: String,
    },
    LinearRegression {
        label_column: String,
        features_column: String,
        fit_intercept: bool,
        max_iter: u32,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        reg_param: f64,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        elastic_net_param: f64,
    },
    GeneralizedLinearRegression {
        label_column: String,
        features_column: String,
        family: String,
        link: String,
        fit_intercept: bool,
        max_iter: u32,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        reg_param: f64,
    },
    DecisionTreeRegressor {
        label_column: String,
        features_column: String,
        max_depth: u32,
        max_bins: u32,
        min_instances_per_node: u32,
        seed: u32,
    },
    RandomForestRegressor {
        label_column: String,
        features_column: String,
        max_depth: u32,
        max_bins: u32,
        min_instances_per_node: u32,
        seed: u32,
        num_trees: u32,
        bootstrap: bool,
        feature_subset_strategy: String,
    },
    RegressionMetrics {
        label_column: String,
        prediction_column: String,
    },
    LogisticRegression {
        label_column: String,
        features_column: String,
        fit_intercept: bool,
        max_iter: u32,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        reg_param: f64,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        elastic_net_param: f64,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        threshold: f64,
    },
    LinearSvc {
        label_column: String,
        features_column: String,
        fit_intercept: bool,
        max_iter: u32,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        reg_param: f64,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        threshold: f64,
    },
    DecisionTreeClassifier {
        label_column: String,
        features_column: String,
        max_depth: u32,
        max_bins: u32,
        min_instances_per_node: u32,
        seed: u32,
        impurity: String,
    },
    RandomForestClassifier {
        label_column: String,
        features_column: String,
        max_depth: u32,
        max_bins: u32,
        min_instances_per_node: u32,
        seed: u32,
        num_trees: u32,
        bootstrap: bool,
        feature_subset_strategy: String,
        impurity: String,
    },
    ClassificationMetrics {
        label_column: String,
        prediction_column: String,
    },
    FeatureSelection {
        label_column: String,
        columns: Vec<String>,
        categorical_columns: Vec<String>,
        num_buckets: u32,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        relative_error: f64,
    },
    FeatureSelectionPipeline {
        label_column: String,
        columns: Vec<String>,
        categorical_columns: Vec<String>,
        num_buckets: u32,
        #[serde(deserialize_with = "deserialize_parameter_number")]
        relative_error: f64,
        num_top_features: u32,
        output_column: String,
        keep_unknown: bool,
    },
    RandomForestPipeline {
        label_column: String,
        features_column: String,
        output_column: String,
        num_top_features: u32,
        task: String,
        max_depth: u32,
        max_bins: u32,
        min_instances_per_node: u32,
        seed: u32,
        num_trees: u32,
        bootstrap: bool,
        feature_subset_strategy: String,
    },
    Predict,
    TablePreview {
        rows: u32,
    },
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    Table,
    Model,
}

impl NodeKind {
    pub fn spec(&self) -> &'static NodeSpec {
        match self {
            Self::ParquetExport { .. } => &registry::PARQUET_EXPORT,
            Self::HdfsImportParquet { .. } => &registry::HDFS_PARQUET_IMPORT,
            Self::HdfsExportParquet { .. } => &registry::HDFS_PARQUET_EXPORT,
            Self::HdfsImportText { .. } => &registry::HDFS_IMPORT,
            Self::HdfsExportText { .. } => &registry::HDFS_EXPORT,
            Self::CsvSource { .. } => &registry::CSV,
            Self::ParquetSource { .. } => &registry::PARQUET,
            Self::JsonSource { .. } => &registry::JSON,
            Self::TrainTestSplit { .. } => &registry::SPLIT,
            Self::MakeDesignMatrix { .. } => &registry::DESIGN,
            Self::IndexCategoricals { .. } => &registry::INDEX,
            Self::ReferenceCodeCategoricals { .. } => &registry::REFERENCE,
            Self::Transform => &registry::TRANSFORM,
            Self::Filter { .. } => &registry::FILTER,
            Self::SelectColumns { .. } => &registry::SELECT,
            Self::Limit { .. } => &registry::LIMIT,
            Self::AssembleFeatures { .. } => &registry::ASSEMBLE,
            Self::LinearRegression { .. } => &registry::REGRESSION,
            Self::GeneralizedLinearRegression { .. } => &registry::GLM,
            Self::DecisionTreeRegressor { .. } => &registry::TREE,
            Self::RandomForestRegressor { .. } => &registry::FOREST,
            Self::RegressionMetrics { .. } => &registry::METRICS,
            Self::LogisticRegression { .. } => &registry::LOGISTIC,
            Self::LinearSvc { .. } => &registry::SVC,
            Self::DecisionTreeClassifier { .. } => &registry::CLASSIFIER_TREE,
            Self::RandomForestClassifier { .. } => &registry::CLASSIFIER_FOREST,
            Self::ClassificationMetrics { .. } => &registry::CLASSIFIER_METRICS,
            Self::FeatureSelection { .. } => &registry::FEATURE_SELECTION,
            Self::FeatureSelectionPipeline { .. } => &registry::FEATURE_PIPELINE,
            Self::RandomForestPipeline { .. } => &registry::FOREST_PIPELINE,
            Self::Predict => &registry::PREDICT,
            Self::TablePreview { .. } => &registry::PREVIEW,
        }
    }

    pub fn inputs(&self) -> Vec<(&'static str, PortType)> {
        self.spec()
            .inputs
            .iter()
            .map(|p| (p.id, p.data_type))
            .collect()
    }

    pub fn outputs(&self) -> Vec<(&'static str, PortType)> {
        self.spec()
            .outputs
            .iter()
            .map(|p| (p.id, p.data_type))
            .collect()
    }

    pub fn is_output(&self) -> bool {
        self.spec().selectable_output
    }

    pub fn validate_parameters(&self) -> Result<(), ParameterError> {
        self.spec().validate_config(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: Uuid,
    pub position: [f32; 2],
    pub config: NodeKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: Uuid,
    pub from_port: String,
    pub to: Uuid,
    pub to_port: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("duplicate node {0}")]
    DuplicateNode(Uuid),
    #[error("missing node {0}")]
    MissingNode(Uuid),
    #[error("node {node}: {message}")]
    Parameters { node: Uuid, message: &'static str },
    #[error("invalid port {port} on node {node}")]
    Port { node: Uuid, port: String },
    #[error("incompatible connection from {from} to {to}")]
    Incompatible { from: Uuid, to: Uuid },
    #[error("duplicate connection to {node}:{port}")]
    DuplicateInput { node: Uuid, port: String },
    #[error("required input missing: {node}:{port}")]
    MissingInput { node: Uuid, port: String },
    #[error("graph contains a cycle")]
    Cycle,
    #[error("choose an output node explicitly (found {0} candidates)")]
    AmbiguousOutput(usize),
    #[error("node {0} is not a preview or analysis output")]
    InvalidOutput(Uuid),
}

/// Recoverable draft issues, distinct from structural corruption.
#[derive(Debug, Error, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphDiagnostic {
    #[error("node {node}: {error}")]
    Parameters { node: Uuid, error: ParameterError },
    #[error("required input missing: {node}:{port}")]
    MissingInput { node: Uuid, port: String },
}

impl From<GraphDiagnostic> for GraphError {
    fn from(diagnostic: GraphDiagnostic) -> Self {
        match diagnostic {
            GraphDiagnostic::Parameters { node, error } => Self::Parameters {
                node,
                message: error.message,
            },
            GraphDiagnostic::MissingInput { node, port } => Self::MissingInput { node, port },
        }
    }
}

impl Graph {
    pub fn diagnostics(&self) -> Result<Vec<GraphDiagnostic>, GraphError> {
        self.validate_structure()?;
        Ok(self.collect_diagnostics())
    }

    fn collect_diagnostics(&self) -> Vec<GraphDiagnostic> {
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by_key(|n| n.id);
        let occupied: BTreeSet<_> = self
            .edges
            .iter()
            .map(|e| (e.to, e.to_port.as_str()))
            .collect();
        let mut diagnostics = Vec::new();
        for node in nodes {
            if let Err(error) = node.config.validate_parameters() {
                diagnostics.push(GraphDiagnostic::Parameters {
                    node: node.id,
                    error,
                });
            }
            for input in node.config.spec().inputs {
                if !occupied.contains(&(node.id, input.id)) {
                    diagnostics.push(GraphDiagnostic::MissingInput {
                        node: node.id,
                        port: input.id.into(),
                    });
                }
            }
        }
        diagnostics
    }
    /// Validate the complete graph and return UUID-ordered deterministic Kahn traversal.
    pub fn validate(&self) -> Result<Vec<Uuid>, GraphError> {
        let order = self.validate_structure()?;
        if let Some(diagnostic) = self.collect_diagnostics().into_iter().next() {
            return Err(diagnostic.into());
        }
        Ok(order)
    }

    /// Integrity only: disconnected inputs and unfinished parameters are allowed.
    pub fn validate_structure(&self) -> Result<Vec<Uuid>, GraphError> {
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if nodes.insert(node.id, node).is_some() {
                return Err(GraphError::DuplicateNode(node.id));
            }
            if !node.position.iter().all(|n| n.is_finite()) {
                return Err(GraphError::Parameters {
                    node: node.id,
                    message: "non-finite position",
                });
            }
            if matches!(&node.config,NodeKind::TrainTestSplit {train_fraction,..} if !train_fraction.is_finite())
                || matches!(&node.config,NodeKind::GeneralizedLinearRegression {reg_param,..} if !reg_param.is_finite())
                || matches!(&node.config,NodeKind::LogisticRegression {reg_param,elastic_net_param,threshold,..}
                    if !reg_param.is_finite() || !elastic_net_param.is_finite() || !threshold.is_finite())
                || matches!(&node.config,NodeKind::LinearSvc {reg_param,threshold,..}
                    if !reg_param.is_finite() || !threshold.is_finite())
                || matches!(&node.config,NodeKind::FeatureSelection {relative_error,..} | NodeKind::FeatureSelectionPipeline {relative_error,..}
                    if !relative_error.is_finite())
            {
                return Err(GraphError::Parameters {
                    node: node.id,
                    message: "non-finite numeric parameter",
                });
            }
            if let NodeKind::LinearRegression {
                reg_param,
                elastic_net_param,
                ..
            } = &node.config
            {
                if !reg_param.is_finite() || !elastic_net_param.is_finite() {
                    return Err(GraphError::Parameters {
                        node: node.id,
                        message: "non-finite numeric parameter",
                    });
                }
            }
        }
        let mut occupied = BTreeSet::new();
        let mut degrees: BTreeMap<Uuid, usize> = nodes.keys().map(|id| (*id, 0)).collect();
        let mut downstream: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
        let mut edges: Vec<_> = self.edges.iter().collect();
        edges.sort();
        for edge in edges {
            let source = nodes
                .get(&edge.from)
                .ok_or(GraphError::MissingNode(edge.from))?;
            let target = nodes
                .get(&edge.to)
                .ok_or(GraphError::MissingNode(edge.to))?;
            let output = source
                .config
                .spec()
                .outputs
                .iter()
                .find(|p| p.id == edge.from_port)
                .ok_or_else(|| GraphError::Port {
                    node: edge.from,
                    port: edge.from_port.clone(),
                })?;
            let input = target
                .config
                .spec()
                .inputs
                .iter()
                .find(|p| p.id == edge.to_port)
                .ok_or_else(|| GraphError::Port {
                    node: edge.to,
                    port: edge.to_port.clone(),
                })?;
            if output.data_type != input.data_type {
                return Err(GraphError::Incompatible {
                    from: edge.from,
                    to: edge.to,
                });
            }
            if !occupied.insert((edge.to, edge.to_port.clone())) {
                return Err(GraphError::DuplicateInput {
                    node: edge.to,
                    port: edge.to_port.clone(),
                });
            }
            *degrees.entry(edge.to).or_default() += 1;
            downstream.entry(edge.from).or_default().push(edge.to);
        }
        let mut ready: BTreeSet<_> = degrees
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut order = Vec::new();
        while let Some(id) = ready.pop_first() {
            order.push(id);
            if let Some(children) = downstream.get(&id) {
                for child in children {
                    if let Some(degree) = degrees.get_mut(child) {
                        *degree -= 1;
                        if *degree == 0 {
                            ready.insert(*child);
                        }
                    }
                }
            }
        }
        if order.len() != nodes.len() {
            return Err(GraphError::Cycle);
        }
        Ok(order)
    }

    pub fn output(&self, requested: Option<Uuid>) -> Result<Uuid, GraphError> {
        if let Some(id) = requested {
            let node = self
                .nodes
                .iter()
                .find(|n| n.id == id)
                .ok_or(GraphError::MissingNode(id))?;
            return if node.config.is_output() {
                Ok(id)
            } else {
                Err(GraphError::InvalidOutput(id))
            };
        }
        let candidates: Vec<_> = self.nodes.iter().filter(|n| n.config.is_output()).collect();
        match candidates.as_slice() {
            [node] => Ok(node.id),
            _ => Err(GraphError::AmbiguousOutput(candidates.len())),
        }
    }

    /// Only the requested output's ancestors, in validated execution order.
    pub fn execution_order(&self, output: Uuid) -> Result<Vec<Uuid>, GraphError> {
        self.output(Some(output))?;
        self.dependency_order(output)
    }

    /// Validated ancestors of any node, including non-report intermediate nodes.
    pub fn dependency_order(&self, output: Uuid) -> Result<Vec<Uuid>, GraphError> {
        let order = self.validate_structure()?;
        if !order.contains(&output) {
            return Err(GraphError::MissingNode(output));
        }
        let mut required = BTreeSet::from([output]);
        let mut pending = vec![output];
        while let Some(id) = pending.pop() {
            for edge in self.edges.iter().filter(|e| e.to == id) {
                if required.insert(edge.from) {
                    pending.push(edge.from);
                }
            }
        }
        let branch = Graph {
            nodes: self
                .nodes
                .iter()
                .filter(|n| required.contains(&n.id))
                .cloned()
                .collect(),
            edges: self
                .edges
                .iter()
                .filter(|e| required.contains(&e.to))
                .cloned()
                .collect(),
        };
        branch.validate()?;
        Ok(order
            .into_iter()
            .filter(|id| required.contains(id))
            .collect())
    }
}
