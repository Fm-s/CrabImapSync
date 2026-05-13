use crate::error::{Error, Result};
use std::time::Duration;

/// Run `op` up to `retries + 1` times, each call wrapped in a `timeout`.
/// Exponential back-off (1 s, 2 s, 4 s, … capped at 64 s) is applied between
/// attempts when the inner future returns an error.
///
/// Timeout-elapsed attempts are retried without back-off (the outer loop simply
/// increments the attempt counter and tries again immediately).
pub async fn with_retry<F, Fut, T>(
    retries: u32,
    timeout: Duration,
    op_name: &str,
    mut op: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt: u32 = 0;
    loop {
        match tokio::time::timeout(timeout, op()).await {
            Ok(Ok(v)) => return Ok(v),
            Ok(Err(e)) => {
                if attempt >= retries {
                    return Err(e);
                }
                let backoff = Duration::from_secs(1u64 << attempt.min(6));
                tracing::warn!(op = op_name, attempt, error = %e, "retrying after error");
                tokio::time::sleep(backoff).await;
                attempt += 1;
            }
            Err(_elapsed) => {
                if attempt >= retries {
                    return Err(Error::Network(format!(
                        "operation '{op_name}' timed out after {} attempts",
                        retries + 1
                    )));
                }
                tracing::warn!(
                    op = op_name,
                    attempt,
                    timeout_secs = timeout.as_secs(),
                    "operation timed out, retrying"
                );
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn succeeds_on_first_try() {
        let result = with_retry(3, Duration::from_secs(5), "test-ok", || async {
            Ok::<i32, Error>(42)
        })
        .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn returns_error_after_retries_exhausted() {
        let mut calls = 0u32;
        let result = with_retry(2, Duration::from_secs(5), "test-fail", || {
            calls += 1;
            async { Err::<i32, Error>(Error::Network("boom".into())) }
        })
        .await;
        // 1 initial attempt + 2 retries = 3 total
        assert_eq!(calls, 3);
        assert!(result.is_err());
    }
}
