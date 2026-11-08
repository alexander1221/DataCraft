//! Explicit development utility: update only positions in the project files passed as arguments.
use datacraft_engine::{Command, Session};
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let files: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if files.is_empty() {
        return Err("usage: arrange_projects <project.dcraft>...".into());
    }
    for path in files {
        let mut session = Session::default();
        session.dispatch(Command::Open {
            path: path.clone(),
            discard_changes: false,
        })?;
        let project = session.project().ok_or("project did not open")?;
        let commands = datacraft_desktop::arranged_positions(&project.graph)?
            .into_iter()
            .map(|(node_id, position)| Command::MoveNode { node_id, position })
            .collect();
        session.dispatch(Command::Batch { commands })?;
        session.dispatch(Command::Save)?;
        println!("Arranged {}", path.display());
    }
    Ok(())
}
