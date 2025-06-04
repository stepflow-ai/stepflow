use std::sync::Arc;

use crate::Result;
use stepflow_execution::StepFlowExecutor;

pub async fn serve(_executor: Arc<StepFlowExecutor>, _port: u16) -> Result<()> {
    todo!()
}
