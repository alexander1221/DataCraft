use datacraft_core::{Graph, GraphError, NodeKind};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    #[serde(flatten)]
    pub graph: Graph,
    pub output_node: Option<Uuid>,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed project: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported project format: {0}")]
    Version(serde_json::Value),
    #[error("project name must not be empty")]
    EmptyName,
    #[error(transparent)]
    Graph(#[from] GraphError),
}

impl Project {
    pub fn from_json(json: &str) -> Result<Self, ProjectError> {
        // Inspect version before interpreting the version-specific schema.
        let value: serde_json::Value = serde_json::from_str(json)?;
        let version = value.get("format_version").cloned().unwrap_or_default();
        if version.as_u64() != Some(u64::from(FORMAT_VERSION)) {
            return Err(ProjectError::Version(version));
        }
        let project: Self = serde_json::from_value(value)?;
        project.validate_structure()?;
        Ok(project)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        self.validate_structure()?;
        self.graph.validate()?;
        Ok(())
    }

    /// Allow incomplete parameters/inputs while rejecting corrupt project data.
    pub fn validate_structure(&self) -> Result<(), ProjectError> {
        if self.format_version != FORMAT_VERSION {
            return Err(ProjectError::Version(self.format_version.into()));
        }
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName);
        }
        self.graph.validate_structure()?;
        if let Some(id) = self.output_node {
            self.graph.output(Some(id))?;
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        Self::from_json(&fs::read_to_string(path)?)
    }

    pub fn to_json(&self) -> Result<String, ProjectError> {
        self.validate_structure()?;
        let mut canonical = self.clone();
        canonical.graph.nodes.sort_by_key(|n| n.id);
        canonical.graph.edges.sort();
        Ok(serde_json::to_string_pretty(&canonical)? + "\n")
    }

    /// Same-directory temporary file and atomic replacement; protect future formats.
    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let json = self.to_json()?;
        if path.exists() {
            Self::load(path)?;
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(json.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|e| ProjectError::Io(e.error))?;
        Ok(())
    }

    /// Runtime copy only: keep persistent source paths relative in saved projects.
    pub fn resolved(&self, project_file: &Path) -> Result<Self, ProjectError> {
        self.validate_structure()?;
        let directory = fs::canonicalize(project_file)?
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let mut resolved = self.clone();
        for node in &mut resolved.graph.nodes {
            let path = match &mut node.config {
                NodeKind::CsvSource { path, .. } | NodeKind::ParquetSource { path } => path,
                _ => continue,
            };
            // Preserve unfinished source values. Only a selected ready branch
            // may execute; unrelated draft placeholders must not become paths.
            if path.trim().is_empty() || path.contains("://") {
                continue;
            }
            let local = PathBuf::from(&*path);
            let absolute = if local.is_absolute() {
                local
            } else {
                directory.join(local)
            };
            *path = absolute.to_string_lossy().into_owned();
        }
        Ok(resolved)
    }
}
