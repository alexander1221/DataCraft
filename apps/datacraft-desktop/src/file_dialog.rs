//! Native picker lifecycle, separate from persistent data. Contract: docs/FILE_DIALOGS.md.
use eframe::egui;
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    Csv,
    Parquet,
    Json,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Purpose {
    OpenProject,
    SaveProject {
        close_after: bool,
    },
    Source {
        node_id: Uuid,
        parameter: &'static str,
        format: SourceFormat,
    },
    Destination {
        node_id: Uuid,
        parameter: &'static str,
        format: SourceFormat,
    },
}

#[derive(Clone, Debug)]
pub struct Request {
    pub purpose: Purpose,
    pub revision: u64,
    pub initial_path: Option<PathBuf>,
}

pub struct Selection {
    pub request: Request,
    pub path: Option<PathBuf>,
}

type DialogFuture = Pin<Box<dyn Future<Output = Option<PathBuf>>>>;

struct Repaint(egui::Context);
impl Wake for Repaint {
    fn wake(self: Arc<Self>) {
        self.0.request_repaint();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.request_repaint();
    }
}

#[derive(Default)]
pub struct FileDialogs {
    queued: Option<Request>,
    active: Option<(Request, DialogFuture)>,
}

impl FileDialogs {
    #[cfg(test)]
    pub fn take_queued_for_test(&mut self) -> Option<Request> {
        self.queued.take()
    }
    pub fn is_busy(&self) -> bool {
        self.queued.is_some() || self.active.is_some()
    }

    pub fn queue(&mut self, request: Request) -> Result<(), String> {
        if self.is_busy() {
            return Err("A file dialog is already open".into());
        }
        self.queued = Some(request);
        Ok(())
    }

    /// Called only by the native App on its GUI thread; never from headless rendering tests.
    pub fn start(&mut self, parent: &eframe::Frame) {
        let Some(request) = self.queued.take() else {
            return;
        };
        let mut dialog = rfd::AsyncFileDialog::new().set_parent(parent);
        let save = matches!(
            request.purpose,
            Purpose::SaveProject { .. } | Purpose::Destination { .. }
        );
        dialog = match request.purpose {
            Purpose::OpenProject => dialog
                .set_title("Open DataCraft project")
                .add_filter("DataCraft projects", &["dcraft"]),
            Purpose::SaveProject { .. } => dialog
                .set_title("Save DataCraft project as")
                .add_filter("DataCraft projects", &["dcraft"]),
            Purpose::Destination { format, .. } => {
                let (label, extensions) = match format {
                    SourceFormat::Parquet => ("Parquet data", &["parquet"][..]),
                    SourceFormat::Csv => ("CSV data", &["csv"][..]),
                    SourceFormat::Json => ("JSON data", &["json"][..]),
                };
                dialog
                    .set_title("Choose export destination")
                    .add_filter(label, extensions)
            }
            Purpose::Source {
                format: SourceFormat::Json,
                ..
            } => dialog
                .set_title("Choose JSON source")
                .add_filter("JSON data", &["json", "jsonl", "ndjson"]),
            Purpose::Source {
                format: SourceFormat::Csv,
                ..
            } => dialog
                .set_title("Choose CSV source")
                .add_filter("CSV data", &["csv"]),
            Purpose::Source {
                format: SourceFormat::Parquet,
                ..
            } => dialog
                .set_title("Choose Parquet source")
                .add_filter("Parquet data", &["parquet"]),
        };
        if let Some(path) = &request.initial_path {
            let directory = if path.is_dir() {
                Some(path.as_path())
            } else {
                path.parent()
            };
            if let Some(directory) = directory.filter(|p| !p.as_os_str().is_empty()) {
                dialog = dialog.set_directory(directory);
            }
            if save {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    dialog = dialog.set_file_name(name);
                }
            }
        } else if save {
            dialog =
                dialog.set_file_name(if matches!(request.purpose, Purpose::Destination { .. }) {
                    "data.parquet"
                } else {
                    "project.dcraft"
                });
        }
        // Construct the rfd task here (main thread); polling it does not block egui.
        let future: DialogFuture = if save {
            let task = dialog.save_file();
            Box::pin(async move { task.await.map(|file| file.path().to_owned()) })
        } else {
            let task = dialog.pick_file();
            Box::pin(async move { task.await.map(|file| file.path().to_owned()) })
        };
        self.active = Some((request, future));
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Selection> {
        let (_, future) = self.active.as_mut()?;
        let waker = Waker::from(Arc::new(Repaint(ctx.clone())));
        match future.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(path) => self
                .active
                .take()
                .map(|(request, _)| Selection { request, path }),
            Poll::Pending => {
                ctx.request_repaint_after(Duration::from_millis(50));
                None
            }
        }
    }
}

pub fn project_extension(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("dcraft");
    }
    path
}

pub fn source_start(path: &str, project: Option<&Path>) -> Option<PathBuf> {
    if path.trim().is_empty() {
        return project.and_then(Path::parent).map(Path::to_owned);
    }
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Some(path)
    } else {
        project.and_then(Path::parent).map(|base| base.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            purpose: Purpose::OpenProject,
            revision: 4,
            initial_path: None,
        }
    }
    #[test]
    fn only_one_dialog_is_allowed_and_completion_is_delivered_once() {
        let mut dialogs = FileDialogs::default();
        dialogs.queue(request()).unwrap();
        assert!(dialogs.queue(request()).is_err());
        let request = dialogs.queued.take().unwrap();
        dialogs.active = Some((
            request,
            Box::pin(std::future::ready(Some(PathBuf::from("project.dcraft")))),
        ));
        let ctx = egui::Context::default();
        let result = dialogs.poll(&ctx).unwrap();
        assert_eq!(result.request.revision, 4);
        assert_eq!(result.path, Some("project.dcraft".into()));
        assert!(!dialogs.is_busy());
        assert!(dialogs.poll(&ctx).is_none());
    }
    #[test]
    fn pending_is_nonblocking_and_cancel_is_not_a_path() {
        let ctx = egui::Context::default();
        let mut dialogs = FileDialogs {
            queued: None,
            active: Some((request(), Box::pin(std::future::pending()))),
        };
        assert!(dialogs.poll(&ctx).is_none());
        assert!(dialogs.is_busy());
        dialogs.active = Some((request(), Box::pin(std::future::ready(None))));
        assert!(dialogs.poll(&ctx).unwrap().path.is_none());
        assert!(!dialogs.is_busy());
    }
    #[test]
    fn extension_and_source_directory_do_not_use_working_directory() {
        assert_eq!(
            project_extension("report".into()),
            PathBuf::from("report.dcraft")
        );
        assert_eq!(
            project_extension("report.v1.dcraft".into()),
            PathBuf::from("report.v1.dcraft")
        );
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.dcraft");
        assert_eq!(
            source_start("data/sales.csv", Some(&project)),
            Some(dir.path().join("data/sales.csv"))
        );
        assert_eq!(
            source_start("", Some(&project)),
            Some(dir.path().to_owned())
        );
        assert!(source_start("data/sales.csv", None).is_none());
    }
}
