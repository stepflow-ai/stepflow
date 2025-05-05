use std::fs::File;
use std::io::BufReader;

use stepflow_compile::compile;
use stepflow_steps::Plugins;

#[test]
fn inputs_snapshot() {
    let plugins = Plugins::new();

    insta::glob!("inputs/*.yaml", |path| {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);

        let flow = stepflow_workflow::Flow::from_yaml_reader(reader).unwrap();
        let result = compile(&plugins, flow).map_err(|e| e.to_string());

        insta::assert_yaml_snapshot!(result)
    })
}