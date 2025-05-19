use std::fs::File;
use std::io::BufReader;

use serde::Deserialize;
use stepflow_execution::execute;
use stepflow_plugin::Plugins;
use stepflow_plugin_testing::{MockComponentBehavior, MockPlugin};
use stepflow_workflow::Value;
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

#[derive(Deserialize)]
struct TestFlow {
    #[serde(flatten)]
    flow: stepflow_workflow::Flow,
    test_cases: Vec<TestCase>,
}

#[derive(Deserialize)]
struct TestCase {
    input: serde_json::Value,
    #[serde(default)]
    expect_failure: bool,
}

#[test]
fn execute_flows() {
    init_test_logging();

    let mut plugins = Plugins::new();

    let mut mock_plugin = MockPlugin::new("mock");
    mock_plugin
        .mock_component("mock://one_output")
        .behavior(
            Value::new(serde_json::json!({ "input": "a" })),
            MockComponentBehavior::Valid {
                output: Value::new(serde_json::json!({ "output": "b" })),
            },
        )
        .behavior(
            Value::new(serde_json::json!({ "input": "hello" })),
            MockComponentBehavior::Valid {
                output: Value::new(serde_json::json!({ "output": "world" })),
            },
        );
    mock_plugin
        .mock_component("mock://two_outputs")
        .behavior(
            Value::new(serde_json::json!({ "input": "b" })),
            MockComponentBehavior::Valid {
                output: Value::new(serde_json::json!({ "x": 1, "y": 2 })),
            },
        )
        .behavior(
            Value::new(serde_json::json!({ "input": "world" })),
            MockComponentBehavior::Valid {
                output: Value::new(serde_json::json!({ "x": 2, "y": 8 })),
            },
        );
    plugins.register(mock_plugin);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    insta::glob!("flows/*.yaml", |path| {
        rt.block_on(async {
            let file = File::open(path).unwrap();
            let reader = BufReader::new(file);
            let TestFlow { flow, test_cases } =
                serde_yml::from_reader::<_, TestFlow>(reader).unwrap();

            let mut settings = insta::Settings::clone_current();
            settings.set_input_file(path);

            for (index, test_case) in test_cases.into_iter().enumerate() {
                settings.set_description(format!("case {index}"));
                settings.set_info(&test_case.input);

                let result = execute(&plugins, &flow, Value::new(test_case.input)).await;
                if test_case.expect_failure {
                    let result = result.unwrap_err();
                    settings.bind(|| {
                        insta::assert_yaml_snapshot!(result);
                    });
                } else {
                    let result = result.unwrap();
                    settings.bind(|| {
                        insta::assert_yaml_snapshot!(result);
                    });
                }
            }
        })
    })
}
