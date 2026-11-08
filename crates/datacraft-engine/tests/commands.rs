use datacraft_core::{Edge, NodeKind};
use datacraft_engine::commands::validate_command_specs;
use datacraft_engine::{Command, COMMANDS};
use datacraft_project::Project;
use std::collections::BTreeSet;
use uuid::Uuid;

#[test]
fn registry_matches_every_json_command_and_parameter_contract() {
    validate_command_specs(COMMANDS).unwrap();
    let project =
        Project::from_json(include_str!("../../../examples/pipelines/sample.dcraft")).unwrap();
    let id = Uuid::from_u128(1);
    let edge = Edge {
        from: id,
        from_port: "table".into(),
        to: Uuid::from_u128(2),
        to_port: "table".into(),
    };
    let commands = vec![
        Command::AddNode {
            node: project.graph.nodes[0].clone(),
        },
        Command::RemoveNode { node_id: id },
        Command::ConfigureNode {
            node_id: id,
            config: NodeKind::Limit { rows: 10 },
        },
        Command::MoveNode {
            node_id: id,
            position: [0.0, 1.0],
        },
        Command::Connect { edge: edge.clone() },
        Command::Disconnect { edge },
        Command::SetOutput { output: None },
        Command::Rename {
            name: "Test".into(),
        },
        Command::Batch { commands: vec![] },
        Command::Undo,
        Command::Redo,
        Command::Open {
            path: "project.dcraft".into(),
            discard_changes: false,
        },
        Command::Save,
        Command::SaveAs {
            path: "copy.dcraft".into(),
        },
        Command::Run { output: None },
        Command::Cancel { job_id: id },
        Command::JobStatus { job_id: id },
        Command::New {
            name: "New graph".into(),
            discard_changes: false,
        },
    ];
    assert_eq!(commands.len(), COMMANDS.len());
    let mut seen = BTreeSet::new();
    for command in commands {
        let spec = command.spec();
        assert!(seen.insert(spec.id));
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(json["command"], spec.id);
        let actual: BTreeSet<_> = json
            .get("parameters")
            .and_then(|p| p.as_object())
            .into_iter()
            .flatten()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(actual, spec.parameters.iter().map(|p| p.name).collect());
        let decoded: Command = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), json);
    }
    assert!(seen.contains("execution.cancel"));
    assert!(seen.contains("execution.status"));
    let duplicated = [COMMANDS[0], COMMANDS[0]];
    assert!(validate_command_specs(&duplicated).is_err());
    let mut bad = COMMANDS[0];
    bad.mutates_project = false;
    assert!(validate_command_specs(&[bad]).is_err());
    assert!(serde_json::from_str::<Command>(r#"{"command":"unknown"}"#).is_err());
    assert!(serde_json::from_str::<Command>(
        r#"{"command":"graph.node.move","parameters":{"node_id":"bad","position":[0,1]}}"#
    )
    .is_err());
}
