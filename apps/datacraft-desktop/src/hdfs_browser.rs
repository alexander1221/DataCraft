//! Remote picker over Session jobs; no filesystem/network implementation in the UI.
use datacraft_core::NodeKind;
use datacraft_engine::{Command, CommandPayload, Session};
use eframe::egui;
use uuid::Uuid;

pub struct Browser {
    pub node: Uuid,
    pub revision: u64,
    config: NodeKind,
    path: String,
    job: Option<Uuid>,
    entries: Vec<(String, bool, u64)>,
    error: Option<String>,
}
pub enum Action {
    Close,
    Select(String),
}
impl Browser {
    pub fn new(node: Uuid, revision: u64, config: NodeKind, session: &mut Session) -> Self {
        let mut browser = Self {
            node,
            revision,
            config,
            path: "/".into(),
            job: None,
            entries: Vec::new(),
            error: None,
        };
        browser.start(session);
        browser
    }
    fn start(&mut self, session: &mut Session) {
        match session.dispatch(Command::HdfsList {
            node_id: self.node,
            config: self.config.clone(),
            path: self.path.clone(),
        }) {
            Ok(result) => {
                if let CommandPayload::JobStarted { job_id, .. } = result.payload {
                    self.job = Some(job_id);
                    self.error = None;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub fn close(&mut self, session: &mut Session) {
        if let Some(job_id) = self.job.take() {
            let _ = session.dispatch(Command::Cancel { job_id });
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, session: &mut Session) -> Option<Action> {
        if let Some(id) = self.job {
            match session.job_status(id) {
                Ok(info) if info.status.is_terminal() => {
                    self.job = None;
                    self.entries.clear();
                    if let Some(error) = info.error {
                        self.error = Some(error.message);
                    } else if info.status != datacraft_engine::jobs::JobStatus::Completed {
                        self.error = Some(format!("HDFS browsing ended: {:?}", info.status));
                    } else if let Ok(Some(record)) = session.job_result(id) {
                        if let Some(table) = record.result.tables.first() {
                            self.entries = table
                                .rows
                                .iter()
                                .filter_map(|r| {
                                    Some((
                                        r["path"].as_str()?.into(),
                                        r["directory"].as_bool()?,
                                        r["length"].as_str()?.parse().ok()?,
                                    ))
                                })
                                .collect();
                            self.entries.sort_by(|a, b| (!a.1, &a.0).cmp(&(!b.1, &b.0)));
                        }
                    }
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    self.job = None;
                }
                _ => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            }
        }
        let mut action = None;
        let mut navigate = None;
        egui::Modal::new(egui::Id::new("hdfs_picker")).show(ctx, |ui| {
            ui.set_min_width(540.0);
            ui.heading("HDFS files");
            ui.label("Read-only browser; first 1000 entries. Enter a new file path for export.");
            ui.label("Directory or file path");
            ui.horizontal(|ui| {
                ui.add_enabled(
                    self.job.is_none(),
                    egui::TextEdit::singleline(&mut self.path).desired_width(380.0),
                );
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new("List"))
                    .clicked()
                {
                    navigate = Some(self.path.clone());
                }
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Up"))
                    .clicked()
                {
                    navigate = Some(
                        self.path
                            .trim_end_matches('/')
                            .rsplit_once('/')
                            .map_or(
                                "/",
                                |(parent, _)| if parent.is_empty() { "/" } else { parent },
                            )
                            .into(),
                    );
                }
            });
            if self.job.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Reading HDFS directory…");
                });
            }
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (path, directory, bytes) in &self.entries {
                        let label = if *directory {
                            format!("[Directory] {path}")
                        } else {
                            format!("{path} ({bytes} bytes)")
                        };
                        if ui
                            .add_enabled(self.job.is_none(), egui::Button::new(label))
                            .clicked()
                        {
                            if *directory {
                                navigate = Some(path.clone());
                            } else {
                                self.path = path.clone();
                            }
                        }
                    }
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Use path"))
                    .clicked()
                {
                    action = Some(Action::Select(self.path.clone()));
                }
                if ui.button("Close browser").clicked() {
                    action = Some(Action::Close);
                }
            });
        });
        if let Some(path) = navigate {
            self.path = path;
            self.start(session);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some(Action::Close);
        }
        action
    }
}
