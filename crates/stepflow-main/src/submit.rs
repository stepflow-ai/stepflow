use crate::Result;
use stepflow_core::workflow::Flow;
use url::Url;

pub async fn submit(
    _service: Url,
    _flow: Flow,
    _input: stepflow_core::workflow::Value,
) -> Result<stepflow_core::workflow::Value> {
    todo!()
}
