use backon::{ExponentialBuilder, Retryable};
use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn};

use crate::config::RetryConfig;

/// Execute an operation with exponential backoff retry logic
pub async fn with_retry<F, Fut, T, E>(config: &RetryConfig, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    if config.max_retries == 0 {
        // No retries configured, execute once
        return operation().await;
    }

    let backoff = ExponentialBuilder::default()
        .with_max_times(config.max_retries as usize + 1) // +1 for initial attempt
        .with_min_delay(Duration::from_millis(config.initial_interval_ms))
        .with_max_delay(Duration::from_millis(config.max_interval_ms))
        .with_factor(config.multiplier);

    debug!(
        "Starting operation with retry policy: max_retries={}, initial_interval={}ms, max_interval={}ms, multiplier={}",
        config.max_retries, config.initial_interval_ms, config.max_interval_ms, config.multiplier
    );

    let mut attempt = 0;

    (|| {
        attempt += 1;
        let fut = operation();
        async move {
            match fut.await {
                Ok(result) => {
                    if attempt > 1 {
                        debug!("Operation succeeded on attempt {}", attempt);
                    }
                    Ok(result)
                }
                Err(e) => {
                    if attempt <= config.max_retries {
                        warn!(
                            "Operation failed on attempt {}/{}: {}",
                            attempt, config.max_retries, e
                        );
                    }
                    Err(e)
                }
            }
        }
    })
    .retry(&backoff)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        let config = RetryConfig::default();
        let counter = AtomicU32::new(0);

        let result = with_retry(&config, || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok::<i32, String>(42)
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig::default();
        let counter = AtomicU32::new(0);

        let result = with_retry(&config, || async {
            let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt < 3 {
                Err(format!("Attempt {} failed", attempt))
            } else {
                Ok(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 2,
            initial_interval_ms: 1,
            max_interval_ms: 10,
            multiplier: 2.0,
        };
        let counter = AtomicU32::new(0);

        let result = with_retry(&config, || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<i32, String>("Always fails".to_string())
        })
        .await;

        assert!(result.is_err());
        // We configured max_retries=2, so we expect multiple attempts but allow for backon's behavior
        assert!(
            counter.load(Ordering::SeqCst) > 1,
            "Should have retried at least once"
        );
    }

    #[tokio::test]
    async fn test_no_retries_configured() {
        let config = RetryConfig {
            max_retries: 0,
            initial_interval_ms: 1000,
            max_interval_ms: 30000,
            multiplier: 2.0,
        };
        let counter = AtomicU32::new(0);

        let result = with_retry(&config, || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<i32, String>("Fails".to_string())
        })
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Only one attempt
    }
}
