use stepflow_workflow::{Component, Step, Value};
use stepflow_steps::{ComponentInfo, Result, StepPlugin};

pub struct MockPlugin {
    kind: &'static str,
}

impl MockPlugin {
    fn new(kind: &'static str) -> Self {
        Self { kind }
    }
}

impl StepPlugin for MockPlugin {
    fn protocol(&self) -> &'static str {
        self.kind
    }

    fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        todo!()
    }

    fn execute(&self, component: &Component, args: Vec<Value>) -> Result<Vec<Value>> {
        todo!()
    }
}
