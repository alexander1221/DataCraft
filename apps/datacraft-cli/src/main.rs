use clap::{Parser, Subcommand};
use datacraft_core::{Edge, Graph, Node, NodeKind, MAX_PREVIEW_ROWS};
use datacraft_engine::Worker;
use datacraft_project::{Project, FORMAT_VERSION};
use std::{error::Error, path::PathBuf, process::ExitCode};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    name = "datacraft",
    version,
    about = "Local visual data pipelines: headless runner"
)]
struct Cli {
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
    /// Check project format, parameters, ports and DAG (does not read datasets).
    Validate { project: PathBuf },
    /// Execute selected preview or model analysis.
    Run {
        project: PathBuf,
        #[arg(long)]
        output: Option<Uuid>,
    },
    /// Read a bounded CSV/Parquet preview and schema.
    Inspect {
        path: PathBuf,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=1000))]
        rows: u32,
    },
}

fn execute(cli: Cli) -> Result<(), Box<dyn Error>> {
    let (project, output) = match cli.command {
        Commands::Validate { project } => {
            let project = Project::load(&project)?;
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
            let project = Project::load(&path)?.resolved(&path)?;
            (project, output)
        }
        Commands::Inspect { path, rows } => {
            let path = path.canonicalize()?;
            let source = Uuid::new_v4();
            let preview = Uuid::new_v4();
            let config = match path.extension().and_then(|s| s.to_str()) {
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
            (
                Project {
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
                },
                None,
            )
        }
    };
    project.validate()?;
    let output = project.graph.output(output.or(project.output_node))?;
    let mut worker = Worker::start(&cli.java, &cli.worker_classpath)?;
    let result = worker.run(&project, Some(output))?;
    for table in result.tables {
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
