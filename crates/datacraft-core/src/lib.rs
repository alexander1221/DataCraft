//! Persistent graph contracts, independent of Spark and the UI.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_PREVIEW_ROWS: u32 = 1000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    CsvSource {
        path: String,
        header: bool,
        infer_schema: bool,
    },
    ParquetSource {
        path: String,
    },
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
        reg_param: f64,
        elastic_net_param: f64,
    },
    Predict,
    TablePreview {
        rows: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortType {
    Table,
    Model,
}

impl NodeKind {
    pub fn inputs(&self) -> Vec<(&'static str, PortType)> {
        match self {
            Self::CsvSource { .. } | Self::ParquetSource { .. } => vec![],
            Self::Predict => vec![("table", PortType::Table), ("model", PortType::Model)],
            _ => vec![("table", PortType::Table)],
        }
    }

    pub fn outputs(&self) -> Vec<(&'static str, PortType)> {
        match self {
            Self::LinearRegression { .. } => vec![("model", PortType::Model)],
            Self::TablePreview { .. } => vec![],
            _ => vec![("table", PortType::Table)],
        }
    }

    pub fn is_output(&self) -> bool {
        matches!(
            self,
            Self::TablePreview { .. } | Self::LinearRegression { .. }
        )
    }

    fn parameter_error(&self) -> Option<&'static str> {
        let names_ok = |names: &[String]| {
            !names.is_empty()
                && names.iter().all(|s| !s.trim().is_empty())
                && names.iter().collect::<BTreeSet<_>>().len() == names.len()
        };
        match self {
            Self::CsvSource { path, .. } | Self::ParquetSource { path }
                if path.trim().is_empty() || path.contains("://") =>
            {
                Some("expected a local filesystem path")
            }
            Self::Filter { expression } if expression.trim().is_empty() => {
                Some("empty filter expression")
            }
            Self::SelectColumns { columns } if !names_ok(columns) => {
                Some("columns must be nonempty and unique")
            }
            Self::AssembleFeatures {
                columns,
                output_column,
            } if !names_ok(columns)
                || output_column.trim().is_empty()
                || columns.contains(output_column) =>
            {
                Some("invalid feature columns")
            }
            Self::Limit { rows } if *rows == 0 || *rows > i32::MAX as u32 => {
                Some("limit must be between 1 and 2147483647")
            }
            Self::TablePreview { rows } if *rows == 0 || *rows > MAX_PREVIEW_ROWS => {
                Some("preview rows must be between 1 and 1000")
            }
            Self::LinearRegression {
                label_column,
                features_column,
                max_iter,
                reg_param,
                elastic_net_param,
                ..
            } if label_column.trim().is_empty()
                || features_column.trim().is_empty()
                || label_column == features_column
                || *max_iter == 0
                || *max_iter > i32::MAX as u32
                || !reg_param.is_finite()
                || *reg_param < 0.0
                || !elastic_net_param.is_finite()
                || !(0.0..=1.0).contains(elastic_net_param) =>
            {
                Some("invalid regression parameters")
            }
            _ => None,
        }
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

impl Graph {
    /// Validate the complete graph and return UUID-ordered deterministic Kahn traversal.
    pub fn validate(&self) -> Result<Vec<Uuid>, GraphError> {
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
            if let Some(message) = node.config.parameter_error() {
                return Err(GraphError::Parameters {
                    node: node.id,
                    message,
                });
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
                .outputs()
                .into_iter()
                .find(|(p, _)| *p == edge.from_port)
                .ok_or_else(|| GraphError::Port {
                    node: edge.from,
                    port: edge.from_port.clone(),
                })?;
            let input = target
                .config
                .inputs()
                .into_iter()
                .find(|(p, _)| *p == edge.to_port)
                .ok_or_else(|| GraphError::Port {
                    node: edge.to,
                    port: edge.to_port.clone(),
                })?;
            if output.1 != input.1 {
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
        for (id, node) in &nodes {
            for (port, _) in node.config.inputs() {
                if !occupied.contains(&(*id, port.to_owned())) {
                    return Err(GraphError::MissingInput {
                        node: *id,
                        port: port.to_owned(),
                    });
                }
            }
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
        let order = self.validate()?;
        self.output(Some(output))?;
        let mut required = BTreeSet::from([output]);
        let mut pending = vec![output];
        while let Some(id) = pending.pop() {
            for edge in self.edges.iter().filter(|e| e.to == id) {
                if required.insert(edge.from) {
                    pending.push(edge.from);
                }
            }
        }
        Ok(order
            .into_iter()
            .filter(|id| required.contains(id))
            .collect())
    }
}
