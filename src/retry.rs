use std::time::Duration;
use tracing::{warn, error};

/// Retry configuration for SQLite operations
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff_delays: Vec<Duration>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_attempts: 3,
            backoff_delays: vec![
                Duration::from_millis(10),
                Duration::from_millis(20),
                Duration::from_millis(40),
            ],
        }
    }
}

/// Synchronous version of retry_with_backoff for use in blocking contexts
pub fn retry_with_backoff_sync<F, T>(
    mut f: F,
    config: RetryConfig,
) -> Result<T, rusqlite::Error>
where
    F: FnMut() -> Result<T, rusqlite::Error>,
{
    let mut attempt = 0;

    loop {
        match f() {
            Ok(result) => return Ok(result),
            Err(rusqlite::Error::SqliteFailure(err, _)) if err.code == rusqlite::ErrorCode::DatabaseBusy => {
                attempt += 1;
                if attempt >= config.max_attempts {
                    error!("SQLite retry exhausted after {} attempts", config.max_attempts);
                    return Err(rusqlite::Error::SqliteFailure(err, None));
                }

                let delay = config.backoff_delays.get(attempt as usize - 1)
                    .copied()
                    .unwrap_or(Duration::from_millis(40));

                warn!(
                    "SQLite BUSY error on attempt {}/{}, retrying after {:?}",
                    attempt, config.max_attempts, delay
                );

                std::thread::sleep(delay);
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_sync_succeeds_on_first_attempt() {
        let result = retry_with_backoff_sync(
            || Ok(42),
            RetryConfig::default(),
        );

        assert_eq!(result.unwrap(), 42);
    }
}
