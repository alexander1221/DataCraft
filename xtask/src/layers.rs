//! Cargo metadata policy. Specification and limitations: docs/ARCHITECTURE_CHECKS.md.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Layer {
    Types,
    Core,
    Graph,
    Nodes,
    Project,
    Planner,
    Connectors,
    Runtime,
    Engine,
    Cli,
    Mcp,
    Ui,
    Tooling,
}

impl Layer {
    fn allowed(self) -> &'static [Self] {
        use Layer::*;
        match self {
            Types => &[Types],
            Core => &[Core, Types],
            Graph => &[Graph, Types],
            Nodes => &[Nodes, Core, Types, Graph],
            Project => &[Project, Core, Types, Graph, Nodes],
            Planner => &[Planner, Core, Types, Graph, Nodes, Project],
            Connectors => &[Connectors, Core, Types, Graph, Nodes],
            Runtime => &[Runtime, Core, Types, Graph, Nodes, Planner, Connectors],
            Engine => &[
                Engine, Core, Types, Graph, Nodes, Project, Planner, Runtime, Connectors,
            ],
            Cli => &[Cli, Core, Types, Graph, Nodes, Project, Engine],
            Mcp => &[Mcp, Core, Types, Graph, Nodes, Engine],
            Ui => &[Ui, Core, Types, Graph, Nodes, Engine],
            Tooling => &[
                Types, Core, Graph, Nodes, Project, Planner, Connectors, Runtime, Engine, Cli, Mcp,
                Ui, Tooling,
            ],
        }
    }
    fn closure(self) -> BTreeSet<Self> {
        let mut found = BTreeSet::new();
        let mut pending = vec![self];
        while let Some(layer) = pending.pop() {
            if found.insert(layer) {
                pending.extend_from_slice(layer.allowed());
            }
        }
        found
    }
    fn foundational(self) -> bool {
        matches!(
            self,
            Self::Types | Self::Core | Self::Graph | Self::Nodes | Self::Project
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    resolve: Option<Resolve>,
}
#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    #[serde(default)]
    metadata: serde_json::Value,
}
#[derive(Debug, Deserialize)]
struct Resolve {
    nodes: Vec<ResolveNode>,
}
#[derive(Debug, Deserialize)]
struct ResolveNode {
    id: String,
    deps: Vec<Dependency>,
}
#[derive(Debug, Deserialize)]
struct Dependency {
    pkg: String,
    dep_kinds: Vec<DependencyKind>,
}
#[derive(Debug, Deserialize)]
struct DependencyKind {
    kind: Option<String>,
    target: Option<String>,
}

fn ui(name: &str) -> bool {
    matches!(
        name,
        "egui" | "eframe" | "winit" | "iced" | "druid" | "tauri" | "slint"
    ) || [
        "egui-", "egui_", "eframe-", "iced_", "iced-", "tauri-", "i-slint-",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}
fn datafusion(name: &str) -> bool {
    name == "datafusion" || name.starts_with("datafusion-") || name.starts_with("datafusion_")
}
fn edge_label(dep: &Dependency) -> String {
    let mut kinds: Vec<_> = dep
        .dep_kinds
        .iter()
        .map(|k| match &k.target {
            Some(target) => format!("{} on {target}", k.kind.as_deref().unwrap_or("normal")),
            None => k.kind.as_deref().unwrap_or("normal").to_owned(),
        })
        .collect();
    kinds.sort();
    kinds.join(", ")
}

pub fn check(metadata: &Metadata) -> Result<Vec<String>, String> {
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or("cargo metadata has no resolve graph")?;
    let packages: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let nodes: BTreeMap<_, _> = resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    if packages.len() != metadata.packages.len() || nodes.len() != resolve.nodes.len() {
        return Err("duplicate Cargo package or resolve ID".into());
    }
    let mut layers = BTreeMap::new();
    let mut errors = BTreeSet::new();
    for id in &metadata.workspace_members {
        let package = packages
            .get(id.as_str())
            .ok_or_else(|| format!("missing workspace package {id}"))?;
        let value = package
            .metadata
            .pointer("/datacraft/layer")
            .cloned()
            .unwrap_or_default();
        match serde_json::from_value::<Layer>(value) {
            Ok(layer) => {
                layers.insert(id.as_str(), layer);
            }
            Err(_) => {
                errors.insert(format!(
                    "{}: missing/unknown package.metadata.datacraft.layer",
                    package.name
                ));
            }
        }
    }
    if metadata.workspace_members.is_empty() {
        return Err("empty workspace membership".into());
    }
    for node in &resolve.nodes {
        if !packages.contains_key(node.id.as_str()) {
            return Err(format!("unknown resolve package {}", node.id));
        }
        for dep in &node.deps {
            if !packages.contains_key(dep.pkg.as_str()) || !nodes.contains_key(dep.pkg.as_str()) {
                return Err(format!("missing resolved dependency {}", dep.pkg));
            }
            if dep.dep_kinds.is_empty()
                || dep
                    .dep_kinds
                    .iter()
                    .any(|k| !matches!(k.kind.as_deref(), None | Some("dev" | "build")))
            {
                return Err(format!("unknown dependency kind for {}", dep.pkg));
            }
        }
    }
    for (root, layer) in &layers {
        if !nodes.contains_key(root) {
            return Err(format!("missing workspace resolve node {root}"));
        }
        let allowed = layer.closure();
        let mut visited = BTreeSet::from([*root]);
        let mut pending = VecDeque::from([(*root, packages[root].name.clone(), 0usize)]);
        while let Some((id, path, depth)) = pending.pop_front() {
            let node = nodes
                .get(id)
                .ok_or_else(|| format!("missing resolve node {id}"))?;
            let mut deps: Vec<_> = node.deps.iter().collect();
            deps.sort_by_key(|d| &d.pkg);
            for dep in deps {
                let package = packages[dep.pkg.as_str()];
                let trace = format!("{path} --[{}]--> {}", edge_label(dep), package.name);
                if let Some(target) = layers.get(dep.pkg.as_str()) {
                    if depth == 0 && !layer.allowed().contains(target) {
                        errors.insert(format!("forbidden direct workspace dependency: {trace}"));
                    } else if !allowed.contains(target) {
                        errors.insert(format!(
                            "forbidden transitive workspace dependency: {trace}"
                        ));
                    }
                }
                if *layer != Layer::Tooling {
                    if *layer != Layer::Ui && ui(&package.name) {
                        errors.insert(format!("UI dependency in headless layer: {trace}"));
                    }
                    if (layer.foundational() || (*layer == Layer::Ui && depth == 0))
                        && datafusion(&package.name)
                    {
                        errors.insert(format!(
                            "DataFusion dependency crosses domain/UI boundary: {trace}"
                        ));
                    }
                }
                if visited.insert(dep.pkg.as_str()) {
                    pending.push_back((dep.pkg.as_str(), trace, depth + 1));
                }
            }
        }
    }
    let members: BTreeSet<_> = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect();
    let mut colors = BTreeMap::new();
    let mut stack = Vec::new();
    for id in &members {
        cycles(
            id,
            &nodes,
            &packages,
            &members,
            &mut colors,
            &mut stack,
            &mut errors,
        );
    }
    Ok(errors.into_iter().collect())
}

fn cycles<'a>(
    id: &'a str,
    nodes: &BTreeMap<&'a str, &'a ResolveNode>,
    packages: &BTreeMap<&str, &Package>,
    members: &BTreeSet<&str>,
    colors: &mut BTreeMap<&'a str, u8>,
    stack: &mut Vec<&'a str>,
    errors: &mut BTreeSet<String>,
) {
    if colors.contains_key(id) {
        return;
    }
    colors.insert(id, 1);
    stack.push(id);
    if let Some(node) = nodes.get(id) {
        let mut deps: Vec<_> = node.deps.iter().collect();
        deps.sort_by_key(|d| &d.pkg);
        for dep in deps {
            let target = dep.pkg.as_str();
            if colors.get(target) == Some(&1) {
                if let Some(start) = stack.iter().position(|n| *n == target) {
                    if stack[start..].iter().any(|n| members.contains(n)) {
                        let path = stack[start..]
                            .iter()
                            .copied()
                            .chain([target])
                            .map(|n| packages[n].name.as_str())
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        errors.insert(format!("workspace dependency cycle: {path}"));
                    }
                }
            } else {
                cycles(target, nodes, packages, members, colors, stack, errors);
            }
        }
    }
    stack.pop();
    colors.insert(id, 2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn metadata(
        packages: Vec<(&str, Option<&str>)>,
        edges: Vec<(&str, &str, Option<&str>, Option<&str>)>,
    ) -> Metadata {
        serde_json::from_value(json!({
            "workspace_members":packages.iter().filter(|(_,l)|l.is_some()).map(|(n,_)|*n).collect::<Vec<_>>(),
            "packages":packages.iter().map(|(n,l)|json!({"id":n,"name":n,"metadata":l.map(|layer|json!({"datacraft":{"layer":layer}})).unwrap_or(Value::Null)})).collect::<Vec<_>>(),
            "resolve":{"nodes":packages.iter().map(|(n,_)|json!({"id":n,"deps":edges.iter().filter(|(from,_,_,_)|from==n).map(|(_,to,kind,target)|json!({"name":"renamed_alias","pkg":to,"dep_kinds":[{"kind":kind,"target":target}]})).collect::<Vec<_>>()})).collect::<Vec<_>>()}
        })).unwrap()
    }
    #[test]
    fn renamed_target_dev_build_and_transitive_ui_are_detected() {
        for kind in [None, Some("dev"), Some("build")] {
            let m = metadata(
                vec![("domain", Some("core")), ("wrapper", None), ("egui", None)],
                vec![
                    ("domain", "wrapper", kind, Some("cfg(windows)")),
                    ("wrapper", "egui", None, None),
                ],
            );
            let errors = check(&m).unwrap();
            assert!(errors.iter().any(|e| e.contains("UI dependency")
                && e.contains("wrapper")
                && e.contains("cfg(windows)")));
        }
    }
    #[test]
    fn ui_can_use_runtime_through_engine_but_not_directly() {
        let packages = vec![
            ("canvas", Some("ui")),
            ("app", Some("engine")),
            ("runner", Some("runtime")),
            ("datafusion", None),
            ("egui", None),
        ];
        let good = metadata(
            packages.clone(),
            vec![
                ("canvas", "app", None, None),
                ("app", "runner", None, None),
                ("runner", "datafusion", None, None),
                ("canvas", "egui", None, None),
            ],
        );
        assert!(check(&good).unwrap().is_empty());
        let bad = metadata(
            packages,
            vec![
                ("canvas", "runner", None, None),
                ("canvas", "datafusion", None, None),
            ],
        );
        let errors = check(&bad).unwrap();
        assert!(errors.iter().any(|e| e.contains("forbidden direct")));
        assert!(errors.iter().any(|e| e.contains("DataFusion")));
    }
    #[test]
    fn foundational_datafusion_and_upward_edges_are_rejected() {
        let m = metadata(
            vec![
                ("graph", Some("graph")),
                ("plan", Some("planner")),
                ("wrapper", None),
                ("datafusion-common", None),
            ],
            vec![
                ("graph", "wrapper", None, None),
                ("wrapper", "plan", None, None),
                ("wrapper", "datafusion-common", None, None),
            ],
        );
        let errors = check(&m).unwrap();
        assert!(errors.iter().any(|e| e.contains("transitive workspace")));
        assert!(errors.iter().any(|e| e.contains("DataFusion")));
    }
    #[test]
    fn cycle_including_dev_edges_is_rejected_and_layers_must_be_known() {
        let m = metadata(
            vec![("a", Some("core")), ("b", Some("core"))],
            vec![("a", "b", Some("dev"), None), ("b", "a", None, None)],
        );
        assert!(check(&m).unwrap().iter().any(|e| e.contains("cycle")));
        let mut m = metadata(vec![("a", Some("unknown"))], vec![]);
        assert!(!check(&m).unwrap().is_empty());
        m.packages[0].metadata = Value::Null;
        assert!(!check(&m).unwrap().is_empty());
        m.resolve = None;
        assert!(check(&m).is_err());
    }
    #[test]
    fn tooling_is_exempt_but_production_cannot_depend_on_it() {
        let m = metadata(
            vec![
                ("domain", Some("core")),
                ("tool", Some("tooling")),
                ("egui", None),
            ],
            vec![("tool", "egui", None, None)],
        );
        assert!(check(&m).unwrap().is_empty());
        let m = metadata(
            vec![("domain", Some("core")), ("tool", Some("tooling"))],
            vec![("domain", "tool", Some("build"), None)],
        );
        assert!(!check(&m).unwrap().is_empty());
    }
    #[test]
    fn missing_resolve_nodes_and_unknown_dependency_kinds_fail_closed() {
        let mut m = metadata(
            vec![("domain", Some("core")), ("helper", None)],
            vec![("domain", "helper", Some("future_kind"), None)],
        );
        assert!(check(&m).is_err());
        m.resolve.as_mut().unwrap().nodes[0].deps[0].dep_kinds[0].kind = None;
        m.resolve.as_mut().unwrap().nodes.pop();
        assert!(check(&m).is_err());
    }
}
