use std::path::PathBuf;

use serde_json::json;
use stepflow_plugin::Plugin;
use stepflow_plugin_protocol::stdio::{Client, StdioPlugin};
use stepflow_workflow::Component;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

static INIT_TEST_LOGGING: std::sync::Once = std::sync::Once::new();

/// Makes sure logging is initialized for test.
///
/// This needs to be called on each test.
pub fn init_test_logging() {
    INIT_TEST_LOGGING.call_once(|| {
        // We don't use a test writer for end to end testts.
        let fmt_layer = tracing_subscriber::fmt::layer();

        tracing_subscriber::registry()
            .with(EnvFilter::new("stepflow_=trace,info"))
            .with(fmt_layer)
            .with(tracing_error::ErrorLayer::default())
            .try_init()
            .unwrap();
    });
}

#[tokio::test]
async fn test_initialize_disconnect() {
    init_test_logging();
    tracing::info!("Starting test");

    let uv = which::which("uv").unwrap();
    tracing::info!("Found uv at {uv:?}");

    // Determine the path to the `sdks/python` directory from the CARGO_MANIFEST_DIR.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let python_dir: PathBuf = [&manifest_dir, "..", "..", "sdks", "python"]
        .iter()
        .collect();
    let python_dir = python_dir.to_str().unwrap();

    let client = Client::builder(uv)
        .args(["--project", python_dir, "run", "stepflow_sdk"])
        .build()
        .await
        .unwrap();

    let plugin = StdioPlugin::new(client.handle());
    plugin.init().await.unwrap();
    tracing::info!("Initialized plugin");

    let add_component = Component::parse("python://add").unwrap();
    let add_info = plugin.component_info(&add_component).await.unwrap();

    let expected_input_schema = json!({
        "$ref": "#/$defs/AddInput",
        "$defs": {
          "AddInput": {
            "title": "AddInput",
            "type": "object",
            "properties": {
              "a": {
                "type": "integer"
              },
              "b": {
                "type": "integer"
              }
            },
            "required": [
              "a",
              "b"
            ]
          }
        }
    });

    similar_asserts::assert_serde_eq!(
        serde_json::to_value(&add_info.input_schema).unwrap(),
        expected_input_schema
    );

    let mut add_input = serde_json::Map::with_capacity(2);
    add_input.insert("a".to_string(), 1.into());
    add_input.insert("b".to_string(), 2.into());
    let add_output = plugin
        .execute(&add_component, add_input.into())
        .await
        .unwrap();

    let expected_output = json!({
        "result": 3
    });
    similar_asserts::assert_serde_eq!(add_output.as_ref(), &expected_output);

    // TODO: Close the plugin/client?
}
