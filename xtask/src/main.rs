//! Repository checks. Contract: docs/ARCHITECTURE_CHECKS.md.
mod fixtures;
mod layers;

use clap::{Parser, Subcommand};
use datacraft_core::{builtin_nodes, registry::validate_specs};
use datacraft_engine::{commands::validate_command_specs, WorkerConfig, COMMANDS};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    error::Error,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::Duration,
};

#[derive(Parser)]
#[command(about = "DataCraft architecture and acceptance checks")]
struct Options {
    /// Keep Cargo metadata/build/test operations offline (does not configure sbt).
    #[arg(long, global = true)]
    offline: bool,
    #[arg(long, global = true, env = "DATACRAFT_JAVA", default_value = "java")]
    java: PathBuf,
    #[arg(
        long,
        global = true,
        env = "DATACRAFT_WORKER_CLASSPATH",
        default_value = "workers/spark/target/classpath.txt"
    )]
    worker_classpath: PathBuf,
    #[arg(long,global=true,default_value_t=300,value_parser=clap::value_parser!(u64).range(1..))]
    timeout_seconds: u64,
    #[command(subcommand)]
    task: Task,
}
#[derive(Subcommand)]
enum Task {
    Layers,
    Commands,
    Nodes,
    Fixtures,
    Ci {
        #[arg(long)]
        with_spark: bool,
    },
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")))
}
fn cargo_check(root: &Path, args: &[&str], offline: bool) -> Result<(), Box<dyn Error>> {
    println!(
        "[RUN] cargo {}{}",
        args.join(" "),
        if offline { " (offline)" } else { "" }
    );
    let mut command = cargo();
    command.args(args).current_dir(root);
    // Clippy arguments following -- belong to the compiler, not Cargo.
    if offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!("cargo {} failed: {status}", args.join(" ")).into());
    }
    Ok(())
}
fn check_layers(root: &Path, offline: bool) -> Result<(), Box<dyn Error>> {
    let mut command = cargo();
    command
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
        ])
        .current_dir(root);
    if offline {
        command.arg("--offline");
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: layers::Metadata = serde_json::from_slice(&output.stdout)?;
    let errors = layers::check(&metadata)?;
    if !errors.is_empty() {
        return Err(errors.join("\n").into());
    }
    println!("[PASS] layers: resolved normal/dev/build/target dependency graph");
    Ok(())
}
fn check_commands(root: &Path, offline: bool, test_contract: bool) -> Result<(), Box<dyn Error>> {
    validate_command_specs(COMMANDS)?;
    if test_contract {
        cargo_check(
            root,
            &[
                "test",
                "--locked",
                "-p",
                "datacraft-engine",
                "--test",
                "commands",
            ],
            offline,
        )?;
    }
    println!(
        "[PASS] commands: {} IDs and typed contracts",
        COMMANDS.len()
    );
    Ok(())
}
fn check_nodes(root: &Path, offline: bool, test_contract: bool) -> Result<(), Box<dyn Error>> {
    validate_specs(builtin_nodes())?;
    for spec in builtin_nodes() {
        let draft = spec.create_draft_config()?;
        let json = serde_json::to_value(&draft)?;
        if json.get("kind").and_then(Value::as_str) != Some(spec.id) || draft.spec().id != spec.id {
            return Err(format!("{}: typed kind disagrees with registry", spec.id).into());
        }
        let fields: BTreeSet<_> = json
            .as_object()
            .ok_or("typed config is not an object")?
            .keys()
            .filter(|k| *k != "kind")
            .map(String::as_str)
            .collect();
        if fields != spec.parameters.iter().map(|p| p.name).collect() {
            return Err(format!("{}: typed config fields disagree with registry", spec.id).into());
        }
        for p in spec.parameters {
            if let Some(default) = p.default {
                if json.get(p.name) != Some(&serde_json::to_value(default)?) {
                    return Err(
                        format!("{}.{}: default disagrees with factory", spec.id, p.name).into(),
                    );
                }
            }
        }
    }
    if test_contract {
        cargo_check(
            root,
            &[
                "test",
                "--locked",
                "-p",
                "datacraft-core",
                "--test",
                "registry",
            ],
            offline,
        )?;
    }
    println!(
        "[PASS] nodes: {} typed definitions, ports, parameters and defaults",
        builtin_nodes().len()
    );
    Ok(())
}
fn fixture_check(root: &Path, options: &Options) -> Result<(), Box<dyn Error>> {
    let manifest = fixtures::Manifest::parse(&std::fs::read_to_string(
        root.join("examples/expected/pipelines.json"),
    )?)?;
    let classpath = if options.worker_classpath.is_absolute() {
        options.worker_classpath.clone()
    } else {
        root.join(&options.worker_classpath)
    };
    fixtures::run(
        root,
        &manifest,
        WorkerConfig {
            java: options.java.clone(),
            classpath_file: classpath,
        },
        Duration::from_secs(options.timeout_seconds),
    )
}
fn ci(root: &Path, options: &Options, with_spark: bool) -> Result<(), Box<dyn Error>> {
    cargo_check(root, &["fmt", "--all", "--", "--check"], options.offline)?;
    cargo_check(root, &["check", "--workspace", "--locked"], options.offline)?;
    cargo_check(root, &["test", "--workspace", "--locked"], options.offline)?;
    cargo_check(
        root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        options.offline,
    )?;
    check_layers(root, options.offline)?;
    // The workspace suite above already ran the typed contract tests.
    check_commands(root, options.offline, false)?;
    check_nodes(root, options.offline, false)?;
    if with_spark {
        println!("[RUN] sbt test writeClasspath");
        let status = Command::new(if cfg!(windows) { "sbt.bat" } else { "sbt" })
            .args(["test", "writeClasspath"])
            .current_dir(root.join("workers/spark"))
            .status()?;
        if !status.success() {
            return Err(format!("sbt test writeClasspath failed: {status}").into());
        }
        fixture_check(root, options)?;
        let mut command = cargo();
        command
            .args([
                "run",
                "--locked",
                "-p",
                "datacraft-engine",
                "--example",
                "job_lifecycle",
                "--",
                "examples/pipelines/sample.dcraft",
            ])
            .current_dir(root)
            .env("DATACRAFT_JAVA", &options.java);
        let classpath = if options.worker_classpath.is_absolute() {
            options.worker_classpath.clone()
        } else {
            root.join(&options.worker_classpath)
        };
        command.env("DATACRAFT_WORKER_CLASSPATH", classpath);
        if options.offline {
            command.env("CARGO_NET_OFFLINE", "true");
        }
        if !command.status()?.success() {
            return Err("real JVM lifecycle check failed".into());
        }
    } else {
        println!("[NOT_RUN] Spark tests/fixtures/lifecycle (use ci --with-spark)");
    }
    println!("[PASS] requested CI checks completed");
    Ok(())
}
fn execute(options: Options) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask is outside workspace")?
        .canonicalize()?;
    match options.task {
        Task::Layers => check_layers(&root, options.offline),
        Task::Commands => check_commands(&root, options.offline, true),
        Task::Nodes => check_nodes(&root, options.offline, true),
        Task::Fixtures => fixture_check(&root, &options),
        Task::Ci { with_spark } => ci(&root, &options, with_spark),
    }
}
fn main() -> ExitCode {
    match execute(Options::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[FAIL] {error}");
            ExitCode::FAILURE
        }
    }
}
