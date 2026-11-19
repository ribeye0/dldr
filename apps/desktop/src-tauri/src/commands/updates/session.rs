//! Short state transitions only. Network transfers never hold this mutex or the
//! library's mutation lock; cancellation also covers requests not yet admitted.
use super::{packages::PreparedPackage, PackageKind};
use std::sync::Arc;
use tauri_plugin_updater::Update;
use tokio::sync::{watch, Mutex};

#[derive(Default)]
pub(crate) struct PendingUpdate(pub(super) Mutex<Session>);

#[derive(Default)]
pub(super) struct Session {
    pub candidate: Option<Update>,
    pub prepared: Option<Arc<PreparedPackage>>,
    active: Option<Transfer>,
    cancelled_id: Option<String>,
    pub installing: bool,
}

struct Transfer {
    id: String,
    cancel: watch::Sender<bool>,
}

impl Session {
    pub fn begin(&mut self, id: &str) -> Result<watch::Receiver<bool>, String> {
        if self.cancelled_id.as_deref() == Some(id) {
            return Err("update_cancelled".into());
        }
        if self.active.is_some() || self.installing {
            return Err("update_busy".into());
        }
        let (cancel, receiver) = watch::channel(false);
        self.active = Some(Transfer {
            id: id.into(),
            cancel,
        });
        Ok(receiver)
    }

    pub fn cancel(&mut self, id: &str) {
        self.cancelled_id = Some(id.into());
        if let Some(active) = &self.active {
            if active.id == id {
                let _ = active.cancel.send(true);
            }
        }
        if !self.installing && self.prepared.as_ref().is_some_and(|p| p.transfer_id == id) {
            self.prepared = None;
        }
    }

    pub fn finish(&mut self, id: &str) -> Result<(), String> {
        let Some(active) = self.active.take() else {
            return Err("update_cancelled".into());
        };
        if active.id != id {
            self.active = Some(active);
            return Err("update_busy".into());
        }
        if *active.cancel.borrow() || self.cancelled_id.as_deref() == Some(id) {
            return Err("update_cancelled".into());
        }
        Ok(())
    }

    pub fn ready(
        &self,
        kind: PackageKind,
        trust_local: bool,
    ) -> Result<Arc<PreparedPackage>, String> {
        if self.installing || self.active.is_some() {
            return Err("update_busy".into());
        }
        let package = self.prepared.clone().ok_or("update_missing")?;
        if package.kind != kind {
            return Err("update_package".into());
        }
        if !package.info.verified && !trust_local {
            return Err("update_untrusted".into());
        }
        Ok(package)
    }
}

pub(super) async fn cancellable<T>(
    mut cancel: watch::Receiver<bool>,
    operation: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    if *cancel.borrow() {
        return Err("update_cancelled".into());
    }
    tokio::select! {
        biased;
        _ = cancel.changed() => Err("update_cancelled".into()),
        result = operation => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_interrupts_a_stalled_transfer_and_allows_retry() {
        let mut session = Session::default();
        let receiver = session.begin("first").unwrap();
        assert_eq!(session.begin("duplicate").unwrap_err(), "update_busy");
        session.cancel("first");
        let result = cancellable(receiver, std::future::pending::<Result<(), String>>()).await;
        assert_eq!(result.unwrap_err(), "update_cancelled");
        assert_eq!(session.finish("first").unwrap_err(), "update_cancelled");
        assert!(session.begin("retry").is_ok());
    }

    #[test]
    fn native_installation_requires_trust_and_preserves_the_ready_package() {
        let mut session = Session {
            prepared: Some(Arc::new(PreparedPackage {
                transfer_id: "local".into(),
                kind: PackageKind::AppImage,
                info: super::super::packages::info(
                    "0.1.4",
                    "0.1.2",
                    "test.AppImage".into(),
                    1,
                    "local",
                    false,
                    String::new(),
                )
                .unwrap(),
                bytes: vec![1],
                automatic: None,
            })),
            ..Session::default()
        };
        assert!(
            matches!(session.ready(PackageKind::AppImage, false), Err(e) if e == "update_untrusted")
        );
        assert!(session.ready(PackageKind::AppImage, true).is_ok());
        assert!(
            matches!(session.ready(PackageKind::WindowsPortable, true), Err(e) if e == "update_package")
        );
        session.installing = true;
        session.cancel("local");
        assert!(session.prepared.is_some());
        assert!(matches!(session.ready(PackageKind::AppImage, true), Err(e) if e == "update_busy"));
        session.installing = false;
        session.cancel("local");
        assert!(session.prepared.is_none());
    }

    #[test]
    fn early_and_late_cancellation_cannot_start_or_cancel_the_wrong_transfer() {
        let mut session = Session::default();
        session.cancel("early");
        assert_eq!(session.begin("early").unwrap_err(), "update_cancelled");
        let receiver = session.begin("new").unwrap();
        session.cancel("old");
        assert!(!*receiver.borrow());
        session.finish("new").unwrap();
    }
}
