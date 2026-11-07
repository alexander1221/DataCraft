//! Single-job, blocking worker client. Use from a background thread in a future UI.
use datacraft_project::Project;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Project(#[from] datacraft_project::ProjectError),
    #[error(transparent)]
    Graph(#[from] datacraft_core::GraphError),
    #[error("worker I/O (install Java 17/21 and build the Scala worker): {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid worker response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("worker protocol error: {0}")]
    Protocol(String),
    #[error("Spark node {node:?}: {message}")]
    Execution { node: Option<Uuid>, message: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResultTable {
    pub name: String,
    pub schema: serde_json::Value,
    pub rows: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ExecutionResult {
    pub tables: Vec<ResultTable>,
}

#[derive(Deserialize)]
struct Response {
    protocol_version: u32,
    request_id: Uuid,
    ok: bool,
    #[serde(default)]
    tables: Vec<ResultTable>,
    node: Option<Uuid>,
    message: Option<String>,
}

pub struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Worker {
    /// The classpath is produced by `sbt writeClasspath`; no shell is involved.
    pub fn start(java: &Path, classpath_file: &Path) -> Result<Self, EngineError> {
        let classpath = fs::read_to_string(classpath_file)?;
        let mut command = Command::new(java);
        for module in [
            "java.lang",
            "java.lang.invoke",
            "java.lang.reflect",
            "java.io",
            "java.net",
            "java.nio",
            "java.util",
            "java.util.concurrent",
            "java.util.concurrent.atomic",
            "sun.nio.ch",
            "sun.nio.cs",
            "sun.security.action",
            "sun.util.calendar",
        ] {
            command.arg(format!("--add-opens=java.base/{module}=ALL-UNNAMED"));
        }
        let mut child = command
            .args(["-Xmx2g", "-cp", classpath.trim(), "datacraft.Worker"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::Protocol("missing stdin".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| EngineError::Protocol("missing stdout".into()))?;
        Ok(Self {
            child,
            input: Some(input),
            output: BufReader::new(output),
        })
    }

    pub fn run(
        &mut self,
        project: &Project,
        output: Option<Uuid>,
    ) -> Result<ExecutionResult, EngineError> {
        project.validate()?;
        let output = project.graph.output(output.or(project.output_node))?;
        let order = project.graph.execution_order(output)?;
        let request_id = Uuid::new_v4();
        let request = serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request_id,
            "operation": "run",
            "project": project,
            "output_node": output,
            "execution_order": order
        });
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| EngineError::Protocol("worker closed".into()))?;
        serde_json::to_writer(&mut *input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut line = String::new();
        // Limit each frame to avoid allocating an arbitrary response from a failed worker.
        let count = Read::by_ref(&mut self.output)
            .take(MAX_RESPONSE_BYTES + 1)
            .read_line(&mut line)?;
        if count == 0 {
            return Err(EngineError::Protocol(
                "worker exited before replying; see stderr".into(),
            ));
        }
        if count as u64 > MAX_RESPONSE_BYTES || !line.ends_with('\n') {
            return Err(EngineError::Protocol(
                "oversized or incomplete response".into(),
            ));
        }
        let response: Response = serde_json::from_str(&line)?;
        if response.protocol_version != PROTOCOL_VERSION || response.request_id != request_id {
            return Err(EngineError::Protocol(
                "version or request ID mismatch".into(),
            ));
        }
        if !response.ok {
            return Err(EngineError::Execution {
                node: response.node,
                message: response.message.unwrap_or_else(|| "unknown error".into()),
            });
        }
        Ok(ExecutionResult {
            tables: response.tables,
        })
    }

    /// Coarse cancellation: terminate the worker. Start a new session after cancellation.
    pub fn cancel(&mut self) -> Result<(), EngineError> {
        self.input.take();
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait()?;
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}
