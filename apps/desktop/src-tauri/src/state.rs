use crate::two_factor::TwoFactorPrompts;
use dldr_audit::AuditLogger;
use dldr_jobs::JobManager;
use dldr_library::Library;
use dldr_storage::Storage;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

pub(crate) struct AppState {
    pub(crate) mutation_lock: tokio::sync::RwLock<()>,
    pub(crate) queue_lock: tokio::sync::Mutex<()>,
    pub(crate) download_rate_limiter: dldr_download::DownloadRateLimiter,
    pub(crate) download_rate_settings_lock: tokio::sync::Mutex<()>,
    pub(crate) storage: Storage,
    pub(crate) library: Library,
    pub(crate) jobs: JobManager,
    pub(crate) audit: AuditLogger,
    pub(crate) download_reservations: DownloadReservations,
    pub(crate) two_factor_prompts: TwoFactorPrompts,
    pub(crate) _instance: dldr_library::LibraryInstanceGuard,
}

pub(crate) const WORK_DOWNLOAD_PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const BULK_DOWNLOAD_PAGE_LIMIT: u32 = 500;
pub(crate) const DOWNLOAD_RESERVATION_METADATA_KEY: &str = "downloadReservationId";

#[derive(Clone, Default)]
pub(crate) struct DownloadReservations {
    pub(crate) inner: Arc<Mutex<DownloadReservationsInner>>,
}

#[derive(Default)]
pub(crate) struct DownloadReservationsInner {
    pub(crate) next_owner: u64,
    pub(crate) by_work_id: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DownloadReservationSplit {
    pub(crate) available: Vec<String>,
    pub(crate) reserved: Vec<String>,
}

impl DownloadReservations {
    pub(crate) fn next_owner_id(&self) -> String {
        let mut inner = self.inner.lock().expect("download reservations lock");

        inner.next_owner = inner.next_owner.saturating_add(1);
        format!("download-reservation-{}", inner.next_owner)
    }

    pub(crate) fn split_available(&self, work_ids: &[String]) -> DownloadReservationSplit {
        let inner = self.inner.lock().expect("download reservations lock");
        let mut seen = BTreeSet::new();
        let mut available = Vec::new();
        let mut reserved = Vec::new();

        for work_id in work_ids {
            if !seen.insert(work_id.as_str()) {
                continue;
            }

            if inner.by_work_id.contains_key(work_id) {
                reserved.push(work_id.clone());
            } else {
                available.push(work_id.clone());
            }
        }

        DownloadReservationSplit {
            available,
            reserved,
        }
    }

    pub(crate) fn claim_available(
        &self,
        work_ids: &[String],
        owner_id: &str,
    ) -> DownloadReservationSplit {
        let mut inner = self.inner.lock().expect("download reservations lock");
        let mut seen = BTreeSet::new();
        let mut available = Vec::new();
        let mut reserved = Vec::new();

        for work_id in work_ids {
            if !seen.insert(work_id.as_str()) {
                continue;
            }

            if inner.by_work_id.contains_key(work_id) {
                reserved.push(work_id.clone());
            } else {
                inner
                    .by_work_id
                    .insert(work_id.clone(), owner_id.to_owned());
                available.push(work_id.clone());
            }
        }

        DownloadReservationSplit {
            available,
            reserved,
        }
    }

    pub(crate) fn release_owner(&self, owner_id: &str) -> usize {
        let mut inner = self.inner.lock().expect("download reservations lock");
        let before = inner.by_work_id.len();

        inner
            .by_work_id
            .retain(|_, reservation_owner| reservation_owner != owner_id);

        before.saturating_sub(inner.by_work_id.len())
    }

    #[cfg(test)]
    pub(crate) fn is_reserved(&self, work_id: &str) -> bool {
        self.inner
            .lock()
            .expect("download reservations lock")
            .by_work_id
            .contains_key(work_id)
    }
}
