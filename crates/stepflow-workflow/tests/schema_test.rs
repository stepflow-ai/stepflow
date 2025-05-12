#[test]
fn test_schema_up_to_date() {
    let schema = schemars::schema_for!(stepflow_workflow::Flow);
    let schema = serde_json::to_string_pretty(&schema).unwrap();

    // If UPDATE_SCHEMA is true, update the schema file (`workflow.schema.json`).
    // Otherwise, ensure the schema is up to date with the source code.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let schema_path = std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("workflow.schema.json");

    if std::env::var("UPDATE_SCHEMA").is_ok() {
        std::fs::write(&schema_path, schema).unwrap();
    } else if schema_path.is_file() {
        let expected_schema = std::fs::read_to_string(&schema_path).unwrap();
        assert_eq!(schema, expected_schema);
    } else {
        panic!("{} not found", schema_path.display());
    }
}
