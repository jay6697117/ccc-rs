//! Retry logic for transient API errors (429 / 529).
//! Mirrors TS `src/services/api/withRetry.ts`.

use std::time::Duration;
use tokio::time::sleep;

use crate::error::ApiError;

const DEFAULT_MAX_RETRIES: u32 = 10;
const BASE_DELAY_MS: u64 = 500;
const MAX_DELAY_MS: u64 = 60_000;
const MAX_529_RETRIES: u32 = 3;

/// Configuration for the retry loop.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum total attempts (1 = no retries).
    pub max_attempts: u32,
    /// Maximum consecutive 529 attempts.
    pub max_overloaded: u32,
    /// Base delay for exponential back-off.
    pub base_delay_ms: u64,
    /// Cap on back-off delay.
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_RETRIES,
            max_overloaded: MAX_529_RETRIES,
            base_delay_ms: BASE_DELAY_MS,
            max_delay_ms: MAX_DELAY_MS,
        }
    }
}

/// Compute the next back-off delay (exponential with jitter).
pub fn backoff_delay(attempt: u32, base_ms: u64, max_ms: u64) -> Duration {
    // 2^attempt * base, capped at max
    let exp = base_ms.saturating_mul(1u64 << attempt.min(62));
    let capped = exp.min(max_ms);
    Duration::from_millis(capped)
}

/// Execute `f` with retry on 429/529.
///
/// `f` receives the attempt index (0-based) and returns a future.
/// On a retryable error the loop sleeps with exponential back-off then
/// calls `f` again, up to `cfg.max_attempts` total attempts.
pub async fn with_retry<F, Fut, T>(cfg: &RetryConfig, mut f: F) -> Result<T, ApiError>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiError>>,
{
    let mut overloaded_count = 0u32;
    for attempt in 0..cfg.max_attempts {
        match f(attempt).await {
            Ok(v) => return Ok(v),
            Err(e) if attempt + 1 >= cfg.max_attempts => return Err(e),
            Err(ApiError::Overloaded) => {
                overloaded_count += 1;
                if overloaded_count >= cfg.max_overloaded {
                    return Err(ApiError::Overloaded);
                }
                let delay = backoff_delay(attempt, cfg.base_delay_ms, cfg.max_delay_ms);
                sleep(delay).await;
            }
            Err(ApiError::RateLimited { retry_after_secs }) => {
                let delay = retry_after_secs
                    .map(|s| Duration::from_secs(s))
                    .unwrap_or_else(|| backoff_delay(attempt, cfg.base_delay_ms, cfg.max_delay_ms));
                sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop always returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_exponentially() {
        let d0 = backoff_delay(0, 500, 60_000);
        let d1 = backoff_delay(1, 500, 60_000);
        let d2 = backoff_delay(2, 500, 60_000);
        assert_eq!(d0, Duration::from_millis(500));
        assert_eq!(d1, Duration::from_millis(1000));
        assert_eq!(d2, Duration::from_millis(2000));
    }

    #[test]
    fn backoff_is_capped() {
        let d = backoff_delay(100, 500, 60_000);
        assert_eq!(d, Duration::from_millis(60_000));
    }

    #[tokio::test]
    async fn retry_succeeds_on_third_attempt() {
        let cfg = RetryConfig {
            max_attempts: 5,
            max_overloaded: 5,
            base_delay_ms: 0, // no delay in tests
            max_delay_ms: 0,
        };
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let result = with_retry(&cfg, |_| {
            let cc = cc.clone();
            async move {
                let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 {
                    Err(ApiError::RateLimited { retry_after_secs: None })
                } else {
                    Ok(42u32)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_gives_up_after_max_overloaded() {
        let cfg = RetryConfig {
            max_attempts: 10,
            max_overloaded: 3,
            base_delay_ms: 0,
            max_delay_ms: 0,
        };
        let result = with_retry(&cfg, |_| async { Err::<(), _>(ApiError::Overloaded) }).await;
        assert!(matches!(result, Err(ApiError::Overloaded)));
    }
}
