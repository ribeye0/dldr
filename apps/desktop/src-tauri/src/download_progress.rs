//! Desktop event bridge; counters and rates live in dldr-library.
use crate::progress::WorkDownloadProgressThrottle;
use dldr_jobs::{JobContext, JobMetadata, JobProgress};
use dldr_library::{
    download_progress::{DownloadProgressTracker, DownloadWorkPlan},
    WorkDownloadProgress,
};
use std::{collections::BTreeMap, sync::Mutex};

pub(super) struct DownloadJobProgress {
    state: Mutex<State>,
    bulk: bool,
}

struct State {
    tracker: DownloadProgressTracker,
    throttles: BTreeMap<String, WorkDownloadProgressThrottle>,
}

impl DownloadJobProgress {
    pub fn new(plans: Vec<DownloadWorkPlan>, bulk: bool) -> Self {
        Self {
            bulk,
            state: Mutex::new(State {
                tracker: DownloadProgressTracker::new(plans),
                throttles: BTreeMap::new(),
            }),
        }
    }

    pub fn reset(&self, context: &JobContext, plans: Vec<DownloadWorkPlan>) {
        let mut state = self.state.lock().expect("download metrics lock");
        state.tracker = DownloadProgressTracker::new(plans);
        state.throttles.clear();
        self.publish(context, &state);
    }

    pub fn start(&self, context: &JobContext, work_id: &str) {
        let mut state = self.state.lock().expect("download metrics lock");
        state.tracker.start(work_id);
        self.publish(context, &state);
    }

    pub fn observe(&self, context: &JobContext, work_id: &str, progress: &WorkDownloadProgress) {
        if matches!(progress, WorkDownloadProgress::Warning(_)) {
            return;
        }
        let mut state = self.state.lock().expect("download metrics lock");
        state.tracker.observe(work_id, progress);
        let throttle = state.throttles.entry(work_id.to_owned()).or_default();
        if let WorkDownloadProgress::Download(p) = progress {
            if p.phase == dldr_download::DownloadPhase::ProbingMetadata
                || throttle.emit_decision(p.phase).is_none()
            {
                return;
            }
        } else {
            throttle.reset();
        }
        self.publish(context, &state);
    }

    pub fn finish(&self, context: &JobContext, work_id: &str, success: bool) {
        let mut state = self.state.lock().expect("download metrics lock");
        state.tracker.finish(work_id, success);
        state.throttles.remove(work_id);
        self.publish(context, &state);
    }

    pub fn defer(&self, context: &JobContext, work_id: &str) {
        let mut state = self.state.lock().expect("download progress");
        state.tracker.defer(work_id);
        state.throttles.remove(work_id);
        self.publish(context, &state);
    }

    fn publish(&self, context: &JobContext, state: &State) {
        let metrics = state.tracker.snapshot();
        let active = metrics["activeWorks"]
            .as_array()
            .and_then(|works| works.first());
        let progress = match (self.bulk, active) {
            (false, Some(work)) => {
                JobProgress::bytes(work["bytesReceived"].as_u64(), work["bytesTotal"].as_u64())
            }
            _ => JobProgress::items(
                metrics["completedWorks"].as_u64(),
                metrics["totalWorks"].as_u64(),
            ),
        };
        let mut details = JobMetadata::new();
        details.insert("download".into(), metrics);
        context.set_progress_with_details(progress, details);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dldr_jobs::JobManager;

    #[tokio::test]
    async fn bulk_bridge_retains_work_bytes_and_outcomes_in_final_snapshot() {
        let jobs = JobManager::default();
        let observer = jobs.clone();
        let id = jobs.spawn(
            "bulkWorkDownload",
            "test",
            JobMetadata::new(),
            move |context| async move {
                let sink = crate::progress::JobBulkWorkDownloadProgressSink {
                    context,
                    metrics: DownloadJobProgress::new(Vec::new(), true),
                };
                use dldr_library::{BulkWorkDownloadProgress as P, BulkWorkDownloadProgressSink};
                sink.emit(P::Planned {
                    works: vec![DownloadWorkPlan {
                        work_id: "RJ1".into(),
                        title: "Example".into(),
                        bytes_total: Some(100),
                    }],
                });
                sink.emit(P::WorkStarted {
                    work_id: "RJ1".into(),
                    current: 1,
                    total: 1,
                });
                sink.emit(P::WorkProgress {
                    work_id: "RJ1".into(),
                    progress: WorkDownloadProgress::Download(dldr_download::DownloadProgress {
                        phase: dldr_download::DownloadPhase::Downloading,
                        file_index: Some(0),
                        file_kind: None,
                        bytes_received: 50,
                        bytes_total: Some(100),
                    }),
                });
                let in_flight = observer.get_job(sink.context.job_id()).unwrap();
                assert_eq!(
                    in_flight.progress,
                    Some(JobProgress::items(Some(0), Some(1)))
                );
                assert_eq!(
                    in_flight.progress_details["download"]["activeWorks"][0]["bytesReceived"],
                    50
                );
                sink.emit(P::WorkCompleted {
                    work_id: "RJ1".into(),
                    current: 1,
                    total: 1,
                });
                sink.context.set_phase("completed");
                Ok(JobMetadata::new())
            },
        );
        let snapshot = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let snapshot = jobs.get_job(&id).unwrap();
                if snapshot.status.is_terminal() {
                    break snapshot;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            snapshot.progress,
            Some(JobProgress::items(Some(1), Some(1)))
        );
        assert_eq!(snapshot.progress_details["download"]["bytesReceived"], 50);
        assert_eq!(snapshot.progress_details["download"]["succeededWorks"], 1);
        assert_eq!(
            snapshot.progress_details["download"]["activeWorks"],
            serde_json::json!([])
        );
    }
}
