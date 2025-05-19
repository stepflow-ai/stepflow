use std::fs::File;
use std::io::BufReader;

use stepflow_compiler::compile;
use stepflow_plugin::Plugins;
use stepflow_plugin_testing::MockPlugin;

#[test]
fn valid_snapshot() {
    let mut plugins = Plugins::new();

    let mut mock_plugin = MockPlugin::new("mock");
    mock_plugin.mock_component("mock://one_output");
    // .outputs(&["output"]);
    mock_plugin.mock_component("mock://two_outputs");
    // .outputs(&["a", "b"]);
    plugins.register("mock".to_owned(), mock_plugin);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    insta::glob!("valid/*.yaml", |path| {
        rt.block_on(async {
            let file = File::open(path).unwrap();
            let reader = BufReader::new(file);

            let flow = stepflow_workflow::Flow::from_yaml_reader(reader).unwrap();
            let compiled = compile(&plugins, flow).await.unwrap();
            insta::assert_yaml_snapshot!("compiled", compiled);

            // validate_flow(&compiled).unwrap();
        })
    })
}
