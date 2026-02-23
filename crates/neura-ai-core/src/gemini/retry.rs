use std::time::Duration;
use tracing::warn;

/// Exponential backoff retry for API calls.
pub async fn with_retry<F, Fut, T, E>(
    max_retries: u32,
    base_delay_ms: u64,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = base_delay_ms * 2u64.pow(attempt - 1);
                warn!("Retry {}/{} after {}ms: {}", attempt, max_retries, delay, e);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }
    }
}
