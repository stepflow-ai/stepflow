use std::io::BufReader;
use std::{fs::File, sync::Arc};

use serde::Deserialize;
use serde_json::{Map, Value};
use stepflow_execution::StepFlowExecutor;
use stepflow_main::StepflowConfig;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

static INIT_TEST_LOGGING: std::sync::Once = std::sync::Once::new();

/// Makes sure logging is initialized for test.
///
/// This needs to be called on each test.
pub fn init_test_logging() {
    INIT_TEST_LOGGING.call_once(|| {
        // We don't use a test writer for end to end tests.
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
    config: StepflowConfig,
    #[serde(flatten)]
    flow: stepflow_core::workflow::Flow,
    test_cases: Vec<TestCase>,
}

#[derive(Deserialize)]
struct TestCase {
    input: serde_json::Value,
}

fn run_tests(rt: tokio::runtime::Handle) {
    insta::glob!("flows/*.yaml", |path| {
        rt.block_on(async {
            let file = File::open(path).unwrap();
            let reader = BufReader::new(file);
            let TestFlow {
                mut config,
                flow,
                test_cases,
            } = serde_yml::from_reader::<_, TestFlow>(reader).unwrap_or_else(|e| {
                panic!("Failed to parse {path:?}: {e:?}");
            });

            config.working_directory = Some(path.parent().unwrap().to_path_buf());
            let executor = StepFlowExecutor::new(config.create_plugins().await.unwrap());

            let flow = Arc::new(flow);
            let mut settings = insta::Settings::clone_current();
            settings.set_input_file(path);

            for (index, test_case) in test_cases.into_iter().enumerate() {
                settings.set_description(format!("case {index}"));
                settings.set_info(&test_case.input);

                let result =
                    stepflow_main::run(executor.clone(), flow.clone(), test_case.input.into())
                        .await;

                let result = result.unwrap_or_else(|e| {
                    panic!("Running {path:?} test {index}: Expected success, but got error: {e:?}");
                });
                let result = normalize_value(result);
                settings.bind(|| {
                    insta::assert_yaml_snapshot!(result);
                });
            }
        })
    });
}

fn normalize_value(value: stepflow_core::FlowResult) -> stepflow_core::FlowResult {
    match value {
        stepflow_core::FlowResult::Success { result } => {
            let result = normalize_json(result.as_ref().to_owned()).into();
            stepflow_core::FlowResult::Success { result }
        }
        other => other,
    }
}

/// Recursively sorts all objects in a `serde_json::Value`.
fn normalize_json(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            // Normalize all values first
            let mut sorted = Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();

            for key in keys {
                let mut val = map.remove(&key).unwrap();

                // Normalize execution_id fields to a fixed value for snapshot testing
                if key == "execution_id" && val.is_string() {
                    val = Value::String("00000000-0000-0000-0000-000000000000".to_string());
                } else {
                    val = normalize_json(val);
                }

                sorted.insert(key, val);
            }

            Value::Object(sorted)
        }
        Value::Array(arr) => {
            // Recursively normalize array elements
            Value::Array(arr.drain(..).map(normalize_json).collect())
        }
        _ => value,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_flows() {
    init_test_logging();

    let rt = tokio::runtime::Handle::current();

    let handle = rt.clone().spawn_blocking(|| run_tests(rt));
    handle.await.unwrap();
}
