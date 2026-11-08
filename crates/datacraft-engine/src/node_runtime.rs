//! Session-local materialized results. Contract: docs/NODE_EXECUTION.md.
use crate::{EngineError, Worker};
use datacraft_core::{Edge, Graph, GraphError, NodeKind};
use datacraft_project::Project;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    #[default]
    Idle,
    Queued,
    Running,
    Prepared,
    Completed,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(PartialEq)]
struct Fingerprint {
    config: NodeKind,
    edges: Vec<Edge>,
    parents: Vec<Uuid>,
}
struct Entry {
    fingerprint: Fingerprint,
    token: Uuid,
    status: NodeStatus,
    ready: bool,
}
pub(crate) fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|p| p.into_inner())
}
#[derive(Default)]
pub(crate) struct WorkerSlot(pub Mutex<Option<Worker>>);
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        if let Some(mut worker) = lock(&self.0).take() {
            // Issue the stop before scheduling cleanup, so immediate host exit
            // cannot leave an idle worker waiting on stdin. Reaping stays off UI.
            worker.close_input();
            let _ = worker.cancellation_handle().terminate();
            let _ = std::thread::Builder::new()
                .name("datacraft-worker-cleanup".into())
                .spawn(move || drop(worker));
        }
    }
}
#[derive(Clone, Default)]
pub struct NodeRuntime {
    entries: Arc<Mutex<BTreeMap<Uuid, Entry>>>,
    pub(crate) worker: Arc<WorkerSlot>,
}
impl NodeRuntime {
    pub fn statuses(&self) -> BTreeMap<Uuid, NodeStatus> {
        lock(&self.entries)
            .iter()
            .map(|(id, e)| (*id, e.status))
            .collect()
    }
    pub(crate) fn sync(&self, project: &Project) -> Result<(), GraphError> {
        let order = project.graph.validate_structure()?;
        let mut entries = lock(&self.entries);
        entries.retain(|id, _| order.contains(id));
        let by_id: BTreeMap<_, _> = project.graph.nodes.iter().map(|n| (n.id, n)).collect();
        for id in order {
            let mut edges: Vec<_> = project
                .graph
                .edges
                .iter()
                .filter(|e| e.to == id)
                .cloned()
                .collect();
            edges.sort_by(|a, b| {
                (&a.to_port, a.from, &a.from_port).cmp(&(&b.to_port, b.from, &b.from_port))
            });
            let parents = edges
                .iter()
                .filter_map(|e| entries.get(&e.from).map(|p| p.token))
                .collect();
            let fingerprint = Fingerprint {
                config: by_id[&id].config.clone(),
                edges,
                parents,
            };
            if !entries
                .get(&id)
                .is_some_and(|e| e.fingerprint == fingerprint)
            {
                entries.insert(
                    id,
                    Entry {
                        fingerprint,
                        token: Uuid::new_v4(),
                        status: NodeStatus::Idle,
                        ready: false,
                    },
                );
            }
        }
        Ok(())
    }
    pub(crate) fn tokens(&self) -> BTreeMap<Uuid, Uuid> {
        lock(&self.entries)
            .iter()
            .map(|(id, e)| (*id, e.token))
            .collect()
    }
    pub(crate) fn update(&self, tokens: &BTreeMap<Uuid, Uuid>, id: Uuid, status: NodeStatus) {
        self.display(tokens, id, status, true);
    }
    pub(crate) fn display(
        &self,
        tokens: &BTreeMap<Uuid, Uuid>,
        id: Uuid,
        status: NodeStatus,
        cached: bool,
    ) {
        if let Some(e) = lock(&self.entries).get_mut(&id) {
            if tokens.get(&id) == Some(&e.token) {
                e.status = status;
                if cached {
                    e.ready = status == NodeStatus::Completed;
                }
            }
        }
    }
    pub(crate) fn lost_worker(&self) {
        for e in lock(&self.entries).values_mut() {
            e.token = Uuid::new_v4();
            e.ready = false;
            e.status = match e.status {
                NodeStatus::Running | NodeStatus::Queued => NodeStatus::Cancelled,
                NodeStatus::Failed => NodeStatus::Failed,
                _ => NodeStatus::Idle,
            };
        }
    }
    pub(crate) fn settle(&self, failure: Option<Uuid>, cancelled: bool) {
        for (id, e) in lock(&self.entries).iter_mut() {
            if failure == Some(*id)
                && matches!(
                    e.status,
                    NodeStatus::Queued | NodeStatus::Running | NodeStatus::Prepared
                )
            {
                e.status = NodeStatus::Failed;
            } else if matches!(
                e.status,
                NodeStatus::Queued | NodeStatus::Running | NodeStatus::Prepared
            ) {
                e.status = if cancelled {
                    NodeStatus::Cancelled
                } else {
                    NodeStatus::Idle
                };
            }
        }
    }
    pub(crate) fn prepare(
        &self,
        project: &Project,
        node: Uuid,
        downstream: bool,
    ) -> Result<Vec<Uuid>, EngineError> {
        self.sync(project)?;
        let order = project.graph.validate_structure()?;
        if !order.contains(&node) {
            return Err(GraphError::MissingNode(node).into());
        }
        let selected = descendants(&project.graph, BTreeSet::from([node]));
        let selected = if downstream {
            selected
        } else {
            BTreeSet::from([node])
        };
        for id in &selected {
            project.graph.dependency_order(*id)?;
        }
        {
            let entries = lock(&self.entries);
            for edge in &project.graph.edges {
                if selected.contains(&edge.to)
                    && !selected.contains(&edge.from)
                    && !entries.get(&edge.from).is_some_and(|e| e.ready)
                {
                    return Err(EngineError::Execution {
                        node: Some(edge.to),
                        message: format!(
                            "Run input node {} first (port {})",
                            edge.from, edge.to_port
                        ),
                    });
                }
            }
        }
        let invalid = descendants(&project.graph, selected.clone());
        {
            let mut entries = lock(&self.entries);
            for id in invalid {
                if let Some(e) = entries.get_mut(&id) {
                    e.token = Uuid::new_v4();
                    e.status = NodeStatus::Idle;
                    e.ready = false;
                }
            }
        }
        self.sync(project)?;
        Ok(order
            .into_iter()
            .filter(|id| selected.contains(id))
            .collect())
    }
}
fn descendants(graph: &Graph, mut ids: BTreeSet<Uuid>) -> BTreeSet<Uuid> {
    let mut pending: Vec<_> = ids.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for edge in graph.edges.iter().filter(|e| e.from == id) {
            if ids.insert(edge.to) {
                pending.push(edge.to);
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        Project::from_json(include_str!("../../../examples/pipelines/sample.dcraft")).unwrap()
    }
    // Explicit state-machine fixtures; these tests do not stand in for Spark execution.
    fn mark_ready(runtime: &NodeRuntime) {
        for id in runtime.tokens().keys() {
            runtime.update(&runtime.tokens(), *id, NodeStatus::Completed);
        }
    }
    #[test]
    fn strict_run_requires_ready_inputs_and_downstream_has_deterministic_scope() {
        let p = project();
        let r = NodeRuntime::default();
        assert!(r.prepare(&p, Uuid::from_u128(2), false).is_err());
        assert_eq!(
            r.prepare(&p, Uuid::from_u128(1), false).unwrap(),
            vec![Uuid::from_u128(1)]
        );
        r.update(&r.tokens(), Uuid::from_u128(1), NodeStatus::Completed);
        assert_eq!(
            r.prepare(&p, Uuid::from_u128(2), true).unwrap(),
            (2..=5).map(Uuid::from_u128).collect::<Vec<_>>()
        );
        assert!(r.prepare(&p, Uuid::nil(), false).is_err());
    }
    #[test]
    fn edits_invalidate_only_dependencies_and_ignore_late_events() {
        let mut p = project();
        let r = NodeRuntime::default();
        r.sync(&p).unwrap();
        mark_ready(&r);
        let before = r.tokens();
        p.graph.nodes[1].position = [99.0, 50.0];
        p.name = "changed name".into();
        p.output_node = None;
        r.sync(&p).unwrap();
        assert_eq!(before, r.tokens());
        p.graph.nodes[1].config = NodeKind::Filter {
            expression: "quantity > 9".into(),
        };
        r.sync(&p).unwrap();
        assert_eq!(r.statuses()[&Uuid::from_u128(1)], NodeStatus::Completed);
        for id in (2..=5).map(Uuid::from_u128) {
            assert_ne!(r.tokens()[&id], before[&id]);
            r.update(&before, id, NodeStatus::Completed);
            assert_eq!(r.statuses()[&id], NodeStatus::Idle);
        }
    }
    #[test]
    fn lazy_success_does_not_supply_a_materialized_input_and_loss_clears_cache() {
        let p = project();
        let r = NodeRuntime::default();
        r.sync(&p).unwrap();
        r.display(
            &r.tokens(),
            Uuid::from_u128(1),
            NodeStatus::Completed,
            false,
        );
        assert!(r.prepare(&p, Uuid::from_u128(2), false).is_err());
        mark_ready(&r);
        r.lost_worker();
        assert!(r.prepare(&p, Uuid::from_u128(2), false).is_err());
    }
    #[test]
    fn explicit_rerun_invalidates_descendants_but_not_unrelated_branch() {
        let mut p = project();
        let mut independent = p.graph.nodes[0].clone();
        independent.id = Uuid::from_u128(20);
        p.graph.nodes.push(independent);
        let r = NodeRuntime::default();
        r.sync(&p).unwrap();
        mark_ready(&r);
        r.prepare(&p, Uuid::from_u128(2), false).unwrap();
        assert_eq!(r.statuses()[&Uuid::from_u128(20)], NodeStatus::Completed);
        assert_eq!(r.statuses()[&Uuid::from_u128(1)], NodeStatus::Completed);
        for id in (2..=5).map(Uuid::from_u128) {
            assert_eq!(r.statuses()[&id], NodeStatus::Idle);
        }
    }

    #[test]
    fn rewiring_invalidates_consumers_and_disconnection_rejects_a_strict_run() {
        let mut p = project();
        let mut source = p.graph.nodes[0].clone();
        source.id = Uuid::from_u128(20);
        p.graph.nodes.push(source);
        let r = NodeRuntime::default();
        r.sync(&p).unwrap();
        mark_ready(&r);
        let before = r.tokens();
        p.graph
            .edges
            .iter_mut()
            .find(|e| e.to == Uuid::from_u128(2))
            .unwrap()
            .from = Uuid::from_u128(20);
        r.sync(&p).unwrap();
        for id in [Uuid::from_u128(1), Uuid::from_u128(20)] {
            assert_eq!(r.tokens()[&id], before[&id]);
            assert_eq!(r.statuses()[&id], NodeStatus::Completed);
        }
        for id in (2..=5).map(Uuid::from_u128) {
            assert_ne!(r.tokens()[&id], before[&id]);
            assert_eq!(r.statuses()[&id], NodeStatus::Idle);
        }
        assert!(r.prepare(&p, Uuid::from_u128(2), false).is_ok());
        p.graph.edges.retain(|e| e.to != Uuid::from_u128(2));
        assert!(r.prepare(&p, Uuid::from_u128(2), false).is_err());
    }
}
