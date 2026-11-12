//! Persistent HDFS connection/text validation. See docs/HDFS_TEXT.md and docs/PARQUET_IO.md.
use serde_json::{Map, Value};

pub fn decode_escapes(text: &str) -> Result<String, &'static str> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next() {
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('\\') => '\\',
            Some('"') => '"',
            _ => return Err("use only \\n, \\r, \\t, \\\\ or \\\" escapes"),
        });
    }
    Ok(out)
}

pub(crate) fn validate(
    kind: &str,
    values: &Map<String, Value>,
) -> Result<(), (String, &'static str)> {
    let text = |key: &str| values.get(key).and_then(Value::as_str).unwrap_or("");
    let error = |key: &str, msg| (key.to_owned(), msg);
    let host = text("host");
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
    {
        return Err(error(
            "host",
            "enter a hostname or IP address without scheme, credentials or path",
        ));
    }
    let path = text("hdfs_path");
    if !path.starts_with('/')
        || path.contains(['\0', '?', '#', '\\'])
        || path.split('/').any(|p| p == "." || p == "..")
        || (kind.starts_with("hdfs_export_") && path.ends_with('/'))
    {
        return Err(error(
            "hdfs_path",
            "enter an absolute HDFS file path without query, fragment or dot segments",
        ));
    }
    if text("authentication") == "simple" && text("username").trim().is_empty() {
        return Err(error(
            "username",
            "Simple authentication requires a Hadoop username",
        ));
    }
    if !kind.ends_with("_text") {
        return Ok(());
    }
    for key in [
        "delimiter",
        "record_separator",
        "quote_start",
        "quote_end",
        "record_escape",
        "delimiter_escape",
        "quote_start_escape",
        "quote_end_escape",
    ] {
        let decoded = decode_escapes(text(key)).map_err(|m| error(key, m))?;
        let max = if key == "delimiter" || key == "record_separator" {
            16
        } else {
            1
        };
        if decoded.encode_utf16().count() > max || decoded.contains('\0') {
            return Err(error(key, "separator too long or contains NUL"));
        }
    }
    if text("quote_start").is_empty() != text("quote_end").is_empty() {
        return Err(error(
            "quote_end",
            "enable or disable both string quote characters",
        ));
    }
    let delimiter = decode_escapes(text("delimiter")).map_err(|m| error("delimiter", m))?;
    let records =
        decode_escapes(text("record_separator")).map_err(|m| error("record_separator", m))?;
    if delimiter.chars().any(|c| records.contains(c)) {
        return Err(error(
            "delimiter",
            "field and row separators must not overlap",
        ));
    }
    if kind == "hdfs_import_text" {
        let options: Value = serde_json::from_str(text("column_options"))
            .map_err(|_| error("column_options", "expected a JSON array"))?;
        let Some(options) = options.as_array() else {
            return Err(error("column_options", "expected a JSON array"));
        };
        let mut columns = std::collections::BTreeSet::new();
        for option in options {
            let Some(fields) = option.as_object() else {
                return Err(error("column_options", "expected column option objects"));
            };
            if fields.keys().any(|k| {
                ![
                    "column",
                    "name",
                    "type",
                    "omit",
                    "text_length",
                    "input_format",
                    "remove_non_numeric",
                    "missing_value",
                ]
                .contains(&k.as_str())
            }) || !fields
                .get("column")
                .and_then(Value::as_u64)
                .is_some_and(|v| v > 0 && v <= 10000 && columns.insert(v))
            {
                return Err(error(
                    "column_options",
                    "column indexes must be unique, 1-based, <=10000; unknown fields are forbidden",
                ));
            }
            if fields.get("type").is_some_and(|v| {
                !v.as_str().is_some_and(|s| {
                    ["auto", "string", "long", "double", "boolean", "date"].contains(&s)
                })
            }) {
                return Err(error("column_options", "unknown column type"));
            }
            for key in ["omit", "remove_non_numeric"] {
                if fields.get(key).is_some_and(|v| !v.is_boolean()) {
                    return Err(error(
                        "column_options",
                        "omit/remove_non_numeric must be boolean",
                    ));
                }
            }
            for key in ["name", "input_format", "missing_value"] {
                if fields.get(key).is_some_and(|v| !v.is_string()) {
                    return Err(error(
                        "column_options",
                        "name/input_format/missing_value must be strings",
                    ));
                }
            }
            if fields
                .get("text_length")
                .is_some_and(|v| !v.as_u64().is_some_and(|v| v <= 16777216))
            {
                return Err(error("column_options", "invalid text length"));
            }
        }
    }
    Ok(())
}
