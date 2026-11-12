use datacraft_engine::ResultTable;

#[test]
fn spark_result_numbers_keep_int64_and_decimal_tokens_without_float_rounding() {
    let table:ResultTable=serde_json::from_str(r#"{"name":"precise","schema":{"type":"struct","fields":[]},"rows":[{"id":9007199254740993,"amount":12345678901234567890.1234567890,"missing":null}]}"#).unwrap();
    assert_eq!(table.rows[0]["id"].as_i64(), Some(9_007_199_254_740_993));
    assert_eq!(
        table.rows[0]["amount"].to_string(),
        "12345678901234567890.1234567890"
    );
    assert!(table.rows[0]["missing"].is_null());
    let wire = serde_json::to_string(&table).unwrap();
    assert!(wire.contains("12345678901234567890.1234567890"));
}
