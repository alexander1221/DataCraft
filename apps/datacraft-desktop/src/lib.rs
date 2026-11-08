//! Native Session adapter; contracts and manual acceptance: docs/DESKTOP.md.
mod canvas;
mod editor;

use canvas::{Action, Canvas};
use datacraft_core::{builtin_nodes, find_node, Node};
use datacraft_engine::{jobs::ExecutionEvent, Command, CommandPayload, Session, WorkerConfig};
use editor::{EditorAction, ParameterEditor};
use eframe::egui::{self, Color32};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

pub struct Desktop {
    session: Session,
    canvas: Canvas,
    selected: Option<Uuid>,
    editor: Option<ParameterEditor>,
    pending_select: Option<Uuid>,
    pending_new: bool,
    path: String,
    status: String,
    error: Option<String>,
    pending_open: Option<PathBuf>,
    close_dialog: bool,
    closing: bool,
    allow_close: bool,
    smoke_frames: Option<u32>,
    frames: u32,
}

impl Desktop {
    pub fn new(project: Option<PathBuf>, worker: WorkerConfig, smoke_frames: Option<u32>) -> Self {
        let mut session = Session::default();
        session.configure_worker(worker);
        let mut app = Self {
            session,
            canvas: Canvas::default(),
            selected: None,
            editor: None,
            pending_select: None,
            pending_new: false,
            path: String::new(),
            status: "Open a local .dcraft project".into(),
            error: None,
            pending_open: None,
            close_dialog: false,
            closing: false,
            allow_close: false,
            smoke_frames,
            frames: 0,
        };
        if let Some(path) = project {
            app.open(path, false);
        }
        app
    }

    fn dispatch(&mut self, command: Command) -> bool {
        let label = command.spec().label;
        let cancel_gestures = matches!(
            &command,
            Command::Undo | Command::Redo | Command::RemoveNode { .. }
        );
        match self.session.dispatch(command) {
            Ok(result) => {
                self.status = label.into();
                self.error = None;
                if let CommandPayload::Saved { path } = result.payload {
                    self.path = path.to_string_lossy().into_owned();
                }
                self.sync_editor();
                if cancel_gestures {
                    self.canvas.cancel_gestures();
                }
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn open(&mut self, path: PathBuf, discard_changes: bool) {
        if self.has_unapplied() && !discard_changes {
            self.error = Some("Apply or discard inspector changes before opening a project".into());
            return;
        }
        if self.dispatch(Command::Open {
            path,
            discard_changes,
        }) {
            self.path = self
                .session
                .path()
                .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
            self.selected = None;
            self.editor = None;
            self.pending_select = None;
            self.canvas = Canvas::default();
        }
    }

    fn active_job(&self) -> bool {
        self.session
            .current_job()
            .is_some_and(|j| !j.status.is_terminal())
    }

    fn has_unapplied(&self) -> bool {
        self.editor.as_ref().is_some_and(ParameterEditor::is_dirty)
    }
    fn has_unsaved(&self) -> bool {
        self.session.is_dirty() || self.has_unapplied()
    }

    fn new_project(&mut self, discard_changes: bool) {
        if self.has_unapplied() && !discard_changes {
            self.error =
                Some("Apply or discard inspector changes before creating a project".into());
            return;
        }
        if self.dispatch(Command::New {
            name: "Untitled".into(),
            discard_changes,
        }) {
            self.path.clear();
            self.selected = None;
            self.editor = None;
            self.pending_select = None;
            self.canvas = Canvas::default();
        }
    }

    fn set_selection(&mut self, id: Uuid) {
        let editor = self
            .session
            .project()
            .and_then(|p| p.graph.nodes.iter().find(|n| n.id == id))
            .map(ParameterEditor::new);
        match editor {
            Some(Ok(editor)) => {
                self.selected = Some(id);
                self.editor = Some(editor);
            }
            Some(Err(error)) => self.error = Some(error),
            None => {
                self.selected = None;
                self.editor = None;
            }
        }
    }

    fn select(&mut self, id: Uuid) {
        if self.selected == Some(id) {
            return;
        }
        if self.has_unapplied() {
            self.pending_select = Some(id);
        } else {
            self.set_selection(id);
        }
    }

    fn sync_editor(&mut self) {
        let node = self.selected.and_then(|id| {
            self.session
                .project()
                .and_then(|p| p.graph.nodes.iter().find(|n| n.id == id))
        });
        match node {
            Some(node)
                if self.editor.as_ref().is_none_or(|e| {
                    !e.is_dirty() && (e.node_id != node.id || e.original != node.config)
                }) =>
            {
                let id = node.id;
                self.set_selection(id);
            }
            None => {
                self.selected = None;
                self.editor = None;
            }
            _ => {}
        }
    }

    fn apply_parameters(&mut self) -> bool {
        let Some(editor) = self.editor.as_ref().filter(|e| e.is_dirty()) else {
            return true;
        };
        let node_id = editor.node_id;
        let config = match editor.config() {
            Ok(config) => config,
            Err(error) => {
                self.error = Some(error);
                return false;
            }
        };
        if self.dispatch(Command::ConfigureNode { node_id, config }) {
            self.set_selection(node_id);
            true
        } else {
            false
        }
    }

    fn add_node(&mut self, kind: &str) {
        if self.has_unapplied() {
            return;
        }
        let Some(spec) = find_node(kind) else {
            self.error = Some("Unknown node type".into());
            return;
        };
        let config = match spec.create_draft_config() {
            Ok(config) => config,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let count = self.session.project().map_or(0, |p| p.graph.nodes.len());
        let id = Uuid::new_v4();
        let node = Node {
            id,
            position: self.canvas.new_position(count),
            config,
        };
        if self.dispatch(Command::AddNode { node }) {
            self.set_selection(id);
        }
    }

    fn palette(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("palette")
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.heading("Nodes");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let enabled = self.session.project().is_some() && !self.has_unapplied();
                    for spec in builtin_nodes() {
                        if ui
                            .add_enabled(enabled, egui::Button::new(spec.label))
                            .clicked()
                        {
                            self.add_node(spec.id);
                        }
                    }
                    ui.separator();
                    ui.label("Connect: click an output, then an input.");
                    ui.label("Escape / right click cancels a gesture.");
                    if self.has_unapplied() {
                        ui.colored_label(Color32::YELLOW, "Apply or Revert inspector edits first");
                    }
                });
            });
    }

    fn save(&mut self, save_as: bool) -> bool {
        if !self.apply_parameters() {
            return false;
        }
        if save_as || self.session.path().is_none() {
            if self.path.trim().is_empty() {
                self.error = Some("Enter a destination .dcraft path".into());
                return false;
            }
            self.dispatch(Command::SaveAs {
                path: PathBuf::from(&self.path),
            })
        } else {
            self.dispatch(Command::Save)
        }
    }

    fn finish_close(&mut self, ctx: &egui::Context) {
        self.close_dialog = false;
        if let Some(job) = self
            .session
            .current_job()
            .filter(|j| !j.status.is_terminal())
        {
            self.dispatch(Command::Cancel { job_id: job.id });
            self.closing = true;
        } else {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .min_height(90.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("DataCraft");
                    if ui
                        .add_enabled(!self.active_job(), egui::Button::new("New project"))
                        .clicked()
                    {
                        if self.has_unsaved() {
                            self.pending_new = true;
                        } else {
                            self.new_project(false);
                        }
                    }
                    if let Some(project) = self.session.project() {
                        ui.add_sized(
                            [170.0, 20.0],
                            egui::Label::new(format!(
                                "{}{}",
                                project.name,
                                if self.session.is_dirty() { " *" } else { "" }
                            ))
                            .truncate(),
                        );
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.session.can_undo() && !self.has_unapplied(),
                            egui::Button::new("Undo"),
                        )
                        .clicked()
                    {
                        self.dispatch(Command::Undo);
                    }
                    if ui
                        .add_enabled(
                            self.session.can_redo() && !self.has_unapplied(),
                            egui::Button::new("Redo"),
                        )
                        .clicked()
                    {
                        self.dispatch(Command::Redo);
                    }
                    if ui.button("Fit graph").clicked() {
                        self.canvas.fit = true;
                    }
                    let run = Command::Run { output: None };
                    if ui
                        .add_enabled(
                            self.session.availability(&run).enabled && !self.has_unapplied(),
                            egui::Button::new("Run project"),
                        )
                        .clicked()
                    {
                        self.dispatch(run);
                    }
                    let output = self.selected.filter(|id| {
                        self.session.project().is_some_and(|p| {
                            p.graph
                                .nodes
                                .iter()
                                .any(|n| n.id == *id && n.config.is_output())
                        })
                    });
                    let selected_run = Command::Run { output };
                    if ui
                        .add_enabled(
                            output.is_some()
                                && self.session.availability(&selected_run).enabled
                                && !self.has_unapplied(),
                            egui::Button::new("Run selected"),
                        )
                        .clicked()
                    {
                        self.dispatch(selected_run);
                    }
                    if ui
                        .add_enabled(self.active_job(), egui::Button::new("Cancel"))
                        .clicked()
                    {
                        if let Some(job) = self.session.current_job() {
                            self.dispatch(Command::Cancel { job_id: job.id });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Project path");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path)
                            .desired_width((ui.available_width() - 220.0).max(180.0)),
                    );
                    if ui
                        .add_enabled(!self.active_job(), egui::Button::new("Open"))
                        .clicked()
                    {
                        let path = PathBuf::from(&self.path);
                        if self.has_unsaved() {
                            self.pending_open = Some(path);
                        } else {
                            self.open(path, false);
                        }
                    }
                    let loaded = self.session.project().is_some();
                    if ui.add_enabled(loaded, egui::Button::new("Save")).clicked() {
                        self.save(false);
                    }
                    if ui
                        .add_enabled(loaded, egui::Button::new("Save as"))
                        .clicked()
                    {
                        self.save(true);
                    }
                });
            });
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        self.sync_editor();
        let node = self
            .selected
            .and_then(|id| {
                self.session
                    .project()
                    .and_then(|p| p.graph.nodes.iter().find(|n| n.id == id))
            })
            .cloned();
        let edges: Vec<_> = self
            .session
            .project()
            .map(|p| {
                p.graph
                    .edges
                    .iter()
                    .filter(|e| Some(e.from) == self.selected || Some(e.to) == self.selected)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut command = None;
        let mut editor_action = None;
        egui::SidePanel::right("inspector")
            .default_width(290.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(node) = &node {
                        let spec = node.config.spec();
                        ui.strong(spec.label);
                        ui.small(node.id.to_string());
                        ui.label(format!(
                            "Position: {}, {}",
                            node.position[0], node.position[1]
                        ));
                        ui.separator();
                        ui.strong("Parameters");
                        if let Some(editor) = &mut self.editor {
                            editor_action = editor.show(ui);
                        }
                        ui.separator();
                        let pending = self.has_unapplied();
                        if ui
                            .add_enabled(!pending, egui::Button::new("Delete node"))
                            .clicked()
                        {
                            command = Some(Command::RemoveNode { node_id: node.id });
                        }
                        if spec.selectable_output {
                            let designated = self
                                .session
                                .project()
                                .is_some_and(|p| p.output_node == Some(node.id));
                            if designated {
                                ui.label("Project output");
                            }
                            if ui
                                .add_enabled(
                                    !pending,
                                    egui::Button::new(if designated {
                                        "Clear project output"
                                    } else {
                                        "Set project output"
                                    }),
                                )
                                .clicked()
                            {
                                command = Some(Command::SetOutput {
                                    output: if designated { None } else { Some(node.id) },
                                });
                            }
                        }
                        ui.separator();
                        for (title, ports) in [("Inputs", spec.inputs), ("Outputs", spec.outputs)] {
                            ui.strong(title);
                            for port in ports {
                                ui.label(format!("{}: {:?}", port.id, port.data_type));
                            }
                        }
                        if !edges.is_empty() {
                            ui.separator();
                            ui.strong("Connections");
                        }
                        for edge in &edges {
                            ui.push_id(
                                (
                                    edge.from,
                                    edge.from_port.as_str(),
                                    edge.to,
                                    edge.to_port.as_str(),
                                ),
                                |ui| {
                                    let label = |id| {
                                        self.session
                                            .project()
                                            .and_then(|p| p.graph.nodes.iter().find(|n| n.id == id))
                                            .map_or("?", |n| n.config.spec().label)
                                    };
                                    ui.label(format!(
                                        "{}:{} → {}:{}",
                                        label(edge.from),
                                        edge.from_port,
                                        label(edge.to),
                                        edge.to_port
                                    ));
                                    if ui
                                        .add_enabled(!pending, egui::Button::new("Disconnect"))
                                        .clicked()
                                    {
                                        command = Some(Command::Disconnect { edge: edge.clone() });
                                    }
                                },
                            );
                        }
                    } else {
                        ui.label("Select a node on the canvas");
                    }
                    if let Some(project) = self.session.project() {
                        ui.separator();
                        ui.strong("Project readiness");
                        match project.graph.diagnostics() {
                            Ok(issues) if issues.is_empty() => {
                                ui.label(if project.graph.nodes.is_empty() {
                                    "Add a source and a preview to begin"
                                } else {
                                    "Ready"
                                });
                            }
                            Ok(issues) => {
                                for issue in issues {
                                    ui.colored_label(Color32::YELLOW, issue.to_string());
                                }
                            }
                            Err(error) => {
                                ui.colored_label(Color32::LIGHT_RED, error.to_string());
                            }
                        }
                    }
                });
            });
        if let Some(action) = editor_action {
            match action {
                EditorAction::Apply => {
                    self.apply_parameters();
                }
                EditorAction::Revert => {
                    if let Some(editor) = &mut self.editor {
                        editor.revert();
                    }
                    self.error = None;
                    self.status = "Parameter edits reverted".into();
                }
            }
        }
        if let Some(command) = command {
            self.dispatch(command);
        }
    }

    fn results(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("results")
            .resizable(true)
            .default_height(210.0)
            .min_height(100.0)
            .show(ctx, |ui| {
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    ui.heading("Results");
                    ui.label(&self.status);
                    if let Some(job) = self.session.current_job() {
                        ui.label(format!("{:?}", job.status));
                        if let Some(error) = job.error {
                            ui.colored_label(Color32::LIGHT_RED, error.message);
                        }
                        if let Some(error) = job.termination_error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                    }
                });
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                if let Some(record) = self.session.last_execution() {
                    if self.session.current_job().is_some_and(|job|job.status != datacraft_engine::jobs::JobStatus::Completed) {
                        ui.colored_label(Color32::YELLOW,"Showing the last successful result; the current job has no completed result");
                    }
                    if !self.session.result_is_current() {
                        ui.colored_label(
                            Color32::YELLOW,
                            "Result belongs to an earlier project revision",
                        );
                    }
                    egui::ScrollArea::both().show(ui, |ui| {
                        for (index, table) in record.result.tables.iter().enumerate() {
                            ui.strong(format!("{} — {} rows", table.name, table.rows.len()));
                            ui.collapsing(format!("Schema: {}", table.name), |ui| {
                                ui.monospace(table.schema.to_string());
                            });
                            let columns: Vec<_> = table
                                .schema
                                .get("fields")
                                .and_then(|f| f.as_array())
                                .into_iter()
                                .flatten()
                                .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
                                .collect();
                            egui::Grid::new(("preview", index))
                                .striped(true)
                                .show(ui, |ui| {
                                    for column in &columns {
                                        ui.strong(*column);
                                    }
                                    ui.end_row();
                                    for row in &table.rows {
                                        for column in &columns {
                                            ui.label(row.get(*column).map_or_else(
                                                || "null".into(),
                                                |v| {
                                                    v.as_str().map_or_else(
                                                        || v.to_string(),
                                                        str::to_owned,
                                                    )
                                                },
                                            ));
                                        }
                                        ui.end_row();
                                    }
                                });
                        }
                    });
                }
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.pending_new || self.pending_open.is_some() {
            egui::Modal::new(egui::Id::new("replace_project")).show(ctx, |ui| {
                ui.heading("Unsaved changes");
                ui.label("Discard project changes and unapplied parameter edits?");
                ui.horizontal(|ui| {
                    if ui.button("Discard and continue").clicked() {
                        let new = self.pending_new;
                        self.pending_new = false;
                        if let Some(path) = self.pending_open.take() {
                            self.open(path, true);
                        } else if new {
                            self.new_project(true);
                        }
                    }
                    if ui.button("Keep editing").clicked() {
                        self.pending_new = false;
                        self.pending_open = None;
                    }
                });
            });
        } else if let Some(id) = self.pending_select {
            egui::Modal::new(egui::Id::new("inspector_changes")).show(ctx, |ui| {
                ui.heading("Unapplied parameters");
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                ui.label("Apply or discard these edits before selecting another node.");
                ui.horizontal(|ui| {
                    if ui.button("Apply and select").clicked() && self.apply_parameters() {
                        self.pending_select = None;
                        self.set_selection(id);
                    }
                    if ui.button("Discard and select").clicked() {
                        self.pending_select = None;
                        self.set_selection(id);
                    }
                    if ui.button("Keep editing").clicked() {
                        self.pending_select = None;
                    }
                });
            });
        } else if self.close_dialog {
            egui::Modal::new(egui::Id::new("close_project")).show(ctx, |ui| {
                ui.heading("Close DataCraft?");
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                ui.label("The project or inspector contains unsaved changes.");
                ui.horizontal(|ui| {
                    if ui.button("Save and close").clicked() && self.save(false) {
                        self.finish_close(ctx);
                    }
                    if ui.button("Discard and close").clicked() {
                        self.finish_close(ctx);
                    }
                    if ui.button("Keep editing").clicked() {
                        self.close_dialog = false;
                    }
                });
            });
        }
    }

    fn render(&mut self, ctx: &egui::Context) {
        for event in self.session.poll_execution() {
            match event {
                ExecutionEvent::Stage { stage, .. } => self.status = format!("{stage:?}"),
                ExecutionEvent::Failed { error, .. } => self.error = Some(error.message),
                _ => {}
            }
        }
        if self.closing {
            if !self.active_job() {
                self.finish_close(ctx);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Stopping local execution…");
            });
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        self.toolbar(ctx);
        self.results(ctx);
        self.inspector(ctx);
        self.palette(ctx);
        let actions = egui::CentralPanel::default()
            .show(ctx, |ui| {
                if let Some(project) = self.session.project() {
                    self.canvas.show(ui, &project.graph, self.selected)
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Create a New project, or enter a .dcraft path and press Open");
                    });
                    Vec::new()
                }
            })
            .inner;
        for action in actions {
            match action {
                Action::Select(id) => self.select(id),
                Action::Move(node_id, position) => {
                    self.dispatch(Command::MoveNode { node_id, position });
                }
                Action::Connect(edge) => {
                    self.dispatch(Command::Connect { edge });
                }
            }
        }
        if self.selected.is_some_and(|id| {
            !self
                .session
                .project()
                .is_some_and(|p| p.graph.nodes.iter().any(|n| n.id == id))
        }) {
            self.selected = None;
            self.editor = None;
        }
        self.dialogs(ctx);
        if self.active_job() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.has_unsaved() {
                self.close_dialog = true;
            } else {
                self.finish_close(ctx);
            }
        }
        self.render(ctx);
        self.frames = self.frames.saturating_add(1);
        if self.smoke_frames.is_some_and(|limit| self.frames >= limit)
            && !self.has_unsaved()
            && !self.active_job()
            && !self.allow_close
        {
            println!(
                "Native startup: {} frames, project loaded: {}",
                self.frames,
                self.session.project().is_some()
            );
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.smoke_frames.is_some() {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible UI text {label}"))
    }

    fn click(app: &mut Desktop, ctx: &egui::Context, pos: egui::Pos2) {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
    }

    fn click_label(app: &mut Desktop, ctx: &egui::Context, label: &str) {
        // egui Areas use a sizing pass before a newly opened Modal becomes visible.
        frame(app, ctx, Vec::new());
        let output = frame(app, ctx, Vec::new());
        let pos = text_position(&output, label);
        click(app, ctx, pos);
    }

    fn edit_string(app: &mut Desktop, ctx: &egui::Context, label: &str, value: &str) {
        frame(app, ctx, Vec::new());
        let output = frame(app, ctx, Vec::new());
        let label = text_position(&output, label);
        click(app, ctx, label + egui::vec2(0.0, 26.0));
        frame(
            app,
            ctx,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Text(value.into()),
            ],
        );
    }

    #[test]
    fn inspector_input_is_guarded_until_apply_and_undo_restores_config() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.select(Uuid::from_u128(2));
        let original = app.session.project().cloned().unwrap();
        edit_string(&mut app, &ctx, "SQL expression", "quantity >= 5");
        assert!(app.has_unapplied());
        assert!(!app.session.is_dirty());
        assert_eq!(app.session.project(), Some(&original));
        app.select(Uuid::from_u128(3));
        assert_eq!(app.selected, Some(Uuid::from_u128(2)));
        assert_eq!(app.pending_select, Some(Uuid::from_u128(3)));
        app.open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/pipelines/sample.dcraft"),
            false,
        );
        app.new_project(false);
        assert_eq!(app.session.project(), Some(&original));
        assert!(app.has_unapplied());
        click_label(&mut app, &ctx, "Keep editing");
        click_label(&mut app, &ctx, "Apply");
        assert!(!app.has_unapplied());
        assert!(app.session.is_dirty());
        assert!(
            matches!(&app.session.project().unwrap().graph.nodes[1].config,datacraft_core::NodeKind::Filter {expression} if expression=="quantity >= 5")
        );
        click_label(&mut app, &ctx, "Undo");
        assert_eq!(app.session.project(), Some(&original));
        assert!(!app.has_unapplied());
        assert!(!app.has_unsaved());
    }

    #[test]
    fn palette_add_delete_and_draft_save_round_trip_use_session_history() {
        let mut app = app();
        app.new_project(true);
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Vec::new());
        click_label(&mut app, &ctx, "Filter");
        assert_eq!(app.session.project().unwrap().graph.nodes.len(), 1);
        let id = app.selected.unwrap();
        assert_eq!(app.editor.as_ref().unwrap().node_id, id);
        assert_eq!(
            app.session
                .project()
                .unwrap()
                .graph
                .diagnostics()
                .unwrap()
                .len(),
            2
        );
        let before = app.session.project().cloned().unwrap();
        click_label(&mut app, &ctx, "Delete node");
        assert!(app.session.project().unwrap().graph.nodes.is_empty());
        click_label(&mut app, &ctx, "Undo");
        assert_eq!(app.session.project(), Some(&before));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("draft.dcraft");
        app.path = path.to_string_lossy().into_owned();
        assert!(app.save(true));
        assert!(!app.has_unsaved());
        app.new_project(false);
        app.open(path, true);
        assert_eq!(app.session.project(), Some(&before));
        assert_eq!(
            app.session
                .project()
                .unwrap()
                .graph
                .diagnostics()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn rejected_cycle_and_incompatible_wire_preserve_graph_and_undo() {
        let mut app = app();
        app.new_project(true);
        app.add_node("filter");
        let a = app.selected.unwrap();
        app.add_node("filter");
        let b = app.selected.unwrap();
        let wire = |from, to| datacraft_core::Edge {
            from,
            to,
            from_port: "table".into(),
            to_port: "table".into(),
        };
        assert!(app.dispatch(Command::Connect { edge: wire(a, b) }));
        let before = app.session.project().cloned();
        assert!(!app.dispatch(Command::Connect { edge: wire(b, a) }));
        assert_eq!(app.session.project(), before.as_ref());
        assert!(app.dispatch(Command::Undo));
        assert!(app.session.project().unwrap().graph.edges.is_empty());
        app.add_node("linear_regression");
        let model = app.selected.unwrap();
        app.add_node("table_preview");
        let preview = app.selected.unwrap();
        let before = app.session.project().cloned();
        assert!(!app.dispatch(Command::Connect {
            edge: datacraft_core::Edge {
                from: model,
                from_port: "model".into(),
                to: preview,
                to_port: "table".into()
            }
        }));
        assert_eq!(app.session.project(), before.as_ref());
    }

    #[test]
    fn invalid_numeric_buffer_cannot_save_or_silently_disappear() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.select(Uuid::from_u128(4));
        let before = app.session.project().cloned();
        edit_string(&mut app, &ctx, "Rows", "not a number");
        assert!(app.has_unapplied());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("must-not-exist.dcraft");
        app.path = path.to_string_lossy().into_owned();
        assert!(!app.save(true));
        assert!(!path.exists());
        assert_eq!(app.session.project(), before.as_ref());
        assert!(app.has_unsaved());
        assert!(!app.active_job());
        click_label(&mut app, &ctx, "Revert");
        assert!(!app.has_unapplied());
        assert_eq!(app.session.project(), before.as_ref());
    }

    #[test]
    fn delete_and_disconnect_buttons_are_undoable_with_incident_edges() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.select(Uuid::from_u128(1));
        let before = app.session.project().cloned().unwrap();
        click_label(&mut app, &ctx, "Disconnect");
        assert_eq!(
            app.session.project().unwrap().graph.edges.len(),
            before.graph.edges.len() - 1
        );
        click_label(&mut app, &ctx, "Undo");
        assert_eq!(app.session.project(), Some(&before));
        click_label(&mut app, &ctx, "Delete node");
        assert_eq!(
            app.session.project().unwrap().graph.nodes.len(),
            before.graph.nodes.len() - 1
        );
        assert!(app
            .session
            .project()
            .unwrap()
            .graph
            .edges
            .iter()
            .all(|e| e.from != Uuid::from_u128(1) && e.to != Uuid::from_u128(1)));
        click_label(&mut app, &ctx, "Undo");
        assert_eq!(app.session.project(), Some(&before));
    }

    fn frame(app: &mut Desktop, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    }

    #[cfg(feature = "spark-integration-tests")]
    #[test]
    fn run_button_executes_real_spark_and_preview_is_rendered() {
        let mut app = app();
        app.session.configure_worker(WorkerConfig {
            java: std::env::var_os("DATACRAFT_JAVA").map_or_else(|| "java".into(), PathBuf::from),
            classpath_file: std::env::var_os("DATACRAFT_WORKER_CLASSPATH").map_or_else(
                || {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../workers/spark/target/classpath.txt")
                },
                PathBuf::from,
            ),
        });
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Vec::new());
        let output = frame(&mut app, &ctx, Vec::new());
        let button = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Run project" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("run button must be rendered");
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(button),
                egui::Event::PointerButton {
                    pos: button,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: button,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(
            app.session.current_job().is_some(),
            "click must dispatch an execution job: {:?}",
            app.error
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while app.active_job() {
            assert!(
                std::time::Instant::now() < deadline,
                "GUI preview acceptance timed out"
            );
            frame(&mut app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(50));
        }
        let output = frame(&mut app, &ctx, Vec::new());
        assert_eq!(
            app.session.current_job().unwrap().status,
            datacraft_engine::jobs::JobStatus::Completed
        );
        let result = app.session.last_execution().expect("completed preview");
        assert_eq!(result.result.tables.len(), 1);
        assert_eq!(
            result.result.tables[0].rows,
            vec![
                serde_json::json!({"quantity":4,"amount":11}),
                serde_json::json!({"quantity":5,"amount":13}),
                serde_json::json!({"quantity":6,"amount":15}),
            ]
        );
        assert!(output.shapes.iter().any(|clipped|matches!(&clipped.shape,egui::Shape::Text(t) if t.galley.job.text.contains("3 rows"))));
        let first_job = app.session.current_job().unwrap().id;
        app.select(Uuid::from_u128(2));
        edit_string(&mut app, &ctx, "SQL expression", "quantity >= 5");
        assert!(app.has_unapplied());
        click_label(&mut app, &ctx, "Apply");
        click_label(&mut app, &ctx, "Run project");
        assert_ne!(app.session.current_job().unwrap().id, first_job);
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while app.active_job() {
            assert!(
                std::time::Instant::now() < deadline,
                "edited preview acceptance timed out"
            );
            frame(&mut app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(50));
        }
        frame(&mut app, &ctx, Vec::new());
        assert_eq!(
            app.session.current_job().unwrap().status,
            datacraft_engine::jobs::JobStatus::Completed
        );
        assert_eq!(
            app.session.last_execution().unwrap().result.tables[0].rows,
            vec![
                serde_json::json!({"quantity":5,"amount":13}),
                serde_json::json!({"quantity":6,"amount":15}),
                serde_json::json!({"quantity":7,"amount":17}),
            ]
        );
        app.new_project(true);
        frame(&mut app, &ctx, Vec::new());
        assert!(app.session.current_job().is_none());
        assert!(app.session.last_execution().is_none());
    }
    fn app() -> Desktop {
        Desktop::new(
            Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../examples/pipelines/sample.dcraft"),
            ),
            WorkerConfig {
                java: "unavailable-java".into(),
                classpath_file: "unavailable-classpath".into(),
            },
            None,
        )
    }
    #[test]
    fn canvas_and_inspector_render_without_java_or_project_mutation() {
        let mut app = app();
        assert!(app.error.is_none());
        let before = app.session.project().cloned();
        let ctx = egui::Context::default();
        app.selected = Some(Uuid::from_u128(2));
        for _ in 0..3 {
            let output = frame(&mut app, &ctx, Vec::new());
            assert!(!output.shapes.is_empty());
            let path_label = output
                .shapes
                .iter()
                .find_map(|clipped| {
                    if let egui::Shape::Text(text) = &clipped.shape {
                        if text.galley.job.text == "Project path" {
                            return Some((
                                clipped.clip_rect,
                                egui::Rect::from_min_size(text.pos, text.galley.size()),
                            ));
                        }
                    }
                    None
                })
                .expect("project path control must be painted");
            assert!(
                path_label.0.contains_rect(path_label.1),
                "project path row must be visible inside toolbar"
            );
        }
        assert_eq!(app.session.project(), before.as_ref());
        assert!(!app.session.is_dirty());
        assert!(app.session.current_job().is_none());
    }
    #[test]
    fn failed_open_preserves_loaded_project_selection_and_view() {
        let mut app = app();
        let before = app.session.project().cloned();
        app.selected = Some(Uuid::from_u128(2));
        app.canvas.fit = false;
        app.open(PathBuf::from("missing-desktop-fixture.dcraft"), false);
        assert!(app.error.is_some());
        assert_eq!(app.session.project(), before.as_ref());
        assert_eq!(app.selected, Some(Uuid::from_u128(2)));
        assert!(!app.canvas.fit);
    }
}
