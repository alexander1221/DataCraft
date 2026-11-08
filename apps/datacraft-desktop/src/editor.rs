//! Typed inspector buffers, separate from Project state. Contract: docs/GRAPH_EDITOR.md.
use datacraft_core::{
    registry::{ParameterSpec, ParameterType},
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
}

pub struct ParameterEditor {
    pub node_id: Uuid,
    pub original: NodeKind,
    fields: Vec<Field>,
    baseline: Vec<FieldValue>,
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
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.fields
            .iter()
            .map(|f| &f.value)
            .ne(self.baseline.iter())
    }

    pub fn revert(&mut self) {
        for (field, original) in self.fields.iter_mut().zip(&self.baseline) {
            field.value = original.clone();
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
        for field in &mut self.fields {
            ui.push_id(field.spec.name, |ui| {
                ui.strong(field.spec.label);
                match &mut field.value {
                    FieldValue::String(value) | FieldValue::Number(value) => {
                        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
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
        let mut action = None;
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

#[cfg(test)]
mod tests {
    use super::*;
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
