//! Single-job, blocking worker client. Use from a background thread in a future UI.
pub mod commands;
pub mod jobs;
pub mod node_runtime;
pub mod session;
pub use commands::{Command, CommandSpec, COMMANDS};
use datacraft_project::Project;
use serde::{Deserialize, Serialize};
pub use session::{CommandPayload, Session, SessionError, WorkerConfig};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio},
    sync::{Arc, Mutex},
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    child: Arc<Mutex<Child>>,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

/// Cloneable process control; it never needs the blocking response reader.
#[derive(Clone)]
pub struct WorkerCancellation {
    child: Arc<Mutex<Child>>,
}

impl WorkerCancellation {
    pub fn terminate(&self) -> Result<(), EngineError> {
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        Ok(())
    }
}

impl Worker {
    /// The classpath is produced by `sbt writeClasspath`; no shell is involved.
    pub fn start(java: &Path, classpath_file: &Path) -> Result<Self, EngineError> {
        let classpath = fs::read_to_string(classpath_file)?;
        let mut command = ProcessCommand::new(java);
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
            child: Arc::new(Mutex::new(child)),
            input: Some(input),
            output: BufReader::new(output),
        })
    }

    pub fn run(
        &mut self,
        project: &Project,
        output: Option<Uuid>,
    ) -> Result<ExecutionResult, EngineError> {
        self.run_with_progress(project, output, |_, _| {})
    }

    pub fn run_with_progress(
        &mut self,
        project: &Project,
        output: Option<Uuid>,
        progress: impl FnMut(Uuid, node_runtime::NodeStatus),
    ) -> Result<ExecutionResult, EngineError> {
        project.validate_structure()?;
        let output = project.graph.output(output.or(project.output_node))?;
        let order = project.graph.execution_order(output)?;
        let request_id = Uuid::new_v4();
        let request = serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request_id,
            "operation": "run",
            "project": project,
            "output_node": output,
            "execution_order": order,
            "progress": true
        });
        self.exchange(request_id, request, &order, progress)
    }

    pub(crate) fn run_nodes(
        &mut self,
        project: &Project,
        order: &[Uuid],
        tokens: &std::collections::BTreeMap<Uuid, Uuid>,
        progress: impl FnMut(Uuid, node_runtime::NodeStatus),
    ) -> Result<ExecutionResult, EngineError> {
        let request_id = Uuid::new_v4();
        let output = order
            .last()
            .ok_or_else(|| EngineError::Protocol("empty node execution".into()))?;
        let request = serde_json::json!({
            "protocol_version": PROTOCOL_VERSION, "request_id": request_id,
            "operation": "run_nodes", "project": project, "output_node": output,
            "execution_order": order, "cache_tokens": tokens, "progress": true
        });
        self.exchange(request_id, request, order, progress)
    }

    fn exchange(
        &mut self,
        request_id: Uuid,
        request: serde_json::Value,
        order: &[Uuid],
        progress: impl FnMut(Uuid, node_runtime::NodeStatus),
    ) -> Result<ExecutionResult, EngineError> {
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| EngineError::Protocol("worker closed".into()))?;
        serde_json::to_writer(&mut *input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        Self::read_response(
            &mut self.output,
            request_id,
            order,
            request["operation"] == "run_nodes",
            progress,
        )
    }

    fn read_response(
        reader: &mut impl BufRead,
        request_id: Uuid,
        order: &[Uuid],
        cached: bool,
        mut progress: impl FnMut(Uuid, node_runtime::NodeStatus),
    ) -> Result<ExecutionResult, EngineError> {
        let mut index = 0;
        let mut started = false;
        let mut saw_progress = false;
        for _ in 0..order.len().saturating_mul(2).saturating_add(1) {
            let mut line = String::new();
            // Limit each frame to avoid allocating an arbitrary response from a failed worker.
            let count = Read::by_ref(reader)
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
            let frame: serde_json::Value = serde_json::from_str(&line)?;
            if frame.get("event").is_some() {
                let version = frame.get("protocol_version").and_then(|v| v.as_u64());
                let id = frame
                    .get("request_id")
                    .and_then(|v| v.as_str())
                    .and_then(|v| Uuid::parse_str(v).ok());
                let node = frame
                    .get("node")
                    .and_then(|v| v.as_str())
                    .and_then(|v| Uuid::parse_str(v).ok());
                let status = match frame.get("phase").and_then(|v| v.as_str()) {
                    Some("running") => node_runtime::NodeStatus::Running,
                    Some("prepared") => node_runtime::NodeStatus::Prepared,
                    Some("completed") => node_runtime::NodeStatus::Completed,
                    _ => return Err(EngineError::Protocol("invalid node progress phase".into())),
                };
                if version != Some(PROTOCOL_VERSION.into())
                    || id != Some(request_id)
                    || frame["event"] != "node"
                    || !node.is_some_and(|id| order.contains(&id))
                {
                    return Err(EngineError::Protocol(
                        "invalid node progress identity".into(),
                    ));
                }
                if let Some(node) = node {
                    if order.get(index) != Some(&node)
                        || (!started && status != node_runtime::NodeStatus::Running)
                        || (started
                            && status
                                != if cached {
                                    node_runtime::NodeStatus::Completed
                                } else {
                                    node_runtime::NodeStatus::Prepared
                                })
                    {
                        return Err(EngineError::Protocol(
                            "invalid node progress sequence".into(),
                        ));
                    }
                    saw_progress = true;
                    if started {
                        index += 1;
                    }
                    started = !started;
                    progress(node, status);
                }
                continue;
            }
            let response: Response = serde_json::from_value(frame)?;
            if response.protocol_version != PROTOCOL_VERSION || response.request_id != request_id {
                return Err(EngineError::Protocol(
                    "version or request ID mismatch".into(),
                ));
            }
            if !response.ok {
                if response.node.is_some_and(|node| !order.contains(&node)) {
                    return Err(EngineError::Protocol(
                        "failure refers to an unrelated node".into(),
                    ));
                }
                return Err(EngineError::Execution {
                    node: response.node,
                    message: response.message.unwrap_or_else(|| "unknown error".into()),
                });
            }
            if (cached || saw_progress) && (started || index != order.len()) {
                return Err(EngineError::Protocol(
                    "missing node completion frames".into(),
                ));
            }
            return Ok(ExecutionResult {
                tables: response.tables,
            });
        }
        Err(EngineError::Protocol(
            "too many worker progress frames".into(),
        ))
    }

    /// Coarse cancellation: terminate the worker. Start a new session after cancellation.
    pub fn cancel(&mut self) -> Result<(), EngineError> {
        self.close_input();
        let termination = self.cancellation_handle().terminate();
        self.child
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .wait()?;
        termination?;
        Ok(())
    }

    pub(crate) fn close_input(&mut self) {
        self.input.take();
    }

    pub fn cancellation_handle(&self) -> WorkerCancellation {
        WorkerCancellation {
            child: Arc::clone(&self.child),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    fn read(frames: Vec<serde_json::Value>, cached: bool) -> Result<ExecutionResult, EngineError> {
        let text = frames
            .into_iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>();
        Worker::read_response(
            &mut std::io::Cursor::new(text),
            Uuid::from_u128(99),
            &[Uuid::from_u128(1)],
            cached,
            |_, _| {},
        )
    }
    fn event(phase: &str) -> serde_json::Value {
        serde_json::json!({"protocol_version": 1, "request_id": Uuid::from_u128(99), "event": "node", "node": Uuid::from_u128(1), "phase": phase})
    }
    fn done() -> serde_json::Value {
        serde_json::json!({"protocol_version": 1, "request_id": Uuid::from_u128(99), "ok": true, "tables": []})
    }
    #[test]
    fn node_progress_has_verified_identity_and_order_not_just_valid_json() {
        assert!(read(vec![event("running"), event("completed"), done()], true).is_ok());
        assert!(read(vec![event("running"), event("prepared"), done()], false).is_ok());
        assert!(read(vec![done()], false).is_ok()); // previous worker compatibility
        assert!(read(vec![done()], true).is_err());
        assert!(read(vec![event("completed"), done()], true).is_err());
        assert!(read(vec![event("running"), event("running"), done()], true).is_err());
        assert!(read(vec![event("running"), done()], true).is_err());
        for field in ["request_id", "node", "protocol_version", "event", "phase"] {
            let mut wrong = event("running");
            wrong[field] = serde_json::json!("invalid");
            assert!(read(vec![wrong, done()], true).is_err(), "{field}");
        }
    }
    #[test]
    fn recoverable_failure_after_node_start_and_incomplete_frames() {
        let failure = serde_json::json!({"protocol_version":1,"request_id":Uuid::from_u128(99),"ok":false,"node":Uuid::from_u128(1),"message":"bad column"});
        assert!(matches!(
            read(vec![event("running"), failure], true),
            Err(EngineError::Execution { node: Some(_), .. })
        ));
        assert!(Worker::read_response(
            &mut std::io::Cursor::new("{}"),
            Uuid::nil(),
            &[],
            false,
            |_, _| {}
        )
        .is_err());
        assert!(Worker::read_response(
            &mut std::io::Cursor::new(""),
            Uuid::nil(),
            &[],
            false,
            |_, _| {}
        )
        .is_err());
    }
}
