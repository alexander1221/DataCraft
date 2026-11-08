//! Presentation-only canvas. Persistent moves are dispatched by Desktop after drag ends.
use datacraft_core::{Edge, Graph, Node, PortType};
use datacraft_engine::node_runtime::NodeStatus;
use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use std::collections::BTreeMap;
use uuid::Uuid;

pub enum Action {
    Select(Uuid),
    Move(Uuid, [f32; 2]),
    Connect(Edge),
    RunNode(Uuid),
    RunDownstream(Uuid),
}

struct OutputPort {
    node: Uuid,
    port: &'static str,
    data_type: PortType,
}

struct Drag {
    node: Uuid,
    start: [f32; 2],
    position: [f32; 2],
}

pub struct Canvas {
    pan: Vec2,
    zoom: f32,
    pub fit: bool,
    drag: Option<Drag>,
    connection: Option<OutputPort>,
    center: Vec2,
    pub execution: BTreeMap<Uuid, NodeStatus>,
    pub run_enabled: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            pan: Vec2::splat(32.0),
            zoom: 1.0,
            fit: true,
            drag: None,
            connection: None,
            center: egui::vec2(320.0, 240.0),
            execution: BTreeMap::new(),
            run_enabled: true,
        }
    }
}

impl Canvas {
    pub fn cancel_gestures(&mut self) {
        self.drag = None;
        self.connection = None;
    }
    pub fn new_position(&self, count: usize) -> [f32; 2] {
        let position = (self.center - self.pan) / self.zoom - egui::vec2(100.0, 40.0)
            + Vec2::splat((count % 7) as f32 * 24.0);
        [position.x, position.y]
    }
    fn screen(&self, origin: Pos2, position: [f32; 2]) -> Pos2 {
        origin + self.pan + egui::vec2(position[0], position[1]) * self.zoom
    }

    fn fit_graph(&mut self, graph: &Graph, available: Vec2) {
        if graph.nodes.is_empty() {
            return;
        }
        let mut bounds = Rect::NOTHING;
        for node in &graph.nodes {
            bounds = bounds.union(Rect::from_min_size(
                Pos2::new(node.position[0], node.position[1]),
                size(node),
            ));
        }
        if !bounds.size().is_finite() {
            return;
        }
        self.zoom = ((available.x - 64.0) / bounds.width())
            .min((available.y - 64.0) / bounds.height())
            .clamp(0.2, 1.25);
        self.pan = available * 0.5 - bounds.center().to_vec2() * self.zoom;
    }

    fn zoom_at(&mut self, anchor: Vec2, factor: f32) {
        let world = (anchor - self.pan) / self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.2, 2.0);
        self.pan = anchor - world * self.zoom;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        graph: &Graph,
        selected: Option<Uuid>,
    ) -> Vec<Action> {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        self.center = rect.size() * 0.5;
        let cancel = ui.input(|i| {
            i.key_pressed(egui::Key::Escape)
                || (i.pointer.button_clicked(PointerButton::Secondary)
                    && i.pointer.hover_pos().is_some_and(|p| rect.contains(p)))
        });
        if cancel {
            self.cancel_gestures();
        }
        if self.connection.as_ref().is_some_and(|from| {
            !graph.nodes.iter().any(|n| {
                n.id == from.node && n.config.spec().outputs.iter().any(|p| p.id == from.port)
            })
        }) {
            self.connection = None;
        }
        if self.fit {
            self.fit_graph(graph, rect.size());
            self.fit = false;
        }
        if response.dragged_by(PointerButton::Middle) {
            self.pan += ui.input(|i| i.pointer.delta());
        }
        if response.hovered() && self.drag.is_none() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(pointer) = response.hover_pos() {
                    self.zoom_at(pointer - rect.min, (scroll * 0.002).exp());
                }
            }
        }
        painter.rect_filled(rect, 0.0, Color32::from_rgb(19, 23, 32));
        let spacing = 32.0 * self.zoom;
        let offset = egui::vec2(
            self.pan.x.rem_euclid(spacing),
            self.pan.y.rem_euclid(spacing),
        );
        let mut x = rect.left() + offset.x;
        while x < rect.right() {
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.5_f32, Color32::from_gray(36)),
            );
            x += spacing;
        }
        let mut y = rect.top() + offset.y;
        while y < rect.bottom() {
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(0.5_f32, Color32::from_gray(36)),
            );
            y += spacing;
        }
        let mut geometry = BTreeMap::new();
        for node in &graph.nodes {
            let position = self
                .drag
                .as_ref()
                .filter(|d| d.node == node.id)
                .map_or(node.position, |d| d.position);
            let bounds =
                Rect::from_min_size(self.screen(rect.min, position), size(node) * self.zoom);
            if bounds.is_finite() {
                geometry.insert(node.id, bounds);
            }
        }
        let by_id: BTreeMap<_, _> = graph.nodes.iter().map(|n| (n.id, n)).collect();
        if let Some(from) = &self.connection {
            if let (Some(bounds), Some(node), Some(pointer)) = (
                geometry.get(&from.node),
                by_id.get(&from.node),
                ui.input(|i| i.pointer.hover_pos()),
            ) {
                if let Some(index) = node
                    .config
                    .spec()
                    .outputs
                    .iter()
                    .position(|p| p.id == from.port)
                {
                    painter.line_segment(
                        [port(*bounds, index, true, self.zoom), pointer],
                        Stroke::new(2.0_f32, Color32::YELLOW),
                    );
                }
            }
        }
        for edge in &graph.edges {
            if let (Some(from), Some(to), Some(from_node), Some(to_node)) = (
                geometry.get(&edge.from),
                geometry.get(&edge.to),
                by_id.get(&edge.from),
                by_id.get(&edge.to),
            ) {
                let output = from_node
                    .config
                    .spec()
                    .outputs
                    .iter()
                    .position(|p| p.id == edge.from_port);
                let input = to_node
                    .config
                    .spec()
                    .inputs
                    .iter()
                    .position(|p| p.id == edge.to_port);
                if let (Some(out), Some(inp)) = (output, input) {
                    let a = port(*from, out, true, self.zoom);
                    let b = port(*to, inp, false, self.zoom);
                    let reach = ((b.x - a.x).abs() * 0.5).max(32.0 * self.zoom);
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [a, a + egui::vec2(reach, 0.0), b - egui::vec2(reach, 0.0), b],
                        false,
                        Color32::TRANSPARENT,
                        Stroke::new(
                            2.0_f32,
                            match from_node.config.spec().outputs[out].data_type {
                                PortType::Model => Color32::from_rgb(211, 163, 96),
                                PortType::Table => Color32::from_rgb(90, 155, 210),
                            },
                        ),
                    ));
                }
            }
        }
        let mut actions = Vec::new();
        for node in &graph.nodes {
            let Some(bounds) = geometry.get(&node.id).copied() else {
                continue;
            };
            if !bounds.intersects(rect) {
                continue;
            }
            let body = Rect::from_min_max(
                bounds.min,
                bounds.right_bottom() - egui::vec2(0.0, 28.0 * self.zoom),
            );
            let hit = ui.interact(
                body.intersect(rect),
                ui.id().with(node.id),
                Sense::click_and_drag(),
            );
            if hit.clicked() || hit.drag_started_by(PointerButton::Primary) {
                actions.push(Action::Select(node.id));
            }
            if hit.drag_started_by(PointerButton::Primary) && !cancel {
                self.drag = Some(Drag {
                    node: node.id,
                    start: node.position,
                    position: node.position,
                });
            }
            if let Some(drag) = self.drag.as_mut().filter(|d| d.node == node.id) {
                if let Some(delta) = hit.total_drag_delta().map(|d| d / self.zoom) {
                    drag.position = [drag.start[0] + delta.x, drag.start[1] + delta.y];
                }
                if hit.drag_stopped() {
                    actions.push(Action::Move(node.id, drag.position));
                    self.drag = None;
                }
            }
            let status = self.execution.get(&node.id).copied().unwrap_or_default();
            let status_color = match status {
                NodeStatus::Completed | NodeStatus::Succeeded => Color32::from_rgb(74, 210, 126),
                NodeStatus::Failed => Color32::from_rgb(245, 91, 99),
                _ => Color32::from_gray(130),
            };
            let accent = if matches!(
                status,
                NodeStatus::Completed | NodeStatus::Succeeded | NodeStatus::Failed
            ) {
                status_color
            } else if selected == Some(node.id) {
                Color32::from_rgb(97, 196, 255)
            } else {
                Color32::from_gray(76)
            };
            painter.rect_filled(bounds, 6.0, Color32::from_rgb(35, 43, 57));
            painter.rect_stroke(
                bounds,
                6.0,
                Stroke::new(1.5_f32, accent),
                StrokeKind::Inside,
            );
            if selected == Some(node.id) {
                painter.rect_stroke(
                    bounds.expand(3.0 * self.zoom),
                    7.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(97, 196, 255)),
                    StrokeKind::Outside,
                );
            }
            let spec = node.config.spec();
            painter
                .with_clip_rect(Rect::from_min_max(
                    bounds.min,
                    bounds.max - egui::vec2(28.0 * self.zoom, 0.0),
                ))
                .text(
                    bounds.min + egui::vec2(12.0, 18.0) * self.zoom,
                    Align2::LEFT_CENTER,
                    spec.label,
                    FontId::proportional(15.0 * self.zoom),
                    Color32::WHITE,
                );
            if status == NodeStatus::Running {
                let center = bounds.right_top() + egui::vec2(-15.0, 18.0) * self.zoom;
                let angle = ui.input(|i| i.time) as f32 * 5.0;
                let points: Vec<_> = (0..24)
                    .map(|i| {
                        let a = angle + i as f32 * 0.2;
                        center + egui::vec2(a.cos(), a.sin()) * (7.0 * self.zoom)
                    })
                    .collect();
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(2.0 * self.zoom, Color32::from_rgb(108, 208, 255)),
                ));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
            for (index, downstream) in [false, true].into_iter().enumerate() {
                let button = run_button(bounds, index, self.zoom);
                let response = ui.interact(
                    button.intersect(rect),
                    ui.id().with((node.id, "run", downstream)),
                    Sense::click(),
                );
                let enabled = self.run_enabled && ui.is_enabled();
                let color = if enabled {
                    Color32::from_rgb(73, 215, 126)
                } else {
                    Color32::from_gray(85)
                };
                painter.rect_filled(
                    button,
                    3.0,
                    if response.hovered() && enabled {
                        Color32::from_rgb(46, 82, 65)
                    } else {
                        Color32::from_rgb(24, 35, 34)
                    },
                );
                let count = if downstream { 2 } else { 1 };
                for part in 0..count {
                    let start = button.center()
                        + egui::vec2(
                            if downstream {
                                -7.0 + part as f32 * 7.0
                            } else {
                                -3.0
                            },
                            -5.0,
                        ) * self.zoom;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            start,
                            start + egui::vec2(0.0, 10.0) * self.zoom,
                            start + egui::vec2(7.0, 5.0) * self.zoom,
                        ],
                        color,
                        Stroke::NONE,
                    ));
                }
                if enabled && response.clicked() {
                    actions.push(if downstream {
                        Action::RunDownstream(node.id)
                    } else {
                        Action::RunNode(node.id)
                    });
                }
                response.on_hover_text(if downstream {
                    "Run this node and all descendants. Inputs outside the branch must be cached."
                } else {
                    "Run only this node. Run its input nodes first."
                });
            }
            painter.text(
                bounds.left_bottom() + egui::vec2(80.0, -16.0) * self.zoom,
                Align2::LEFT_CENTER,
                status_label(status),
                FontId::proportional(11.0 * self.zoom),
                status_color,
            );
            for (ports, output) in [(spec.inputs, false), (spec.outputs, true)] {
                for (index, p) in ports.iter().enumerate() {
                    let center = port(bounds, index, output, self.zoom);
                    let interaction = ui.interact(
                        Rect::from_center_size(center, Vec2::splat(18.0)).intersect(rect),
                        ui.id().with((node.id, output, p.id)),
                        Sense::click(),
                    );
                    if interaction.clicked() {
                        actions.push(Action::Select(node.id));
                        if output {
                            self.connection = Some(OutputPort {
                                node: node.id,
                                port: p.id,
                                data_type: p.data_type,
                            });
                        } else if let Some(from) = self.connection.take() {
                            actions.push(Action::Connect(Edge {
                                from: from.node,
                                from_port: from.port.into(),
                                to: node.id,
                                to_port: p.id.into(),
                            }));
                        }
                    }
                    interaction.on_hover_text(format!(
                        "{} ({:?}): {}",
                        p.label,
                        p.data_type,
                        if output {
                            "click to start a connection"
                        } else {
                            "click to finish a connection"
                        }
                    ));
                    let color = match p.data_type {
                        PortType::Table => Color32::from_rgb(91, 186, 220),
                        PortType::Model => Color32::from_rgb(211, 163, 96),
                    };
                    painter.circle_filled(center, 4.0 * self.zoom, color);
                    if self.connection.as_ref().is_some_and(|from| {
                        (!output && from.data_type == p.data_type)
                            || (output && from.node == node.id && from.port == p.id)
                    }) {
                        painter.circle_stroke(
                            center,
                            7.0 * self.zoom,
                            Stroke::new(1.5_f32, Color32::YELLOW),
                        );
                    }
                    let offset = if output { -10.0 } else { 10.0 };
                    painter.text(
                        center + egui::vec2(offset * self.zoom, 0.0),
                        if output {
                            Align2::RIGHT_CENTER
                        } else {
                            Align2::LEFT_CENTER
                        },
                        p.id,
                        FontId::monospace(12.0 * self.zoom),
                        color,
                    );
                }
            }
        }
        // Releasing outside the clipped canvas must not leave a dangling drag.
        if !ui.input(|i| i.pointer.primary_down()) {
            if let Some(drag) = self.drag.take() {
                actions.push(Action::Move(drag.node, drag.position));
            }
        }
        if graph.nodes.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Add nodes from the palette to build a pipeline",
                FontId::proportional(18.0),
                Color32::GRAY,
            );
        }
        actions
    }
}

pub(crate) fn size(node: &Node) -> Vec2 {
    let spec = node.config.spec();
    egui::vec2(
        200.0,
        88.0 + 22.0 * spec.inputs.len().max(spec.outputs.len()) as f32,
    )
}
fn run_button(bounds: Rect, index: usize, zoom: f32) -> Rect {
    Rect::from_min_size(
        bounds.left_bottom() + egui::vec2(12.0 + index as f32 * 30.0, -26.0) * zoom,
        egui::vec2(24.0, 20.0) * zoom,
    )
}
fn status_label(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Idle => "",
        NodeStatus::Queued => "Waiting",
        NodeStatus::Running => "Running",
        NodeStatus::Prepared => "Plan ready",
        NodeStatus::Completed => "Done (cached)",
        NodeStatus::Succeeded => "Done (pipeline)",
        NodeStatus::Failed => "Failed",
        NodeStatus::Cancelled => "Stopped",
    }
}
fn port(rect: Rect, index: usize, output: bool, zoom: f32) -> Pos2 {
    Pos2::new(
        if output { rect.right() } else { rect.left() },
        rect.top() + (48.0 + 22.0 * index as f32) * zoom,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_buttons_emit_only_the_requested_run_and_obey_busy_state() {
        let id = Uuid::from_u128(1);
        let graph = Graph {
            nodes: vec![Node {
                id,
                position: [0.0, 0.0],
                config: datacraft_core::NodeKind::CsvSource {
                    path: "local.csv".into(),
                    header: true,
                    infer_schema: true,
                },
            }],
            edges: vec![],
        };
        let ctx = egui::Context::default();
        let mut canvas = Canvas {
            fit: false,
            ..Default::default()
        };
        frame(&ctx, &mut canvas, &graph, vec![]);
        let bounds = Rect::from_min_size(Pos2::new(32.0, 32.0), size(&graph.nodes[0]));
        let actions = click(
            &ctx,
            &mut canvas,
            &graph,
            run_button(bounds, 0, 1.0).center(),
        );
        assert!(matches!(actions.as_slice(), [Action::RunNode(node)] if *node == id));
        let actions = click(
            &ctx,
            &mut canvas,
            &graph,
            run_button(bounds, 1, 1.0).center(),
        );
        assert!(matches!(actions.as_slice(), [Action::RunDownstream(node)] if *node == id));
        canvas.run_enabled = false;
        assert!(click(
            &ctx,
            &mut canvas,
            &graph,
            run_button(bounds, 0, 1.0).center()
        )
        .is_empty());
        assert!(canvas.drag.is_none());
        assert_eq!(graph.nodes[0].position, [0.0, 0.0]);
    }

    #[test]
    fn real_canvas_shapes_keep_neutral_fill_with_status_outlines_and_running_indicator() {
        let id = Uuid::from_u128(7);
        let graph = Graph {
            nodes: vec![Node {
                id,
                position: [0.0, 0.0],
                config: datacraft_core::NodeKind::TablePreview { rows: 5 },
            }],
            edges: vec![],
        };
        let ctx = egui::Context::default();
        let mut canvas = Canvas {
            fit: false,
            ..Default::default()
        };
        for (status, outline, label) in [
            (
                NodeStatus::Completed,
                Color32::from_rgb(74, 210, 126),
                "Done (cached)",
            ),
            (NodeStatus::Failed, Color32::from_rgb(245, 91, 99), "Failed"),
            (NodeStatus::Running, Color32::from_gray(76), "Running"),
        ] {
            canvas.execution.insert(id, status);
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(640.0, 480.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        canvas.show(ui, &graph, None);
                    });
                },
            );
            assert!(out
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == Color32::from_rgb(35,43,57))));
            assert!(out
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.stroke.color == outline)));
            assert!(
                !out.shapes
                    .iter()
                    .any(|s| matches!(&s.shape, egui::Shape::Rect(r)
                if r.fill == Color32::from_rgb(28,65,47) || r.fill == Color32::from_rgb(78,32,39)))
            );
            assert!(out
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == label)));
            if status == NodeStatus::Running {
                assert!(out
                    .shapes
                    .iter()
                    .any(|s| matches!(&s.shape, egui::Shape::Path(p) if p.points.len() == 24)));
            }
        }
    }

    fn click(ctx: &egui::Context, canvas: &mut Canvas, graph: &Graph, pos: Pos2) -> Vec<Action> {
        frame(
            ctx,
            canvas,
            graph,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            ctx,
            canvas,
            graph,
            vec![egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
    }

    #[test]
    fn clicking_ports_emits_stable_edge_and_escape_discards_the_temporary_wire() {
        let source = Uuid::from_u128(20);
        let target = Uuid::from_u128(21);
        let graph = Graph {
            nodes: vec![
                Node {
                    id: source,
                    position: [0.0, 0.0],
                    config: datacraft_core::NodeKind::CsvSource {
                        path: "data.csv".into(),
                        header: true,
                        infer_schema: true,
                    },
                },
                Node {
                    id: target,
                    position: [300.0, 0.0],
                    config: datacraft_core::NodeKind::TablePreview { rows: 5 },
                },
            ],
            edges: Vec::new(),
        };
        let ctx = egui::Context::default();
        let mut canvas = Canvas {
            fit: false,
            ..Default::default()
        };
        frame(&ctx, &mut canvas, &graph, Vec::new());
        let output = Pos2::new(232.0, 80.0);
        let input = Pos2::new(332.0, 80.0);
        let started = click(&ctx, &mut canvas, &graph, output);
        assert!(canvas.connection.is_some());
        assert!(!started
            .iter()
            .any(|a| matches!(a, Action::Connect(_) | Action::Move(..))));
        let completed = click(&ctx, &mut canvas, &graph, input);
        let edges: Vec<_> = completed
            .into_iter()
            .filter_map(|a| match a {
                Action::Connect(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(
            edges,
            vec![Edge {
                from: source,
                from_port: "table".into(),
                to: target,
                to_port: "table".into()
            }]
        );
        assert!(canvas.connection.is_none());
        click(&ctx, &mut canvas, &graph, output);
        let escaped = frame(
            &ctx,
            &mut canvas,
            &graph,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(canvas.connection.is_none());
        assert!(!escaped.iter().any(|a| matches!(a, Action::Connect(_))));
        assert!(!click(&ctx, &mut canvas, &graph, input)
            .iter()
            .any(|a| matches!(a, Action::Connect(_))));
        assert!(graph.edges.is_empty());
    }

    fn frame(
        ctx: &egui::Context,
        canvas: &mut Canvas,
        graph: &Graph,
        events: Vec<egui::Event>,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(640.0, 480.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        actions = canvas.show(ui, graph, None);
                    });
            },
        );
        actions
    }

    #[test]
    fn pointer_drag_emits_one_persistent_move_on_release_only() {
        let graph = Graph {
            nodes: vec![Node {
                id: Uuid::from_u128(7),
                position: [0.0, 0.0],
                config: datacraft_core::NodeKind::TablePreview { rows: 5 },
            }],
            edges: Vec::new(),
        };
        let ctx = egui::Context::default();
        let mut canvas = Canvas {
            fit: false,
            ..Default::default()
        };
        frame(&ctx, &mut canvas, &graph, Vec::new());
        let start = Pos2::new(80.0, 50.0);
        let end = start + egui::vec2(64.0, 32.0);
        let press = egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        };
        let release = egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        };
        let pressed = frame(
            &ctx,
            &mut canvas,
            &graph,
            vec![egui::Event::PointerMoved(start), press],
        );
        assert!(!pressed.iter().any(|a| matches!(a, Action::Move(..))));
        let dragging = frame(
            &ctx,
            &mut canvas,
            &graph,
            vec![egui::Event::PointerMoved(end)],
        );
        assert!(dragging
            .iter()
            .any(|a| matches!(a,Action::Select(id) if *id==Uuid::from_u128(7))));
        assert!(!dragging.iter().any(|a| matches!(a, Action::Move(..))));
        let released = frame(&ctx, &mut canvas, &graph, vec![release]);
        let moves: Vec<_> = released
            .into_iter()
            .filter_map(|a| match a {
                Action::Move(id, p) => Some((id, p)),
                _ => None,
            })
            .collect();
        assert_eq!(moves, vec![(Uuid::from_u128(7), [64.0, 32.0])]);
        assert!(canvas.drag.is_none());
        assert_eq!(graph.nodes[0].position, [0.0, 0.0]);
        assert!(!frame(&ctx, &mut canvas, &graph, Vec::new())
            .iter()
            .any(|a| matches!(a, Action::Move(..))));
    }

    #[test]
    fn zoom_preserves_world_point_under_cursor_and_is_bounded() {
        let mut canvas = Canvas::default();
        let anchor = egui::vec2(256.0, 128.0);
        let before = (anchor - canvas.pan) / canvas.zoom;
        canvas.zoom_at(anchor, 2.0);
        assert_eq!((anchor - canvas.pan) / canvas.zoom, before);
        canvas.zoom_at(anchor, 100.0);
        assert_eq!(canvas.zoom, 2.0);
        canvas.zoom_at(anchor, 0.001);
        assert_eq!(canvas.zoom, 0.2);
    }
}
