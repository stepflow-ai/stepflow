// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

//! Orchestrator-level retry configuration.
//!
//! Controls how the orchestrator retries failed step executions.
//! The backoff strategy applies to all retries (transport and component errors).
//! Transport errors have a separate retry limit configured here; component error
//! limits are configured per-step via `onError: { action: retry, maxRetries }`.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{DefaultOnNull, serde_as};

/// Orchestrator-level retry configuration.
///
/// Controls backoff delays for all retries and the retry limit for transport
/// errors (subprocess crashes, network timeouts, connection refused).
///
/// Component error retry limits are configured per-step via
/// `onError: { action: retry, maxRetries }` and share this backoff config.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetryConfig {
    /// Maximum number of retries for transport errors (default: 3).
    ///
    /// Transport errors are infrastructure-level failures — subprocess crashes,
    /// network timeouts, connection refused — where the component never ran or
    /// didn't complete.
    #[serde(
        default = "RetryConfig::default_transport_max_retries",
        deserialize_with = "null_or_default_transport_max_retries"
    )]
    pub transport_max_retries: u32,
    /// Backoff strategy for all retry delays (default: fibonacci with 1s min, 10s max).
    ///
    /// This backoff applies to both transport error retries and component error
    /// retries. The same delay progression is used regardless of retry reason.
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub backoff: BackoffConfig,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            transport_max_retries: Self::default_transport_max_retries(),
            backoff: BackoffConfig::default(),
        }
    }
}

impl RetryConfig {
    fn default_transport_max_retries() -> u32 {
        3
    }

    /// Compute the backoff delay for a given retry attempt (1-based).
    pub fn delay(&self, retry_count: u32) -> Duration {
        self.backoff.delay(retry_count)
    }
}

/// Backoff strategy for retry delays.
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schemars(transform = stepflow_flow::discriminator_schema::AddDiscriminator::new("type"))]
pub enum BackoffConfig {
    /// Fixed delay between retries.
    #[serde(rename_all = "camelCase")]
    #[schemars(title = "BackoffConfigConstant")]
    Constant {
        /// Delay in milliseconds (default: 1000).
        #[serde(default = "BackoffConfig::default_min_delay_ms")]
        #[schemars(range(min = 0))]
        delay_ms: u64,
    },
    /// Exponential backoff (delay doubles each attempt by default).
    #[serde(rename_all = "camelCase")]
    #[schemars(title = "BackoffConfigExponential")]
    Exponential {
        /// Starting delay in milliseconds (default: 1000).
        #[serde(default = "BackoffConfig::default_min_delay_ms")]
        #[schemars(range(min = 0))]
        min_delay_ms: u64,
        /// Maximum delay cap in milliseconds (default: 10000).
        #[serde(default = "BackoffConfig::default_max_delay_ms")]
        #[schemars(range(min = 0))]
        max_delay_ms: u64,
        /// Multiplier per attempt (default: 2.0).
        #[serde(default = "BackoffConfig::default_factor")]
        factor: f32,
    },
    /// Fibonacci backoff (delay follows the Fibonacci sequence).
    #[serde(rename_all = "camelCase")]
    #[schemars(title = "BackoffConfigFibonacci")]
    Fibonacci {
        /// Starting delay in milliseconds (default: 1000).
        #[serde(default = "BackoffConfig::default_min_delay_ms")]
        #[schemars(range(min = 0))]
        min_delay_ms: u64,
        /// Maximum delay cap in milliseconds (default: 10000).
        #[serde(default = "BackoffConfig::default_max_delay_ms")]
        #[schemars(range(min = 0))]
        max_delay_ms: u64,
    },
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self::Fibonacci {
            min_delay_ms: Self::default_min_delay_ms(),
            max_delay_ms: Self::default_max_delay_ms(),
        }
    }
}

impl BackoffConfig {
    fn default_min_delay_ms() -> u64 {
        1000
    }

    fn default_max_delay_ms() -> u64 {
        10000
    }

    /// Default multiplier for Exponential backoff (used by serde when `factor` is omitted).
    fn default_factor() -> f32 {
        2.0
    }

    /// Compute the delay for a given retry attempt (1-based).
    pub fn delay(&self, retry_count: u32) -> Duration {
        let ms = match self {
            Self::Constant { delay_ms } => *delay_ms,
            Self::Exponential {
                min_delay_ms,
                max_delay_ms,
                factor,
            } => {
                let delay = *min_delay_ms as f64
                    * (*factor as f64).powi(retry_count.saturating_sub(1) as i32);
                (delay as u64).min(*max_delay_ms)
            }
            Self::Fibonacci {
                min_delay_ms,
                max_delay_ms,
            } => {
                let fib = fibonacci(retry_count.saturating_sub(1));
                (min_delay_ms.saturating_mul(fib)).min(*max_delay_ms)
            }
        };
        Duration::from_millis(ms)
    }
}

fn null_or_default_transport_max_retries<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RetryConfig::default_transport_max_retries()))
}

/// Compute the nth Fibonacci number (0-indexed: fib(0)=1, fib(1)=1, fib(2)=2, ...).
fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (1u64, 1u64);
    for _ in 0..n {
        let next = a.saturating_add(b);
        a = b;
        b = next;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fibonacci_sequence() {
        assert_eq!(fibonacci(0), 1);
        assert_eq!(fibonacci(1), 1);
        assert_eq!(fibonacci(2), 2);
        assert_eq!(fibonacci(3), 3);
        assert_eq!(fibonacci(4), 5);
        assert_eq!(fibonacci(5), 8);
    }

    #[test]
    fn test_default_backoff_delays() {
        let config = RetryConfig::default();
        // Fibonacci: 1s, 1s, 2s, 3s, 5s, 8s, 10s (capped)
        assert_eq!(config.delay(1), Duration::from_millis(1000));
        assert_eq!(config.delay(2), Duration::from_millis(1000));
        assert_eq!(config.delay(3), Duration::from_millis(2000));
        assert_eq!(config.delay(4), Duration::from_millis(3000));
        assert_eq!(config.delay(5), Duration::from_millis(5000));
        assert_eq!(config.delay(6), Duration::from_millis(8000));
        assert_eq!(config.delay(7), Duration::from_millis(10000)); // capped
    }

    #[test]
    fn test_constant_backoff() {
        let config = RetryConfig {
            transport_max_retries: 3,
            backoff: BackoffConfig::Constant { delay_ms: 500 },
        };
        assert_eq!(config.delay(1), Duration::from_millis(500));
        assert_eq!(config.delay(5), Duration::from_millis(500));
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig {
            transport_max_retries: 5,
            backoff: BackoffConfig::Exponential {
                min_delay_ms: 100,
                max_delay_ms: 5000,
                factor: 2.0,
            },
        };
        assert_eq!(config.delay(1), Duration::from_millis(100));
        assert_eq!(config.delay(2), Duration::from_millis(200));
        assert_eq!(config.delay(3), Duration::from_millis(400));
        assert_eq!(config.delay(4), Duration::from_millis(800));
        assert_eq!(config.delay(5), Duration::from_millis(1600));
        assert_eq!(config.delay(6), Duration::from_millis(3200));
        assert_eq!(config.delay(7), Duration::from_millis(5000)); // capped
    }

    #[test]
    fn test_default_config_serde_roundtrip() {
        let config = RetryConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.transport_max_retries, 3);
    }

    #[test]
    fn test_retry_config_null_fields_use_custom_defaults() {
        let json = serde_json::json!({
            "transportMaxRetries": null,
            "backoff": null,
        });
        let config: RetryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.transport_max_retries, 3);
        assert!(matches!(config.backoff, BackoffConfig::Fibonacci { .. }));
    }
}
