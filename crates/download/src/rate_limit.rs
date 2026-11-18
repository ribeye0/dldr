//! One shared payload budget for all transfers, including independent jobs.
use crate::{CancellationToken, DownloadError};
use std::{sync::Arc, time::Duration};
use tokio::sync::{watch, Mutex};
use tokio::time::Instant;

pub const MIN_DOWNLOAD_RATE: u64 = 1024;
pub const MAX_DOWNLOAD_RATE: u64 = 1024 * 1024 * 1024;

pub fn validate_download_rate(rate: Option<u64>) -> Result<(), &'static str> {
    if rate.is_some_and(|rate| !(MIN_DOWNLOAD_RATE..=MAX_DOWNLOAD_RATE).contains(&rate)) {
        return Err("Download limit must be between 1 KiB/s and 1024 MiB/s, or unlimited");
    }
    Ok(())
}

#[derive(Clone)]
pub struct DownloadRateLimiter {
    inner: Arc<Inner>,
}

struct Inner {
    rate: watch::Sender<Option<u64>>,
    // FIFO access avoids reserving future bandwidth for every queued transfer.
    gate: Mutex<Budget>,
}

struct Budget {
    next_at: Instant,
    last_grant: Option<Instant>,
    rate: Option<u64>,
}

impl Default for DownloadRateLimiter {
    fn default() -> Self {
        let (rate, _) = watch::channel(None);
        Self {
            inner: Arc::new(Inner {
                rate,
                gate: Mutex::new(Budget {
                    next_at: Instant::now(),
                    last_grant: None,
                    rate: None,
                }),
            }),
        }
    }
}

impl DownloadRateLimiter {
    pub fn limit(&self) -> Option<u64> {
        *self.inner.rate.borrow()
    }

    pub fn set_limit(&self, rate: Option<u64>) -> Result<(), &'static str> {
        validate_download_rate(rate)?;
        if self.limit() != rate {
            self.inner.rate.send_replace(rate);
        }
        Ok(())
    }

    pub async fn acquire(
        &self,
        bytes: usize,
        cancellation: &CancellationToken,
    ) -> Result<usize, DownloadError> {
        if cancellation.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let mut rate = self.inner.rate.subscribe();
        if bytes == 0 || rate.borrow().is_none() {
            return Ok(bytes);
        }
        let mut budget = tokio::select! {
            biased;
            () = cancellation.cancelled() => return Err(DownloadError::Cancelled),
            guard = self.inner.gate.lock() => guard,
        };
        loop {
            let current = *rate.borrow_and_update();
            let Some(current) = current else {
                budget.rate = None;
                return Ok(bytes);
            };
            let now = Instant::now();
            let window = Duration::from_millis(10);
            if budget.rate != Some(current)
                || budget
                    .last_grant
                    .is_none_or(|last| now.duration_since(last) > window)
            {
                budget.next_at = now;
            }
            budget.rate = Some(current);
            // Carry at most 10ms of timer overshoot so high limits aren't reduced
            // to one small buffer per millisecond by the timer's resolution.
            budget.next_at = budget.next_at.max(now - window);
            let grant = bytes.min((current / 10).clamp(1, 16 * 1024) as usize);
            let delay = Duration::from_secs_f64(grant as f64 / current as f64);
            let deadline = budget.next_at + delay;
            tokio::select! {
                biased;
                () = cancellation.cancelled() => return Err(DownloadError::Cancelled),
                _ = rate.changed() => { budget.rate = None; continue; },
                () = tokio::time::sleep_until(deadline) => {
                    budget.next_at = deadline;
                    budget.last_grant = Some(Instant::now());
                    return Ok(grant);
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{advance, Instant};

    #[tokio::test(start_paused = true)]
    async fn high_limits_are_not_bottlenecked_by_timer_resolution() {
        let limiter = DownloadRateLimiter::default();
        limiter.set_limit(Some(64 * 1024 * 1024)).unwrap();
        let token = CancellationToken::new();
        let now = Instant::now();
        for _ in 0..100 {
            assert_eq!(limiter.acquire(16 * 1024, &token).await.unwrap(), 16 * 1024);
        }
        assert!(
            now.elapsed()
                >= Duration::from_secs_f64(100.0 * 16.0 * 1024.0 / (64.0 * 1024.0 * 1024.0))
        );
        assert!(now.elapsed() <= Duration::from_millis(30));
    }

    #[tokio::test(start_paused = true)]
    async fn lowering_the_limit_resizes_an_already_waiting_grant() {
        let limiter = DownloadRateLimiter::default();
        limiter.set_limit(Some(64 * 1024 * 1024)).unwrap();
        let copy = limiter.clone();
        let transfer =
            tokio::spawn(async move { copy.acquire(1_000_000, &CancellationToken::new()).await });
        tokio::task::yield_now().await;
        limiter.set_limit(Some(1024)).unwrap();
        let started = Instant::now();
        assert_eq!(transfer.await.unwrap().unwrap(), 102);
        assert!(started.elapsed() <= Duration::from_millis(101));
    }

    #[tokio::test(start_paused = true)]
    async fn cloned_limiters_share_one_budget_without_idle_credit() {
        let limiter = DownloadRateLimiter::default();
        limiter.set_limit(Some(10240)).unwrap();
        advance(Duration::from_secs(100)).await;
        let second = limiter.clone();
        let token = CancellationToken::new();
        let started = Instant::now();
        let (a, b) = tokio::join!(limiter.acquire(512, &token), second.acquire(512, &token));
        a.unwrap();
        b.unwrap();
        assert_eq!(started.elapsed(), Duration::from_millis(100));
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_or_changing_the_limit_wakes_a_throttled_transfer() {
        let limiter = DownloadRateLimiter::default();
        limiter.set_limit(Some(10240)).unwrap();
        let copy = limiter.clone();
        let started = Instant::now();
        let transfer =
            tokio::spawn(async move { copy.acquire(1024, &CancellationToken::new()).await });
        tokio::task::yield_now().await;
        advance(Duration::from_millis(10)).await;
        limiter.set_limit(Some(20480)).unwrap();
        transfer.await.unwrap().unwrap();
        assert_eq!(started.elapsed(), Duration::from_millis(60));
        let copy = limiter.clone();
        let transfer =
            tokio::spawn(async move { copy.acquire(1_000_000, &CancellationToken::new()).await });
        tokio::task::yield_now().await;
        limiter.set_limit(None).unwrap();
        let before = Instant::now();
        transfer.await.unwrap().unwrap();
        assert_eq!(before.elapsed(), Duration::ZERO);
        limiter.set_limit(Some(10240)).unwrap();
        limiter
            .acquire(1024, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(before.elapsed(), Duration::from_millis(100));
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_interrupts_both_the_budget_owner_and_queued_consumers() {
        let limiter = DownloadRateLimiter::default();
        limiter.set_limit(Some(10240)).unwrap();
        let token = CancellationToken::new();
        let copy = limiter.clone();
        let t = token.clone();
        let owner = tokio::spawn(async move { copy.acquire(1_000_000, &t).await });
        tokio::task::yield_now().await;
        let copy = limiter.clone();
        let t = token.clone();
        let queued = tokio::spawn(async move { copy.acquire(1024, &t).await });
        tokio::task::yield_now().await;
        token.cancel();
        assert!(matches!(
            owner.await.unwrap(),
            Err(DownloadError::Cancelled)
        ));
        assert!(matches!(
            queued.await.unwrap(),
            Err(DownloadError::Cancelled)
        ));
        let now = Instant::now();
        limiter
            .acquire(1024, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(now.elapsed(), Duration::from_millis(100));
    }
}
