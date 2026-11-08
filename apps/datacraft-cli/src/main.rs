use clap::{Parser, Subcommand};
use datacraft_core::{Edge, Graph, Node, NodeKind, MAX_PREVIEW_ROWS};
use datacraft_engine::jobs::{JobOptions, JobStatus};
use datacraft_engine::{Command as EngineCommand, CommandPayload, Session, WorkerConfig};
use datacraft_project::{Project, FORMAT_VERSION};
use std::{error::Error, path::PathBuf, process::ExitCode, time::Duration};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    name = "datacraft",
    version,
    about = "Local visual data pipelines: headless runner"
)]
struct Cli {
    /// Maximum job lifetime including JVM startup.
    #[arg(long, global = true, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..))]
    timeout_seconds: u64,
    #[arg(long, global = true, env = "DATACRAFT_JAVA", default_value = "java")]
    java: PathBuf,
    #[arg(
        long,
        global = true,
        env = "DATACRAFT_WORKER_CLASSPATH",
        default_value = "workers/spark/target/classpath.txt"
    )]
    worker_classpath: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover the headless Session command contracts without Java.
    #[command(name = "commands")]
    CommandCatalog {
        #[arg(long)]
        json: bool,
    },
    /// Discover built-in node contracts without starting Java.
    Nodes {
        #[arg(long)]
        json: bool,
    },
    /// Check project format, parameters, ports and DAG (does not read datasets).
    Validate {
        project: PathBuf,
        /// Accept a structurally valid draft and report its readiness issues.
        #[arg(long)]
        draft: bool,
    },
    /// Execute selected preview or model analysis.
    Run {
        project: PathBuf,
        #[arg(long)]
        output: Option<Uuid>,
    },
    /// Read a bounded CSV/Parquet/NDJSON preview and schema.
    Inspect {
        path: PathBuf,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=1000))]
        rows: u32,
    },
}

fn execute(cli: Cli) -> Result<(), Box<dyn Error>> {
    let worker_config = WorkerConfig {
        java: cli.java,
        classpath_file: cli.worker_classpath,
    };
    let mut session = Session::default();
    session.configure_worker(worker_config.clone());
    let job_options = JobOptions {
        timeout: Some(Duration::from_secs(cli.timeout_seconds)),
    };
    session.configure_job_options(job_options);
    let output = match cli.command {
        Commands::CommandCatalog { json } => {
            datacraft_engine::commands::validate_command_specs(datacraft_engine::COMMANDS)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(datacraft_engine::COMMANDS)?
                );
            } else {
                for spec in datacraft_engine::COMMANDS {
                    println!("{} — {} (undoable={})", spec.id, spec.label, spec.undoable);
                }
            }
            return Ok(());
        }
        Commands::Nodes { json } => {
            datacraft_core::registry::validate_specs(datacraft_core::builtin_nodes())?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(datacraft_core::builtin_nodes())?
                );
            } else {
                for spec in datacraft_core::builtin_nodes() {
                    println!("{} — {} ({:?})", spec.id, spec.label, spec.category);
                    println!("  inputs={:?}, outputs={:?}", spec.inputs, spec.outputs);
                    for p in spec.parameters {
                        println!(
                            "  {}: {:?}, default={:?}, rules={:?}",
                            p.name, p.parameter_type, p.default, p.rules
                        );
                    }
                }
            }
            return Ok(());
        }
        Commands::Validate { project, draft } => {
            session.dispatch(EngineCommand::Open {
                path: project,
                discard_changes: false,
            })?;
            let project = session.project().ok_or("project not open")?;
            if draft {
                let diagnostics = project.graph.diagnostics()?;
                println!(
                    "Draft: {} ({} readiness issues)",
                    project.name,
                    diagnostics.len()
                );
                for diagnostic in diagnostics {
                    println!("  {diagnostic}");
                }
                return Ok(());
            }
            project.validate()?;
            println!(
                "Valid: {} ({} nodes, {} edges)",
                project.name,
                project.graph.nodes.len(),
                project.graph.edges.len()
            );
            return Ok(());
        }
        Commands::Run {
            project: path,
            output,
        } => {
            session.dispatch(EngineCommand::Open {
                path,
                discard_changes: false,
            })?;
            output
        }
        Commands::Inspect { path, rows } => {
            let path = path.canonicalize()?;
            let source = Uuid::new_v4();
            let preview = Uuid::new_v4();
            let config = match path.extension().and_then(|s| s.to_str()) {
                Some("json" | "jsonl" | "ndjson") => NodeKind::JsonSource {
                    path: path.to_string_lossy().into_owned(),
                },
                Some("parquet") => NodeKind::ParquetSource {
                    path: path.to_string_lossy().into_owned(),
                },
                Some("csv") => NodeKind::CsvSource {
                    path: path.to_string_lossy().into_owned(),
                    header: true,
                    infer_schema: true,
                },
                _ => return Err("inspect supports .csv and .parquet files".into()),
            };
            let project = Project {
                format_version: FORMAT_VERSION,
                name: "Inspect".into(),
                graph: Graph {
                    nodes: vec![
                        Node {
                            id: source,
                            position: [0.0, 0.0],
                            config,
                        },
                        Node {
                            id: preview,
                            position: [250.0, 0.0],
                            config: NodeKind::TablePreview { rows },
                        },
                    ],
                    edges: vec![Edge {
                        from: source,
                        from_port: "table".into(),
                        to: preview,
                        to_port: "table".into(),
                    }],
                },
                output_node: Some(preview),
            };
            session =
                Session::from_project(project, datacraft_engine::session::DEFAULT_HISTORY_LIMIT)?;
            session.configure_worker(worker_config);
            session.configure_job_options(job_options);
            None
        }
    };
    let result = session.dispatch(EngineCommand::Run { output })?;
    let CommandPayload::JobStarted { job_id, .. } = result.payload else {
        return Err("unexpected execution response".into());
    };
    let info = session.wait_for_execution(
        job_id,
        Duration::from_secs(cli.timeout_seconds.saturating_add(30)),
    )?;
    if info.status != JobStatus::Completed {
        return Err(info
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| format!("execution {:?} (job {job_id})", info.status))
            .into());
    }
    let record = session
        .job_result(job_id)?
        .ok_or("completed job has no result")?;
    for table in &record.result.tables {
        println!("\n{}", table.name);
        println!("Schema: {}", table.schema);
        let columns: Vec<_> = table
            .schema
            .get("fields")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
            .collect();
        println!("{}", columns.join(" | "));
        for row in table.rows.iter().take(MAX_PREVIEW_ROWS as usize) {
            println!(
                "{}",
                columns
                    .iter()
                    .map(|column| {
                        row.get(*column)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "null".into())
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
        println!("{} rows", table.rows.len());
    }
    Ok(())
}

fn main() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
