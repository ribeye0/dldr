//! Download counters and rolling transfer rates, independent of the desktop UI.
use crate::WorkDownloadProgress;
use dldr_download::DownloadPhase;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadWorkPlan {
    pub work_id: String,
    pub title: String,
    /// Cached catalog size; replaced by the transfer's actual total when available.
    pub bytes_total: Option<u64>,
}

pub struct DownloadProgressTracker {
    started: Instant,
    works: BTreeMap<String, WorkProgress>,
}

struct WorkProgress {
    plan: DownloadWorkPlan,
    phase: &'static str,
    bytes: u64,
    total: Option<u64>,
    active: bool,
    finished: Option<bool>,
    deferred: bool,
    samples: VecDeque<(Duration, u64)>,
    last_observed: Option<Duration>,
    last_file_index: Option<usize>,
    rate: Option<u64>,
    rate_updated_at: i64,
}

impl DownloadProgressTracker {
    pub fn new(plans: Vec<DownloadWorkPlan>) -> Self {
        Self {
            started: Instant::now(),
            works: plans
                .into_iter()
                .map(|plan| {
                    (
                        plan.work_id.clone(),
                        WorkProgress {
                            total: plan.bytes_total,
                            plan,
                            phase: "preparing",
                            bytes: 0,
                            active: false,
                            finished: None,
                            deferred: false,
                            samples: VecDeque::new(),
                            last_observed: None,
                            last_file_index: None,
                            rate: None,
                            rate_updated_at: 0,
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn start(&mut self, work_id: &str) {
        if let Some(work) = self.works.get_mut(work_id) {
            if work.finished.is_none() {
                work.active = true;
            }
        }
    }

    pub fn observe(&mut self, work_id: &str, progress: &WorkDownloadProgress) {
        self.observe_at(
            work_id,
            progress,
            self.started.elapsed(),
            chrono::Utc::now().timestamp_millis(),
        );
    }

    fn observe_at(
        &mut self,
        work_id: &str,
        progress: &WorkDownloadProgress,
        now: Duration,
        wall_ms: i64,
    ) {
        let Some(work) = self.works.get_mut(work_id) else {
            return;
        };
        if work.finished.is_some() || matches!(progress, WorkDownloadProgress::Warning(_)) {
            return;
        }
        work.active = true;
        let phase = match progress {
            WorkDownloadProgress::LoggingIn => "loggingIn",
            WorkDownloadProgress::ResolvingPlan => "resolvingDownload",
            WorkDownloadProgress::Download(p) => match p.phase {
                // Metadata probing emits zero counters per file; these aren't transfer counters.
                DownloadPhase::ProbingMetadata => return,
                DownloadPhase::ResolvingPlan => "resolvingDownload",
                DownloadPhase::Downloading => "downloading",
                DownloadPhase::Unpacking => "unpacking",
                DownloadPhase::Finalizing => "finalizing",
            },
            WorkDownloadProgress::Finalizing => "finalizing",
            WorkDownloadProgress::Completed => "completed",
            WorkDownloadProgress::Warning(_) => unreachable!(),
        };
        if let WorkDownloadProgress::Download(p) = progress {
            work.bytes = p.bytes_received;
            work.total = p.bytes_total;
        }
        if phase == "downloading" {
            let file_index = match progress {
                WorkDownloadProgress::Download(p) => p.file_index,
                _ => None,
            };
            let reset = work.phase != phase
                || file_index != work.last_file_index
                || work
                    .last_observed
                    .is_none_or(|last| now.saturating_sub(last) > Duration::from_secs(3))
                || work
                    .samples
                    .back()
                    .is_some_and(|(_, bytes)| work.bytes < *bytes);
            if reset {
                work.samples.clear();
                work.rate = None;
            }
            work.last_observed = Some(now);
            work.last_file_index = file_index;
            work.rate_updated_at = wall_ms.max(0);
            // Keep a short rolling window without allocating one sample per network chunk.
            if work
                .samples
                .back()
                .is_none_or(|(at, _)| now.saturating_sub(*at) >= Duration::from_millis(250))
            {
                work.samples.push_back((now, work.bytes));
            }
            while work.samples.len() > 2
                && work
                    .samples
                    .get(1)
                    .is_some_and(|(at, _)| now.saturating_sub(*at) >= Duration::from_secs(2))
            {
                work.samples.pop_front();
            }
            if let Some((at, bytes)) = work.samples.front() {
                let elapsed = now.saturating_sub(*at).as_secs_f64();
                if elapsed >= 0.25 {
                    work.rate = Some((work.bytes.saturating_sub(*bytes) as f64 / elapsed) as u64);
                }
            }
        } else {
            work.rate = Some(0);
            work.samples.clear();
            work.last_observed = None;
        }
        work.phase = phase;
        if matches!(progress, WorkDownloadProgress::Completed) {
            self.finish(work_id, true);
        }
    }

    pub fn finish(&mut self, work_id: &str, succeeded: bool) {
        if let Some(work) = self.works.get_mut(work_id) {
            if work.finished.is_none() {
                work.finished = Some(succeeded);
                work.active = false;
                work.rate = Some(0);
            }
        }
    }

    pub fn defer(&mut self, work_id: &str) {
        self.finish(work_id, false);
        if let Some(work) = self.works.get_mut(work_id) {
            work.deferred = true;
        }
    }

    pub fn snapshot(&self) -> Value {
        let succeeded = self
            .works
            .values()
            .filter(|w| w.finished == Some(true))
            .count();
        let failed = self
            .works
            .values()
            .filter(|w| w.finished == Some(false) && !w.deferred)
            .count();
        let deferred = self.works.values().filter(|w| w.deferred).count();
        let bytes = self
            .works
            .values()
            .fold(0u64, |sum, work| sum.saturating_add(work.bytes));
        let active: Vec<Value> = self
            .works
            .values()
            .filter(|w| w.active && w.finished.is_none())
            .map(|w| {
                json!({
                    "workId": w.plan.work_id, "title": w.plan.title, "phase": w.phase,
                    "bytesReceived": w.bytes, "bytesTotal": w.total,
                    "bytesPerSecond": w.rate, "rateUpdatedAt": w.rate_updated_at,
                })
            })
            .collect();
        json!({"totalWorks": self.works.len(), "completedWorks": succeeded + failed + deferred,
            "deferredWorks": deferred,
            "succeededWorks": succeeded, "failedWorks": failed, "bytesReceived": bytes,
            "activeWorks": active})
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn deferred_works_are_processed_without_being_failures() {
        let mut tracker = super::DownloadProgressTracker::new(vec![super::DownloadWorkPlan {
            work_id: "RJ1".into(),
            title: "Work".into(),
            bytes_total: None,
        }]);
        tracker.start("RJ1");
        tracker.defer("RJ1");
        let value = tracker.snapshot();
        assert_eq!(value["completedWorks"], 1);
        assert_eq!(value["deferredWorks"], 1);
        assert_eq!(value["failedWorks"], 0);
        assert_eq!(value["activeWorks"], serde_json::json!([]));
    }

    use super::*;
    fn tracker() -> DownloadProgressTracker {
        DownloadProgressTracker::new(
            ["RJ1", "RJ2"]
                .map(|id| DownloadWorkPlan {
                    work_id: id.into(),
                    title: id.into(),
                    bytes_total: None,
                })
                .to_vec(),
        )
    }
    fn download(bytes: u64) -> WorkDownloadProgress {
        WorkDownloadProgress::Download(dldr_download::DownloadProgress {
            phase: DownloadPhase::Downloading,
            file_index: Some(0),
            file_kind: None,
            bytes_received: bytes,
            bytes_total: None,
        })
    }
    #[test]
    fn rates_use_deltas_excluding_resumed_prefix_and_reset_after_stalls_or_retries() {
        let mut tracker = tracker();
        tracker.observe_at("RJ1", &download(1_000_000), Duration::ZERO, 1000);
        assert!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"].is_null());
        tracker.observe_at("RJ1", &download(1_001_000), Duration::from_secs(1), 2000);
        assert_eq!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"], 1000);
        tracker.observe_at("RJ1", &download(1_002_000), Duration::from_secs(5), 6000);
        assert!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"].is_null());
        tracker.observe_at("RJ1", &download(0), Duration::from_secs(6), 7000);
        assert!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"].is_null());
        tracker.observe_at("RJ1", &download(0), Duration::from_secs(7), 8000);
        assert_eq!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"], 0);
    }
    #[test]
    fn new_files_exclude_their_own_resumed_prefix_and_metadata_does_not_reset_counters() {
        let mut tracker = tracker();
        tracker.observe_at("RJ1", &download(100), Duration::ZERO, 1000);
        tracker.observe_at("RJ1", &download(200), Duration::from_secs(1), 2000);
        let mut second_file = match download(1_000_200) {
            WorkDownloadProgress::Download(p) => p,
            _ => unreachable!(),
        };
        second_file.file_index = Some(1);
        tracker.observe_at(
            "RJ1",
            &WorkDownloadProgress::Download(second_file.clone()),
            Duration::from_secs(2),
            3000,
        );
        assert!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"].is_null());
        second_file.bytes_received += 100;
        tracker.observe_at(
            "RJ1",
            &WorkDownloadProgress::Download(second_file),
            Duration::from_secs(3),
            4000,
        );
        assert_eq!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"], 100);
        let probing = WorkDownloadProgress::Download(dldr_download::DownloadProgress {
            phase: DownloadPhase::ProbingMetadata,
            file_index: Some(1),
            file_kind: None,
            bytes_received: 0,
            bytes_total: None,
        });
        tracker.observe_at("RJ1", &probing, Duration::from_secs(3), 4000);
        assert_eq!(tracker.snapshot()["bytesReceived"], 1_000_300);
    }

    #[test]
    fn concurrent_work_counters_and_completion_are_independent_and_idempotent() {
        let mut tracker = tracker();
        tracker.observe_at("RJ1", &download(10), Duration::ZERO, 1000);
        tracker.observe_at("RJ2", &download(20), Duration::ZERO, 1000);
        assert_eq!(
            tracker.snapshot()["activeWorks"].as_array().unwrap().len(),
            2
        );
        assert_eq!(tracker.snapshot()["bytesReceived"], 30);
        tracker.observe_at(
            "RJ1",
            &WorkDownloadProgress::Finalizing,
            Duration::from_secs(1),
            2000,
        );
        assert_eq!(tracker.snapshot()["activeWorks"][0]["bytesPerSecond"], 0);
        tracker.observe("RJ1", &WorkDownloadProgress::Completed);
        tracker.finish("RJ1", true);
        tracker.finish("RJ2", false);
        let snapshot = tracker.snapshot();
        assert_eq!(snapshot["completedWorks"], 2);
        assert_eq!(snapshot["succeededWorks"], 1);
        assert_eq!(snapshot["failedWorks"], 1);
        assert!(snapshot["activeWorks"].as_array().unwrap().is_empty());
    }
}
