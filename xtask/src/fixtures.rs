//! Real Session execution against an independent JSON oracle. See architecture checks §8.
use datacraft_engine::jobs::{JobOptions, JobStatus};
use datacraft_engine::{Command, CommandPayload, ResultTable, Session, WorkerConfig};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Component, Path},
    time::Duration,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    format_version: u32,
    numerical_assumption: Option<String>,
    pipelines: Vec<Pipeline>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pipeline {
    project: String,
    readiness_issues: usize,
    tables: Vec<Table>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    name: String,
    fields: Vec<Field>,
    rows: Vec<Value>,
    absolute_tolerance: Option<f64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    name: String,
    data_type: FieldType,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum FieldType {
    Integer,
    Long,
    Double,
    String,
    Boolean,
}
impl FieldType {
    fn spark(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Long => "long",
            Self::Double => "double",
            Self::String => "string",
            Self::Boolean => "boolean",
        }
    }
    fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Integer => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
            Self::Long => value.as_i64().is_some(),
            Self::Double => value.as_f64().is_some_and(f64::is_finite),
            Self::String => value.is_string(),
            Self::Boolean => value.is_boolean(),
        }
    }
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|e| format!("fixture manifest JSON: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }
    fn validate(&self) -> Result<(), String> {
        if self.format_version != 1 || self.pipelines.is_empty() {
            return Err("unsupported/empty fixture manifest".into());
        }
        let mut paths = BTreeSet::new();
        let mut provisional = false;
        for pipeline in &self.pipelines {
            let path = Path::new(&pipeline.project);
            if !path.components().all(|c| matches!(c, Component::Normal(_)))
                || path.extension().and_then(|s| s.to_str()) != Some("dcraft")
                || !paths.insert(&pipeline.project)
            {
                return Err(format!(
                    "invalid or duplicate fixture path {}",
                    pipeline.project
                ));
            }
            let mut names = BTreeSet::new();
            if pipeline.tables.is_empty() {
                return Err("fixture must expect result tables".into());
            }
            for table in &pipeline.tables {
                if table.name.trim().is_empty()
                    || !names.insert(&table.name)
                    || table.fields.is_empty()
                {
                    return Err("empty or duplicate table contract".into());
                }
                let fields: BTreeSet<_> = table.fields.iter().map(|f| f.name.as_str()).collect();
                if fields.len() != table.fields.len() || fields.iter().any(|f| f.trim().is_empty())
                {
                    return Err("empty or duplicate expected field".into());
                }
                if let Some(tolerance) = table.absolute_tolerance {
                    if !tolerance.is_finite()
                        || tolerance < 0.0
                        || !table
                            .fields
                            .iter()
                            .any(|f| f.data_type == FieldType::Double)
                    {
                        return Err("invalid numerical tolerance".into());
                    }
                    provisional |= tolerance > 0.0;
                }
                for row in &table.rows {
                    validate_row(row, &table.fields)?;
                }
            }
        }
        if provisional
            && !self
                .numerical_assumption
                .as_deref()
                .is_some_and(|s| s.starts_with("ASSUMPTION:"))
        {
            return Err("provisional numerical tolerances require an explicit ASSUMPTION".into());
        }
        Ok(())
    }
}

fn validate_row(row: &Value, fields: &[Field]) -> Result<(), String> {
    let object = row.as_object().ok_or("expected row object")?;
    let actual: BTreeSet<_> = object.keys().map(String::as_str).collect();
    let expected: BTreeSet<_> = fields.iter().map(|f| f.name.as_str()).collect();
    if actual != expected {
        return Err("row fields do not match schema".into());
    }
    for field in fields {
        if !field.data_type.accepts(&object[&field.name]) {
            return Err(format!("invalid value type for {}", field.name));
        }
    }
    Ok(())
}

fn compare(expected: &[Table], actual: &[ResultTable]) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "expected {} tables, got {}",
            expected.len(),
            actual.len()
        ));
    }
    for (table, result) in expected.iter().zip(actual) {
        if result.name != table.name {
            return Err(format!(
                "expected table {}, got {}",
                table.name, result.name
            ));
        }
        if result.schema.get("type").and_then(Value::as_str) != Some("struct") {
            return Err("expected Spark struct schema".into());
        }
        let fields = result
            .schema
            .get("fields")
            .and_then(Value::as_array)
            .ok_or("missing schema fields")?;
        if fields.len() != table.fields.len() {
            return Err(format!("{}: schema width mismatch", table.name));
        }
        for (field, actual) in table.fields.iter().zip(fields) {
            if actual.get("name").and_then(Value::as_str) != Some(&field.name)
                || actual.get("type").and_then(Value::as_str) != Some(field.data_type.spark())
            {
                return Err(format!("{}: schema mismatch at {}", table.name, field.name));
            }
        }
        if table.rows.len() != result.rows.len() {
            return Err(format!(
                "{}: expected {} rows, got {}",
                table.name,
                table.rows.len(),
                result.rows.len()
            ));
        }
        for (index, (expected, actual)) in table.rows.iter().zip(&result.rows).enumerate() {
            validate_row(actual, &table.fields)?;
            for field in &table.fields {
                let a = &actual[&field.name];
                let e = &expected[&field.name];
                let equal = if field.data_type == FieldType::Double {
                    match (a.as_f64(), e.as_f64()) {
                        (Some(a), Some(e)) => {
                            a.is_finite()
                                && e.is_finite()
                                && (a - e).abs().is_finite()
                                && (a - e).abs() <= table.absolute_tolerance.unwrap_or(0.0)
                        }
                        _ => false,
                    }
                } else {
                    a == e
                };
                if !equal {
                    return Err(format!(
                        "{} row {} column {}: expected {e}, got {a}",
                        table.name,
                        index + 1,
                        field.name
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn run(
    root: &Path,
    manifest: &Manifest,
    config: WorkerConfig,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(assumption) = &manifest.numerical_assumption {
        eprintln!("[WARNING] {assumption}");
    }
    for pipeline in &manifest.pipelines {
        let path = root.join(&pipeline.project);
        let mut session = Session::default();
        session.configure_worker(config.clone());
        session.configure_job_options(JobOptions {
            timeout: Some(timeout),
        });
        session.dispatch(Command::Open {
            path,
            discard_changes: false,
        })?;
        let project = session.project().ok_or("project did not open")?;
        let diagnostics = project.graph.diagnostics()?;
        if diagnostics.len() != pipeline.readiness_issues {
            return Err(format!(
                "{}: expected {} readiness issues, got {}",
                pipeline.project,
                pipeline.readiness_issues,
                diagnostics.len()
            )
            .into());
        }
        let CommandPayload::JobStarted { job_id, .. } =
            session.dispatch(Command::Run { output: None })?.payload
        else {
            return Err("execution did not return a job".into());
        };
        let info =
            session.wait_for_execution(job_id, timeout.saturating_add(Duration::from_secs(30)))?;
        if info.status != JobStatus::Completed {
            return Err(format!(
                "{}: job {:?}: {:?}",
                pipeline.project, info.status, info.error
            )
            .into());
        }
        let result = session
            .job_result(job_id)?
            .ok_or("completed fixture has no result")?;
        compare(&pipeline.tables, &result.result.tables)
            .map_err(|e| format!("{}: {e}", pipeline.project))?;
        println!(
            "[PASS] {}: tables, schemas and values match oracle",
            pipeline.project
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn manifest() -> Manifest {
        Manifest::parse(include_str!("../../examples/expected/pipelines.json")).unwrap()
    }
    fn actual(table: &Table) -> ResultTable {
        ResultTable {
            name: table.name.clone(),
            schema: json!({"type":"struct","fields":table.fields.iter().map(|f|json!({"name":f.name,"type":f.data_type.spark()})).collect::<Vec<_>>()}),
            rows: table.rows.clone(),
        }
    }
    #[test]
    fn independently_declared_exact_and_provisional_contracts_are_enforced() {
        let manifest = manifest();
        for pipeline in &manifest.pipelines {
            let actual: Vec<_> = pipeline.tables.iter().map(actual).collect();
            compare(&pipeline.tables, &actual).unwrap();
        }
        let tables = &manifest.pipelines[0].tables;
        let mut rows: Vec<_> = tables.iter().map(actual).collect();
        rows[0].rows[0]["quantity"] = json!(99);
        assert!(compare(tables, &rows).is_err());
        rows[0] = actual(&tables[0]);
        rows[0].rows.pop();
        assert!(compare(tables, &rows).is_err());
        rows[0] = actual(&tables[0]);
        rows[0].schema["fields"][0]["type"] = json!("double");
        assert!(compare(tables, &rows).is_err());
    }
    #[test]
    fn ml_comparator_rejects_wrong_estimates_null_and_extraneous_fields() {
        let manifest = manifest();
        let tables = &manifest.pipelines[2].tables;
        let mut results: Vec<_> = tables.iter().map(actual).collect();
        results[0].rows[0]["estimate"] = json!(4.0);
        assert!(compare(tables, &results).is_err());
        results[0] = actual(&tables[0]);
        results[0].rows[0]["estimate"] = Value::Null;
        assert!(compare(tables, &results).is_err());
        results[0] = actual(&tables[0]);
        results[0].rows[0]["extra"] = json!(1);
        assert!(compare(tables, &results).is_err());
    }
    #[test]
    fn malformed_manifests_unknown_fields_paths_and_tolerances_are_rejected() {
        let mut invalid = manifest();
        invalid.pipelines[0].project = "../outside.dcraft".into();
        assert!(invalid.validate().is_err());
        let mut invalid = manifest();
        invalid.pipelines[0].tables[0].absolute_tolerance = Some(-1.0);
        assert!(invalid.validate().is_err());
        let mut invalid = manifest();
        invalid.numerical_assumption = None;
        assert!(invalid.validate().is_err());
        assert!(Manifest::parse(r#"{"format_version":1,"pipelines":[],"typo":true}"#).is_err());
    }
}
