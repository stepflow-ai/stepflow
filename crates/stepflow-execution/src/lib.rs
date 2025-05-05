mod error;
mod state;

pub use error::{ExecutionError, Result};
use stepflow_workflow::{Value, Flow};
use stepflow_steps::Plugins;
pub use state::{State, VecState};

pub async fn execute(
    _plugins: &Plugins,
    _flow: Flow,
    _inputs: Vec<Value>,
    _state: impl State,
) -> Result<Vec<Value>> {
    Ok(vec![])
}
