use crate::Result;
use stepflow_workflow::Flow;
use url::Url;

pub async fn submit(
    _service: Url,
    _flow: Flow,
    _input: stepflow_workflow::Value,
) -> Result<stepflow_workflow::Value> {
    todo!()
}
