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

#[tokio::test(flavor = "multi_thread")]
async fn test_run_test_workflows() {
    init_test_logging();

    // Test the stepflow test command on our test directory
    let test_path = std::path::Path::new("../../tests");
    let test_options = stepflow_main::test::TestOptions {
        cases: vec![],
        update: false,
        diff: false,
    };

    // This should find and run tests in the test directory
    let result = stepflow_main::test::run_tests(test_path, None, test_options).await;

    // The test should succeed (basic.yaml should pass, no_tests.yaml should be skipped)
    let result = result.expect("Test command should succeed");
    assert_eq!(result, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_run_example_workflows() {
    init_test_logging();

    // Test the stepflow test command on our examples directory
    let test_path = std::path::Path::new("../../../examples");
    let test_options = stepflow_main::test::TestOptions {
        cases: vec![],
        update: false,
        diff: false,
    };

    // This should find and run tests in the examples directory
    let result = stepflow_main::test::run_tests(test_path, None, test_options).await;

    // The test should succeed - all example workflows with test cases should pass
    let result = result.expect("Example test command should succeed");
    assert_eq!(result, 0);
}
