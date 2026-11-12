//! Command contracts; handlers live in Session. Specification: docs/COMMANDS.md.
use datacraft_core::{Edge, Node, NodeKind};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", content = "parameters", deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "graph.node.add")]
    AddNode { node: Node },
    #[serde(rename = "graph.node.remove")]
    RemoveNode { node_id: Uuid },
    #[serde(rename = "graph.node.configure")]
    ConfigureNode { node_id: Uuid, config: NodeKind },
    #[serde(rename = "graph.node.move")]
    MoveNode { node_id: Uuid, position: [f32; 2] },
    #[serde(rename = "graph.edge.connect")]
    Connect { edge: Edge },
    #[serde(rename = "graph.edge.disconnect")]
    Disconnect { edge: Edge },
    #[serde(rename = "project.output.set")]
    SetOutput { output: Option<Uuid> },
    #[serde(rename = "project.rename")]
    Rename { name: String },
    #[serde(rename = "graph.batch")]
    Batch { commands: Vec<Command> },
    #[serde(rename = "history.undo")]
    Undo,
    #[serde(rename = "history.redo")]
    Redo,
    #[serde(rename = "project.open")]
    Open {
        path: PathBuf,
        #[serde(default)]
        discard_changes: bool,
    },
    #[serde(rename = "project.save")]
    Save,
    #[serde(rename = "project.save_as")]
    SaveAs { path: PathBuf },
    #[serde(rename = "execution.run")]
    Run { output: Option<Uuid> },
    #[serde(rename = "execution.node.run")]
    RunNode { node_id: Uuid },
    #[serde(rename = "execution.downstream.run")]
    RunDownstream { node_id: Uuid },
    #[serde(rename = "data.hdfs.list")]
    HdfsList {
        node_id: Uuid,
        config: NodeKind,
        path: String,
    },
    #[serde(rename = "execution.cancel")]
    Cancel { job_id: Uuid },
    #[serde(rename = "execution.status")]
    JobStatus { job_id: Uuid },
    #[serde(rename = "project.new")]
    New {
        name: String,
        #[serde(default)]
        discard_changes: bool,
    },
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandParameterType {
    Node,
    Uuid,
    NodeConfig,
    Position,
    Edge,
    OptionalUuid,
    String,
    Path,
    Boolean,
    CommandList,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CommandParameter {
    pub name: &'static str,
    pub parameter_type: CommandParameterType,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub mutates_project: bool,
    pub undoable: bool,
    pub parameters: &'static [CommandParameter],
}

const NODE_ID: CommandParameter = CommandParameter {
    name: "node_id",
    parameter_type: CommandParameterType::Uuid,
    required: true,
};
const PATH: CommandParameter = CommandParameter {
    name: "path",
    parameter_type: CommandParameterType::Path,
    required: true,
};
const EDGE: CommandParameter = CommandParameter {
    name: "edge",
    parameter_type: CommandParameterType::Edge,
    required: true,
};
const OUTPUT: CommandParameter = CommandParameter {
    name: "output",
    parameter_type: CommandParameterType::OptionalUuid,
    required: false,
};

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "graph.node.add",
        label: "Add node",
        mutates_project: true,
        undoable: true,
        parameters: &[CommandParameter {
            name: "node",
            parameter_type: CommandParameterType::Node,
            required: true,
        }],
    },
    CommandSpec {
        id: "graph.node.remove",
        label: "Remove node",
        mutates_project: true,
        undoable: true,
        parameters: &[NODE_ID],
    },
    CommandSpec {
        id: "graph.node.configure",
        label: "Configure node",
        mutates_project: true,
        undoable: true,
        parameters: &[
            NODE_ID,
            CommandParameter {
                name: "config",
                parameter_type: CommandParameterType::NodeConfig,
                required: true,
            },
        ],
    },
    CommandSpec {
        id: "graph.node.move",
        label: "Move node",
        mutates_project: true,
        undoable: true,
        parameters: &[
            NODE_ID,
            CommandParameter {
                name: "position",
                parameter_type: CommandParameterType::Position,
                required: true,
            },
        ],
    },
    CommandSpec {
        id: "graph.edge.connect",
        label: "Connect ports",
        mutates_project: true,
        undoable: true,
        parameters: &[EDGE],
    },
    CommandSpec {
        id: "graph.edge.disconnect",
        label: "Disconnect ports",
        mutates_project: true,
        undoable: true,
        parameters: &[EDGE],
    },
    CommandSpec {
        id: "project.output.set",
        label: "Set output",
        mutates_project: true,
        undoable: true,
        parameters: &[OUTPUT],
    },
    CommandSpec {
        id: "project.rename",
        label: "Rename project",
        mutates_project: true,
        undoable: true,
        parameters: &[CommandParameter {
            name: "name",
            parameter_type: CommandParameterType::String,
            required: true,
        }],
    },
    CommandSpec {
        id: "graph.batch",
        label: "Edit transaction",
        mutates_project: true,
        undoable: true,
        parameters: &[CommandParameter {
            name: "commands",
            parameter_type: CommandParameterType::CommandList,
            required: true,
        }],
    },
    CommandSpec {
        id: "history.undo",
        label: "Undo",
        mutates_project: true,
        undoable: false,
        parameters: &[],
    },
    CommandSpec {
        id: "history.redo",
        label: "Redo",
        mutates_project: true,
        undoable: false,
        parameters: &[],
    },
    CommandSpec {
        id: "project.open",
        label: "Open project",
        mutates_project: true,
        undoable: false,
        parameters: &[
            PATH,
            CommandParameter {
                name: "discard_changes",
                parameter_type: CommandParameterType::Boolean,
                required: false,
            },
        ],
    },
    CommandSpec {
        id: "project.save",
        label: "Save project",
        mutates_project: false,
        undoable: false,
        parameters: &[],
    },
    CommandSpec {
        id: "project.save_as",
        label: "Save project as",
        mutates_project: true,
        undoable: false,
        parameters: &[PATH],
    },
    CommandSpec {
        id: "execution.run",
        label: "Run output",
        mutates_project: false,
        undoable: false,
        parameters: &[OUTPUT],
    },
    CommandSpec {
        id: "execution.cancel",
        label: "Cancel execution",
        mutates_project: false,
        undoable: false,
        parameters: &[CommandParameter {
            name: "job_id",
            parameter_type: CommandParameterType::Uuid,
            required: true,
        }],
    },
    CommandSpec {
        id: "execution.status",
        label: "Execution status",
        mutates_project: false,
        undoable: false,
        parameters: &[CommandParameter {
            name: "job_id",
            parameter_type: CommandParameterType::Uuid,
            required: true,
        }],
    },
    CommandSpec {
        id: "project.new",
        label: "New project",
        mutates_project: true,
        undoable: false,
        parameters: &[
            CommandParameter {
                name: "name",
                parameter_type: CommandParameterType::String,
                required: true,
            },
            CommandParameter {
                name: "discard_changes",
                parameter_type: CommandParameterType::Boolean,
                required: false,
            },
        ],
    },
    CommandSpec {
        id: "execution.node.run",
        label: "Run node",
        mutates_project: false,
        undoable: false,
        parameters: &[NODE_ID],
    },
    CommandSpec {
        id: "execution.downstream.run",
        label: "Run downstream",
        mutates_project: false,
        undoable: false,
        parameters: &[NODE_ID],
    },
    CommandSpec {
        id: "data.hdfs.list",
        label: "Browse HDFS",
        mutates_project: false,
        undoable: false,
        parameters: &[
            NODE_ID,
            CommandParameter {
                name: "config",
                parameter_type: CommandParameterType::NodeConfig,
                required: true,
            },
            CommandParameter {
                name: "path",
                parameter_type: CommandParameterType::String,
                required: true,
            },
        ],
    },
];

impl Command {
    pub fn spec(&self) -> &'static CommandSpec {
        // Exhaustive match forces new variants to declare a registry entry.
        &COMMANDS[match self {
            Self::AddNode { .. } => 0,
            Self::RemoveNode { .. } => 1,
            Self::ConfigureNode { .. } => 2,
            Self::MoveNode { .. } => 3,
            Self::Connect { .. } => 4,
            Self::Disconnect { .. } => 5,
            Self::SetOutput { .. } => 6,
            Self::Rename { .. } => 7,
            Self::Batch { .. } => 8,
            Self::Undo => 9,
            Self::Redo => 10,
            Self::Open { .. } => 11,
            Self::Save => 12,
            Self::SaveAs { .. } => 13,
            Self::Run { .. } => 14,
            Self::Cancel { .. } => 15,
            Self::JobStatus { .. } => 16,
            Self::New { .. } => 17,
            Self::RunNode { .. } => 18,
            Self::RunDownstream { .. } => 19,
            Self::HdfsList { .. } => 20,
        }]
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("command registry {id}: {message}")]
pub struct CommandRegistryError {
    pub id: &'static str,
    pub message: &'static str,
}

pub fn validate_command_specs(specs: &[CommandSpec]) -> Result<(), CommandRegistryError> {
    let mut ids = BTreeSet::new();
    for spec in specs {
        let error = |message| CommandRegistryError {
            id: spec.id,
            message,
        };
        if spec.id.is_empty() || spec.label.trim().is_empty() || !ids.insert(spec.id) {
            return Err(error("empty label/id or duplicate id"));
        }
        if spec.undoable && !spec.mutates_project {
            return Err(error("undoable command must mutate project"));
        }
        let mut parameters = BTreeSet::new();
        for p in spec.parameters {
            if p.name.is_empty() || !parameters.insert(p.name) {
                return Err(error("empty or duplicate parameter"));
            }
        }
    }
    Ok(())
}
