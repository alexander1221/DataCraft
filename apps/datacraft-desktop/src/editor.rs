//! Typed inspector buffers, separate from Project state. Contract: docs/GRAPH_EDITOR.md.
use datacraft_core::{
    registry::{ParameterRule, ParameterSpec, ParameterType},
    Node, NodeKind,
};
use eframe::egui;
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
enum FieldValue {
    String(String),
    List(Vec<String>),
    Boolean(bool),
    Number(String),
}

struct Field {
    spec: &'static ParameterSpec,
    value: FieldValue,
}

pub enum EditorAction {
    Apply,
    Revert,
    BrowseHdfs,
    BrowseDestination {
        parameter: &'static str,
        format: crate::file_dialog::SourceFormat,
    },
    Browse {
        parameter: &'static str,
        format: crate::file_dialog::SourceFormat,
    },
}

pub struct ParameterEditor {
    pub node_id: Uuid,
    pub original: NodeKind,
    fields: Vec<Field>,
    baseline: Vec<FieldValue>,
    connections: Vec<(String, NodeKind)>,
}

impl ParameterEditor {
    pub fn new(node: &Node) -> Result<Self, String> {
        let json = serde_json::to_value(&node.config).map_err(|e| e.to_string())?;
        let fields: Result<Vec<_>, String> = node
            .config
            .spec()
            .parameters
            .iter()
            .map(|spec| {
                let value = json
                    .get(spec.name)
                    .ok_or_else(|| format!("Missing parameter {}", spec.name))?;
                let value = match spec.parameter_type {
                    ParameterType::String => {
                        FieldValue::String(value.as_str().ok_or("Expected string")?.into())
                    }
                    ParameterType::StringList => FieldValue::List(
                        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                    ),
                    ParameterType::Boolean => {
                        FieldValue::Boolean(value.as_bool().ok_or("Expected boolean")?)
                    }
                    ParameterType::UnsignedInteger | ParameterType::Float => {
                        FieldValue::Number(value.to_string())
                    }
                };
                Ok(Field { spec, value })
            })
            .collect();
        let fields = fields?;
        let baseline = fields.iter().map(|f| f.value.clone()).collect();
        Ok(Self {
            node_id: node.id,
            original: node.config.clone(),
            fields,
            baseline,
            connections: Vec::new(),
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.fields
            .iter()
            .map(|f| &f.value)
            .ne(self.baseline.iter())
    }
    pub fn set_connections(&mut self, connections: Vec<(String, NodeKind)>) {
        self.connections = connections;
    }
    pub fn set_hdfs_path(&mut self, path: String) -> Result<(), String> {
        let field = self
            .fields
            .iter_mut()
            .find(|f| f.spec.name == "hdfs_path")
            .ok_or("Not an HDFS node")?;
        field.value = FieldValue::String(path);
        Ok(())
    }
    fn copy_connection(&mut self, config: &NodeKind) -> Result<(), String> {
        let json = serde_json::to_value(config).map_err(|e| e.to_string())?;
        for field in &mut self.fields {
            if [
                "connection_name",
                "host",
                "port",
                "protocol",
                "authentication",
                "username",
                "timeout_seconds",
            ]
            .contains(&field.spec.name)
            {
                let value = json.get(field.spec.name).ok_or("Invalid HDFS connection")?;
                field.value = if field.spec.parameter_type == ParameterType::UnsignedInteger {
                    FieldValue::Number(value.to_string())
                } else {
                    FieldValue::String(value.as_str().ok_or("Invalid HDFS field")?.into())
                };
            }
        }
        Ok(())
    }

    pub fn revert(&mut self) {
        for (field, original) in self.fields.iter_mut().zip(&self.baseline) {
            field.value = original.clone();
        }
    }

    pub fn local_path(&self, parameter: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| {
                f.spec.name == parameter
                    && f.spec
                        .rules
                        .iter()
                        .any(|r| matches!(r, ParameterRule::LocalPath))
            })
            .and_then(|f| match &f.value {
                FieldValue::String(path) => Some(path.as_str()),
                _ => None,
            })
    }

    pub fn set_local_path(
        &mut self,
        parameter: &str,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let value = path
            .to_str()
            .ok_or("The selected source path is not valid UTF-8")?;
        if !path.is_absolute() {
            return Err(
                "The file dialog returned a relative source path; enter it manually instead".into(),
            );
        }
        let field = self
            .fields
            .iter_mut()
            .find(|f| {
                f.spec.name == parameter
                    && f.spec
                        .rules
                        .iter()
                        .any(|r| matches!(r, ParameterRule::LocalPath))
            })
            .ok_or("This node has no matching local file parameter")?;
        match &mut field.value {
            FieldValue::String(text) => {
                *text = value.into();
                Ok(())
            }
            _ => Err("The file parameter is not a string".into()),
        }
    }

    pub fn config(&self) -> Result<NodeKind, String> {
        let mut values = Map::new();
        values.insert("kind".into(), self.original.spec().id.into());
        for field in &self.fields {
            let value = match &field.value {
                FieldValue::String(value) => Value::String(value.clone()),
                FieldValue::List(values) => {
                    serde_json::to_value(values).map_err(|e| e.to_string())?
                }
                FieldValue::Boolean(value) => Value::Bool(*value),
                FieldValue::Number(text) => match field.spec.parameter_type {
                    ParameterType::UnsignedInteger => {
                        Value::from(text.trim().parse::<u32>().map_err(|_| {
                            format!(
                                "{}: enter a whole number between 0 and {}",
                                field.spec.label,
                                u32::MAX
                            )
                        })?)
                    }
                    ParameterType::Float => {
                        let value = text
                            .trim()
                            .parse::<f64>()
                            .map_err(|_| format!("{}: enter a finite number", field.spec.label))?;
                        if !value.is_finite() {
                            return Err(format!("{}: enter a finite number", field.spec.label));
                        }
                        Value::from(value)
                    }
                    _ => return Err("Number field does not match node registry".into()),
                },
            };
            values.insert(field.spec.name.into(), value);
        }
        serde_json::from_value(Value::Object(values)).map_err(|e| e.to_string())
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<EditorAction> {
        ui.push_id(self.node_id, |ui| self.show_fields(ui)).inner
    }

    fn show_fields(&mut self, ui: &mut egui::Ui) -> Option<EditorAction> {
        let mut action = None;
        let hdfs = self.original.spec().id.starts_with("hdfs_");
        if hdfs {
            let mut selected = None;
            egui::ComboBox::from_id_salt("hdfs_connection")
                .selected_text("Copy existing connection…")
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in self.connections.iter().enumerate() {
                        if ui.selectable_label(false, name).clicked() {
                            selected = Some(i);
                        }
                    }
                });
            if let Some(i) = selected {
                let config = self.connections[i].1.clone();
                if let Err(error) = self.copy_connection(&config) {
                    ui.colored_label(egui::Color32::RED, error);
                }
            }
            ui.small(
                "WebHDFS: none or Simple user. No stored secrets or Kerberos password emulation.",
            );
            if self.original.spec().id.ends_with("_text") {
                ui.small(
                    "Use \\t, \\n, \\r, \\\\ for separators. Default quoting doubles close quotes.",
                );
            }
            if ui.button("Browse HDFS…").clicked() {
                action = Some(EditorAction::BrowseHdfs);
            }
        }
        let simple = self
            .fields
            .iter()
            .find(|f| f.spec.name == "authentication")
            .is_some_and(|f| matches!(&f.value,FieldValue::String(v) if v=="simple"));
        let source_format = match self.original {
            NodeKind::CsvSource { .. } => Some(crate::file_dialog::SourceFormat::Csv),
            NodeKind::ParquetSource { .. } | NodeKind::ParquetExport { .. } => {
                Some(crate::file_dialog::SourceFormat::Parquet)
            }
            NodeKind::JsonSource { .. } => Some(crate::file_dialog::SourceFormat::Json),
            _ => None,
        };
        for field in &mut self.fields {
            if hdfs && field.spec.name == "username" && !simple {
                continue;
            }
            ui.push_id(field.spec.name, |ui| {
                ui.strong(field.spec.label);
                if hdfs && (field.spec.name == "delimiter" || field.spec.name == "record_separator")
                {
                    if let FieldValue::String(value) = &mut field.value {
                        let presets: &[(&str, &str)] = if field.spec.name == "delimiter" {
                            &[
                                ("Tab", "\\t"),
                                ("Space", " "),
                                ("Comma", ","),
                                ("Semicolon", ";"),
                            ]
                        } else {
                            &[
                                ("Unix (LF)", "\\n"),
                                ("Windows (CRLF)", "\\r\\n"),
                                ("Mac (CR)", "\\r"),
                            ]
                        };
                        let label = presets
                            .iter()
                            .find(|(_, text)| *text == value)
                            .map_or("Other / custom", |(name, _)| *name);
                        egui::ComboBox::from_id_salt("separator_preset")
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                for (name, text) in presets {
                                    ui.selectable_value(value, (*text).into(), *name);
                                }
                            });
                    }
                }
                match &mut field.value {
                    FieldValue::String(value) | FieldValue::Number(value) => {
                        let choices = if hdfs || self.original.spec().id == "parquet_export" {
                            field.spec.rules.iter().find_map(|r| match r {
                                ParameterRule::OneOf { values } => Some(*values),
                                _ => None,
                            })
                        } else {
                            None
                        };
                        if let Some(choices) = choices {
                            egui::ComboBox::from_id_salt("choice")
                                .selected_text(value.as_str())
                                .show_ui(ui, |ui| {
                                    for choice in choices {
                                        ui.selectable_value(value, (*choice).into(), *choice);
                                    }
                                });
                        } else if hdfs && field.spec.name == "column_options" {
                            column_options(ui, value);
                        } else {
                            ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
                        }
                        if field
                            .spec
                            .rules
                            .iter()
                            .any(|r| matches!(r, ParameterRule::LocalPath))
                        {
                            if let Some(format) = source_format {
                                if ui.small_button("Browse…").clicked() {
                                    action = Some(if self.original.spec().id == "parquet_export" {
                                        EditorAction::BrowseDestination {
                                            parameter: field.spec.name,
                                            format,
                                        }
                                    } else {
                                        EditorAction::Browse {
                                            parameter: field.spec.name,
                                            format,
                                        }
                                    });
                                }
                            }
                            ui.small("Relative paths start in the saved project's folder");
                        }
                    }
                    FieldValue::Boolean(value) => {
                        ui.checkbox(value, "Enabled");
                    }
                    FieldValue::List(values) => {
                        let mut remove = None;
                        for (index, value) in values.iter_mut().enumerate() {
                            ui.push_id(index, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(value)
                                            .desired_width((ui.available_width() - 32.0).max(40.0)),
                                    );
                                    if ui.small_button("−").clicked() {
                                        remove = Some(index);
                                    }
                                });
                            });
                        }
                        if let Some(index) = remove {
                            values.remove(index);
                        }
                        if ui.small_button("Add column").clicked() {
                            values.push(String::new());
                        }
                    }
                }
            });
        }
        if self.fields.is_empty() {
            ui.label("This node has no parameters");
        }
        if self.original.spec().id == "hdfs_export_text" {
            ui.small("Table has column names and scalar values; long names, row names, value labels and spreadsheet display formats are unavailable.");
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.is_dirty(), egui::Button::new("Apply"))
                .clicked()
            {
                action = Some(EditorAction::Apply);
            }
            if ui
                .add_enabled(self.is_dirty(), egui::Button::new("Revert"))
                .clicked()
            {
                action = Some(EditorAction::Revert);
            }
        });
        if self.is_dirty() {
            ui.label("Unapplied changes — Apply or Revert before running");
        }
        action
    }
}

fn column_options(ui: &mut egui::Ui, value: &mut String) {
    let Ok(mut options) = serde_json::from_str::<Vec<Map<String, Value>>>(value) else {
        ui.colored_label(
            egui::Color32::RED,
            "Invalid column options JSON. Correct it before applying.",
        );
        ui.text_edit_multiline(value);
        return;
    };
    let before = options.clone();
    let mut remove = None;
    ui.small("Indexes refer to original columns, starting at 1.");
    for (index, option) in options.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.group(|ui| {
                let mut column = option.get("column").and_then(Value::as_u64).unwrap_or(1);
                ui.horizontal(|ui| {
                    ui.label("Column");
                    if ui
                        .add(egui::DragValue::new(&mut column).range(1..=10000))
                        .changed()
                    {
                        option.insert("column".into(), column.into());
                    }
                    if ui.small_button("Remove override").clicked() {
                        remove = Some(index);
                    }
                });
                for (key, label) in [
                    ("name", "Rename"),
                    ("input_format", "Date pattern (e.g. dd.MM.uuuu)"),
                    ("missing_value", "Replace missing value"),
                ] {
                    let mut enabled = option.contains_key(key);
                    if ui.checkbox(&mut enabled, label).changed() {
                        if enabled {
                            option.insert(key.into(), "".into());
                        } else {
                            option.remove(key);
                        }
                    }
                    if enabled {
                        let mut text = option
                            .get(key)
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        if ui.text_edit_singleline(&mut text).changed() {
                            option.insert(key.into(), text.into());
                        }
                    }
                }
                let mut data_type = option
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("auto")
                    .to_owned();
                egui::ComboBox::from_id_salt("column_type")
                    .selected_text(&data_type)
                    .show_ui(ui, |ui| {
                        for choice in ["auto", "string", "long", "double", "boolean", "date"] {
                            if ui
                                .selectable_value(&mut data_type, choice.into(), choice)
                                .changed()
                            {
                                option.insert("type".into(), data_type.clone().into());
                            }
                        }
                    });
                for (key, label) in [
                    ("omit", "Omit column"),
                    ("remove_non_numeric", "Remove non-numeric characters"),
                ] {
                    let mut enabled = option.get(key).and_then(Value::as_bool).unwrap_or(false);
                    if ui.checkbox(&mut enabled, label).changed() {
                        option.insert(key.into(), enabled.into());
                    }
                }
                let mut length = option
                    .get("text_length")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label("Text length limit (0 = none)");
                    if ui
                        .add(egui::DragValue::new(&mut length).range(0..=16777216))
                        .changed()
                    {
                        option.insert("text_length".into(), length.into());
                    }
                });
            });
        });
    }
    if let Some(index) = remove {
        options.remove(index);
    }
    if ui.small_button("Add column override").clicked() {
        let next = (1_u64..=10000).find(|n| {
            !options
                .iter()
                .any(|v| v.get("column").and_then(Value::as_u64) == Some(*n))
        });
        if let Some(column) = next {
            options.push(Map::from_iter([("column".into(), column.into())]));
        }
    }
    if options != before {
        match serde_json::to_string(&options) {
            Ok(text) => *value = text,
            Err(error) => {
                ui.colored_label(egui::Color32::RED, error.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdfs_connection_copy_and_remote_selection_are_reversible_inspector_buffers() {
        let source = datacraft_core::find_node("hdfs_import_text")
            .unwrap()
            .create_config(
                serde_json::from_value(serde_json::json!({"hdfs_path":"/original.tsv"})).unwrap(),
            )
            .unwrap();
        let connection=datacraft_core::find_node("hdfs_export_text").unwrap().create_config(
            serde_json::from_value(serde_json::json!({"hdfs_path":"/unrelated-output.tsv","host":"cluster.test","port":9871,
                "protocol":"swebhdfs","authentication":"simple","username":"analyst","connection_name":"Cluster","timeout_seconds":90})).unwrap()).unwrap();
        let mut editor = editor(source.clone());
        editor.copy_connection(&connection).unwrap();
        editor.set_hdfs_path("/picked.tsv".into()).unwrap();
        assert!(editor.is_dirty());
        assert_eq!(editor.original, source);
        let value = serde_json::to_value(editor.config().unwrap()).unwrap();
        assert_eq!(value["kind"], "hdfs_import_text");
        assert_eq!(value["host"], "cluster.test");
        assert_eq!(value["port"], 9871);
        assert_eq!(value["hdfs_path"], "/picked.tsv");
        assert_eq!(value["skip_rows"], 0);
        editor.revert();
        assert!(!editor.is_dirty());
        assert_eq!(editor.config().unwrap(), source);
    }
    fn editor(kind: NodeKind) -> ParameterEditor {
        ParameterEditor::new(&Node {
            id: Uuid::from_u128(1),
            position: [0.0, 0.0],
            config: kind,
        })
        .unwrap()
    }
    #[test]
    fn every_registry_kind_has_a_lossless_typed_editor() {
        for spec in datacraft_core::builtin_nodes() {
            let config = spec.create_draft_config().unwrap();
            let editor = editor(config.clone());
            assert!(!editor.is_dirty());
            assert_eq!(editor.config().unwrap(), config);
        }
        let config = NodeKind::SelectColumns {
            columns: vec![
                "comma,name".into(),
                "кириллица".into(),
                "line\nbreak".into(),
            ],
        };
        assert_eq!(editor(config.clone()).config().unwrap(), config);
    }
    #[test]
    fn invalid_numbers_do_not_replace_original_and_revert_restores_values() {
        let mut editor = editor(NodeKind::Limit { rows: 5 });
        for invalid in ["", "-1", "2.5", "4294967296", "NaN"] {
            editor.fields[0].value = FieldValue::Number(invalid.into());
            assert!(editor.is_dirty());
            assert!(editor.config().is_err());
            assert_eq!(editor.original, NodeKind::Limit { rows: 5 });
        }
        editor.fields[0].value = FieldValue::Number("12".into());
        assert_eq!(editor.config().unwrap(), NodeKind::Limit { rows: 12 });
        editor.revert();
        assert!(!editor.is_dirty());
        assert_eq!(editor.config().unwrap(), NodeKind::Limit { rows: 5 });
    }
    #[test]
    fn float_fields_reject_nonfinite_and_boolean_and_draft_strings_are_preserved() {
        let mut regression = editor(
            datacraft_core::find_node("linear_regression")
                .unwrap()
                .create_draft_config()
                .unwrap(),
        );
        let field = regression
            .fields
            .iter_mut()
            .find(|f| f.spec.name == "reg_param")
            .unwrap();
        field.value = FieldValue::Number("inf".into());
        assert!(regression.config().is_err());
        regression.revert();
        let mut source = editor(NodeKind::CsvSource {
            path: "data.csv".into(),
            header: true,
            infer_schema: true,
        });
        source
            .fields
            .iter_mut()
            .find(|f| f.spec.name == "header")
            .unwrap()
            .value = FieldValue::Boolean(false);
        assert!(matches!(
            source.config().unwrap(),
            NodeKind::CsvSource { header: false, .. }
        ));
        let filter = editor(NodeKind::Filter {
            expression: String::new(),
        });
        assert!(filter.config().unwrap().validate_parameters().is_err());
    }
}
