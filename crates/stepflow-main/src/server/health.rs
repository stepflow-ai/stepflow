use super::error::ServerResult;
use poem_openapi::{OpenApi, payload::Json};

pub struct HealthApi;

#[OpenApi]
impl HealthApi {
    /// Get server health status
    #[oai(path = "/health", method = "get")]
    pub async fn health(&self) -> ServerResult<Json<serde_json::Value>> {
        Ok(Json(serde_json::json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint_returns_healthy_status() {
        let api = HealthApi;
        let result = api.health().await;

        match result {
            Ok(json_payload) => {
                let value = json_payload.0;
                assert_eq!(value["status"], "healthy");
                assert!(value["timestamp"].is_string());

                // Verify timestamp is valid ISO 8601
                let timestamp = value["timestamp"].as_str().unwrap();
                assert!(chrono::DateTime::parse_from_rfc3339(timestamp).is_ok());
            }
            Err(_) => panic!("Expected Ok result from health endpoint"),
        }
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_current_timestamp() {
        let api = HealthApi;
        let before = chrono::Utc::now();
        let result = api.health().await;
        let after = chrono::Utc::now();

        if let Ok(json_payload) = result {
            let value = json_payload.0;
            let timestamp_str = value["timestamp"].as_str().unwrap();
            let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_str).unwrap();
            let timestamp_utc = timestamp.with_timezone(&chrono::Utc);

            // Timestamp should be between before and after
            assert!(timestamp_utc >= before);
            assert!(timestamp_utc <= after);
        } else {
            panic!("Expected Ok result from health endpoint");
        }
    }
}
