//! Native Session adapter; contracts and manual acceptance: docs/DESKTOP.md.
mod canvas;
mod editor;
mod file_dialog;
mod hdfs_browser;
mod layout;
pub use layout::arranged_positions;

use canvas::{Action, Canvas};
use datacraft_core::{builtin_nodes, find_node, Node};
use datacraft_engine::{jobs::ExecutionEvent, Command, CommandPayload, Session, WorkerConfig};
use editor::{EditorAction, ParameterEditor};
use eframe::egui::{self, Color32};
use file_dialog::{
    FileDialogs, Purpose as FilePurpose, Request as FileRequest, Selection as FileSelection,
};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

pub struct Desktop {
    session: Session,
    canvas: Canvas,
    selected: Option<Uuid>,
    editor: Option<ParameterEditor>,
    pending_select: Option<Uuid>,
    pending_new: bool,
    file_dialogs: FileDialogs,
    rename: Option<RenameDraft>,
    path: String,
    status: String,
    error: Option<String>,
    pending_open: Option<PathBuf>,
    close_dialog: bool,
    closing: bool,
    allow_close: bool,
    smoke_frames: Option<u32>,
    frames: u32,
    hdfs_browser: Option<hdfs_browser::Browser>,
}

struct RenameDraft {
    name: String,
    revision: u64,
    focus: bool,
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
            file_dialogs: FileDialogs::default(),
            rename: None,
            path: String::new(),
            status: "Open a local .dcraft project".into(),
            error: None,
            pending_open: None,
            close_dialog: false,
            closing: false,
            allow_close: false,
            smoke_frames,
            frames: 0,
            hdfs_browser: None,
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
    fn choosing_file(&self) -> bool {
        self.file_dialogs.is_busy() || self.hdfs_browser.is_some()
    }

    fn request_file(&mut self, purpose: FilePurpose, initial_path: Option<PathBuf>) {
        if self.rename.is_some() {
            self.error = Some("Finish renaming before opening a file dialog".into());
            return;
        }
        let request = FileRequest {
            purpose,
            revision: self.session.revision(),
            initial_path,
        };
        match self.file_dialogs.queue(request) {
            Ok(()) => self.canvas.cancel_gestures(),
            Err(error) => self.error = Some(error),
        }
    }

    fn save_dialog(&mut self, close_after: bool) {
        let path = self
            .session
            .path()
            .map(std::path::Path::to_owned)
            .or_else(|| (!self.path.trim().is_empty()).then(|| PathBuf::from(&self.path)));
        self.request_file(FilePurpose::SaveProject { close_after }, path);
    }

    fn request_open(&mut self, path: PathBuf) {
        if self.has_unsaved() {
            self.pending_open = Some(path);
        } else {
            self.open(path, false);
        }
    }

    fn handle_file_selection(&mut self, selection: FileSelection, ctx: &egui::Context) {
        let Some(path) = selection.path else {
            self.status =
                "File dialog closed without a selection; manual paths are also available".into();
            return;
        };
        if selection.request.revision != self.session.revision() {
            self.error = Some(
                "Project changed while the file dialog was open; choose the file again".into(),
            );
            return;
        }
        match selection.request.purpose {
            FilePurpose::OpenProject => self.request_open(path),
            FilePurpose::SaveProject { close_after } => {
                if self.apply_parameters()
                    && self.dispatch(Command::SaveAs {
                        path: file_dialog::project_extension(path),
                    })
                    && close_after
                {
                    self.finish_close(ctx);
                }
            }
            FilePurpose::Source {
                node_id, parameter, ..
            }
            | FilePurpose::Destination {
                node_id, parameter, ..
            } => {
                if self.selected != Some(node_id) {
                    self.error = Some(
                        "Source selection belongs to another node; choose the file again".into(),
                    );
                    return;
                }
                let result = self
                    .editor
                    .as_mut()
                    .filter(|e| e.node_id == node_id)
                    .ok_or_else(|| "The source inspector is no longer open".to_string())
                    .and_then(|e| e.set_local_path(parameter, &path));
                match result {
                    Ok(()) => {
                        self.error = None;
                        self.status = "Source selected — Apply to update the node".into();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }

    fn begin_rename(&mut self) {
        if self.choosing_file() || self.has_unapplied() {
            return;
        }
        if let Some(project) = self.session.project() {
            self.rename = Some(RenameDraft {
                name: project.name.clone(),
                revision: self.session.revision(),
                focus: true,
            });
            self.error = None;
        }
    }

    fn apply_rename(&mut self) -> bool {
        let Some(draft) = &self.rename else {
            return false;
        };
        if draft.revision != self.session.revision() {
            self.error = Some("Project changed; cancel and reopen Rename".into());
            return false;
        }
        let name = draft.name.clone();
        if self.dispatch(Command::Rename { name }) {
            self.rename = None;
            true
        } else {
            false
        }
    }

    fn has_unapplied(&self) -> bool {
        self.editor.as_ref().is_some_and(ParameterEditor::is_dirty)
    }
    fn has_unsaved(&self) -> bool {
        self.session.is_dirty() || self.has_unapplied() || self.rename.is_some()
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

    fn arrange(&mut self) {
        let Some(project) = self.session.project() else {
            return;
        };
        match arranged_positions(&project.graph) {
            Ok(positions) => {
                let commands = positions
                    .into_iter()
                    .map(|(node_id, position)| Command::MoveNode { node_id, position })
                    .collect();
                self.canvas.cancel_gestures();
                if self.dispatch(Command::Batch { commands }) {
                    self.canvas.fit = true;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn palette(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("palette")
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.heading("Nodes");
                if ui
                    .add_enabled(
                        self.session
                            .project()
                            .is_some_and(|p| !p.graph.nodes.is_empty())
                            && !self.choosing_file()
                            && !self.has_unapplied(),
                        egui::Button::new("Arrange"),
                    )
                    .clicked()
                {
                    self.arrange();
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let enabled = self.session.project().is_some()
                        && !self.has_unapplied()
                        && !self.choosing_file();
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

    fn request_close(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if self.choosing_file() {
            self.status = "Close the file dialog before exiting DataCraft".into();
        } else if self.rename.is_some() {
            self.error = Some("Finish or cancel renaming before closing the project".into());
        } else if self.has_unsaved() {
            self.close_dialog = true;
        } else {
            self.finish_close(ctx);
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        let choosing = self.choosing_file();
        self.canvas.execution = self.session.node_statuses();
        self.canvas.run_enabled = !self.active_job() && !self.has_unapplied() && !choosing;
        egui::TopBottomPanel::top("toolbar")
            .min_height(90.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("DataCraft");
                    if ui
                        .add_enabled(
                            !self.active_job() && !choosing,
                            egui::Button::new("New project"),
                        )
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
                    if ui
                        .add_enabled(
                            self.session.project().is_some() && !choosing && !self.has_unapplied(),
                            egui::Button::new("Rename project…"),
                        )
                        .clicked()
                    {
                        self.begin_rename();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.session.can_undo() && !self.has_unapplied() && !choosing,
                            egui::Button::new("Undo"),
                        )
                        .clicked()
                    {
                        self.dispatch(Command::Undo);
                    }
                    if ui
                        .add_enabled(
                            self.session.can_redo() && !self.has_unapplied() && !choosing,
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
                            self.session.availability(&run).enabled
                                && !self.has_unapplied()
                                && !choosing,
                            egui::Button::new("Run project"),
                        )
                        .on_hover_text("Lazy preview. To save inputs for node-by-node runs, use the green Run branch button on a Source.")
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
                                && !self.has_unapplied()
                                && !choosing,
                            egui::Button::new("Run selected"),
                        )
                        .on_hover_text("Lazy preview of the selected output; does not populate or refresh the node cache.")
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
                    ui.add_enabled(
                        !choosing,
                        egui::TextEdit::singleline(&mut self.path)
                            .desired_width((ui.available_width() - 430.0).max(120.0)),
                    );
                    if ui
                        .add_enabled(!self.active_job() && !choosing, egui::Button::new("Open…"))
                        .clicked()
                    {
                        let initial =
                            (!self.path.trim().is_empty()).then(|| PathBuf::from(&self.path));
                        self.request_file(FilePurpose::OpenProject, initial);
                    }
                    if ui
                        .add_enabled(
                            !self.active_job() && !choosing,
                            egui::Button::new("Open path"),
                        )
                        .clicked()
                    {
                        let path = PathBuf::from(&self.path);
                        self.request_open(path);
                    }
                    let loaded = self.session.project().is_some() && !choosing;
                    if ui.add_enabled(loaded, egui::Button::new("Save")).clicked() {
                        if self.session.path().is_some() {
                            self.save(false);
                        } else {
                            self.save_dialog(false);
                        }
                    }
                    if ui
                        .add_enabled(loaded, egui::Button::new("Save as…"))
                        .clicked()
                    {
                        self.save_dialog(false);
                    }
                    if ui
                        .add_enabled(loaded, egui::Button::new("Save path"))
                        .clicked()
                    {
                        self.save(true);
                    }
                });
            });
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        self.sync_editor();
        let connections = self
            .session
            .project()
            .map(|p| {
                p.graph
                    .nodes
                    .iter()
                    .filter(|n| n.config.spec().id.starts_with("hdfs_"))
                    .filter_map(|n| {
                        let value = serde_json::to_value(&n.config).ok()?;
                        Some((
                            format!(
                                "{} ({}:{})",
                                value["connection_name"].as_str().unwrap_or("HDFS"),
                                value["host"].as_str().unwrap_or(""),
                                value["port"]
                            ),
                            n.config.clone(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(editor) = &mut self.editor {
            editor.set_connections(connections);
        }
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
                            editor_action = ui
                                .add_enabled_ui(
                                    !self.file_dialogs.is_busy() && self.hdfs_browser.is_none(),
                                    |ui| editor.show(ui),
                                )
                                .inner;
                        }
                        ui.separator();
                        let pending = self.has_unapplied() || self.choosing_file();
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
                EditorAction::BrowseHdfs => {
                    if self.active_job() {
                        self.error =
                            Some("Wait for execution or cancel it before browsing HDFS".into());
                    } else if let Some(editor) = &self.editor {
                        match editor.config() {
                            Ok(config) => {
                                self.hdfs_browser = Some(hdfs_browser::Browser::new(
                                    editor.node_id,
                                    self.session.revision(),
                                    config,
                                    &mut self.session,
                                ))
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
                EditorAction::Browse { parameter, format }
                | EditorAction::BrowseDestination { parameter, format } => {
                    if let Some(editor) = &self.editor {
                        let initial = editor
                            .local_path(parameter)
                            .and_then(|path| file_dialog::source_start(path, self.session.path()));
                        self.request_file(
                            if matches!(action, EditorAction::BrowseDestination { .. }) {
                                FilePurpose::Destination {
                                    node_id: editor.node_id,
                                    parameter,
                                    format,
                                }
                            } else {
                                FilePurpose::Source {
                                    node_id: editor.node_id,
                                    parameter,
                                    format,
                                }
                            },
                            initial,
                        );
                    }
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
                            "Pipeline changed since this result was computed",
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
                                .spacing(egui::vec2(28.0, 6.0))
                                .min_col_width(64.0)
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
                    if ui.button("Save and close").clicked() {
                        if self.session.path().is_none() {
                            self.close_dialog = false;
                            self.save_dialog(true);
                        } else if self.save(false) {
                            self.finish_close(ctx);
                        }
                    }
                    if ui.button("Discard and close").clicked() {
                        self.finish_close(ctx);
                    }
                    if ui.button("Keep editing").clicked() {
                        self.close_dialog = false;
                    }
                });
            });
        } else if self.rename.is_some() {
            let mut apply = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("rename_project")).show(ctx, |ui| {
                ui.heading("Rename project");
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                if let Some(draft) = &mut self.rename {
                    ui.label("Project name");
                    let input = ui.text_edit_singleline(&mut draft.name);
                    if draft.focus {
                        input.request_focus();
                        draft.focus = false;
                    }
                }
                ui.label("This changes the project name, not the file name");
                ui.horizontal(|ui| {
                    apply = ui.button("Rename").clicked();
                    cancel = ui.button("Cancel rename").clicked();
                });
            });
            if apply {
                self.apply_rename();
            }
            if cancel {
                self.rename = None;
                self.error = None;
            }
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
        if let Some(mut browser) = self.hdfs_browser.take() {
            match browser.show(ctx, &mut self.session) {
                Some(hdfs_browser::Action::Select(path)) => {
                    if self.session.revision() == browser.revision {
                        if let Some(editor) =
                            self.editor.as_mut().filter(|e| e.node_id == browser.node)
                        {
                            if let Err(error) = editor.set_hdfs_path(path) {
                                self.error = Some(error);
                            }
                        }
                    } else {
                        self.error = Some(
                            "Project changed while browsing HDFS; selection was discarded".into(),
                        );
                    }
                    browser.close(&mut self.session);
                }
                Some(hdfs_browser::Action::Close) => browser.close(&mut self.session),
                None => self.hdfs_browser = Some(browser),
            }
        }
        if let Some(selection) = self.file_dialogs.poll(ctx) {
            self.handle_file_selection(selection, ctx);
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
        let choosing = self.choosing_file();
        let actions = egui::CentralPanel::default()
            .show(ctx, |ui| {
                if choosing {
                    ui.disable();
                }
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
            if choosing {
                continue;
            }
            match action {
                Action::Select(id) => self.select(id),
                Action::Move(node_id, position) => {
                    self.dispatch(Command::MoveNode { node_id, position });
                }
                Action::Connect(edge) => {
                    self.dispatch(Command::Connect { edge });
                }
                Action::RunNode(node_id) => {
                    self.dispatch(Command::RunNode { node_id });
                }
                Action::RunDownstream(node_id) => {
                    self.dispatch(Command::RunDownstream { node_id });
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
        if self.active_job() || self.choosing_file() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            self.request_close(ctx);
        }
        self.render(ctx);
        self.file_dialogs.start(frame);
        self.frames = self.frames.saturating_add(1);
        if self.smoke_frames.is_some_and(|limit| self.frames >= limit)
            && !self.has_unsaved()
            && !self.active_job()
            && !self.allow_close
            && !self.choosing_file()
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
mod tests;
