//! Deterministic DAG arrangement. Contract: docs/EDITOR_READABILITY.md.
use datacraft_core::{Graph, GraphError, PortType};
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn arranged_positions(graph: &Graph) -> Result<Vec<(Uuid, [f32; 2])>, GraphError> {
    let order = graph.validate_structure()?;
    let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (n.id, n)).collect();
    let mut parents: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
    for edge in &graph.edges {
        parents.entry(edge.to).or_default().push(edge.from);
    }
    let mut ranks = BTreeMap::new();
    let mut layers: BTreeMap<usize, Vec<Uuid>> = BTreeMap::new();
    for id in order {
        let rank = parents
            .get(&id)
            .into_iter()
            .flatten()
            .map(|p| ranks[p] + 1)
            .max()
            .unwrap_or(0);
        ranks.insert(id, rank);
        layers.entry(rank).or_default().push(id);
    }
    let mut centers: BTreeMap<Uuid, f32> = BTreeMap::new();
    let mut positions = BTreeMap::new();
    for (rank, ids) in layers {
        let mut desired: Vec<_> = ids
            .into_iter()
            .map(|id| {
                let upstream = parents.get(&id);
                let mut center = upstream.map_or(0.0, |ps| {
                    ps.iter().map(|p| centers[p]).sum::<f32>() / ps.len() as f32
                });
                let outputs = nodes[&id].config.spec().outputs;
                if outputs.len() == 1 && outputs[0].data_type == PortType::Model {
                    center -= 180.0;
                }
                (id, center)
            })
            .collect();
        desired.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let target_mean = desired.iter().map(|(_, y)| y).sum::<f32>() / desired.len() as f32;
        let mut bottom = f32::NEG_INFINITY;
        let mut placed = Vec::new();
        for (id, desired_center) in desired {
            let height = crate::canvas::size(nodes[&id]).y;
            let top = (desired_center - height * 0.5).max(bottom + 110.0);
            bottom = top + height;
            placed.push((id, top, height));
        }
        let shift = target_mean
            - placed.iter().map(|(_, y, h)| y + h * 0.5).sum::<f32>() / placed.len() as f32;
        for (id, top, height) in placed {
            let y = top + shift;
            centers.insert(id, y + height * 0.5);
            positions.insert(id, [rank as f32 * 340.0 + 40.0, y]);
        }
    }
    let min_y = positions
        .values()
        .map(|p| p[1])
        .reduce(f32::min)
        .unwrap_or(0.0);
    for point in positions.values_mut() {
        point[1] += 40.0 - min_y;
    }
    Ok(positions.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Graph {
        let mut session = datacraft_engine::Session::default();
        session
            .dispatch(datacraft_engine::Command::Open {
                path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../examples/pipelines/{name}.dcraft")),
                discard_changes: false,
            })
            .unwrap();
        session.project().unwrap().graph.clone()
    }
    #[test]
    fn layout_is_deterministic_forward_and_nonoverlapping_for_ml_branches() {
        let graph = fixture("logistic_regression");
        let expected = arranged_positions(&graph).unwrap();
        let positions: BTreeMap<_, _> = expected.iter().copied().collect();
        for edge in &graph.edges {
            assert!(positions[&edge.from][0] + 200.0 < positions[&edge.to][0]);
        }
        assert!(
            (positions[&Uuid::from_u128(2)][1] - positions[&Uuid::from_u128(3)][1]).abs() > 150.0
        );
        let rectangles: Vec<_> = graph
            .nodes
            .iter()
            .map(|n| {
                eframe::egui::Rect::from_min_size(positions[&n.id].into(), crate::canvas::size(n))
            })
            .collect();
        for (i, a) in rectangles.iter().enumerate() {
            for b in rectangles.iter().skip(i + 1) {
                assert!(!a.intersects(*b));
            }
        }
        let mut reversed = graph;
        reversed.nodes.reverse();
        reversed.edges.reverse();
        for node in &mut reversed.nodes {
            node.position = [-400.0, 800.0];
        }
        assert_eq!(arranged_positions(&reversed).unwrap(), expected);
    }
    #[test]
    fn empty_and_invalid_graphs_are_handled_without_partial_edits() {
        assert!(arranged_positions(&Graph::default()).unwrap().is_empty());
        let mut graph = fixture("train_test");
        graph.edges[0].from = Uuid::from_u128(999);
        assert!(arranged_positions(&graph).is_err());
    }
}
