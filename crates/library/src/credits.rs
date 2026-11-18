use super::*;

impl Library {
    /// Play sometimes omits all creator tags, even for multi-actor audio works.
    /// Supplement from the anonymous public product API without replacing purchases.
    pub async fn product_detail_with_metadata_source<S: LocalWorkMetadataSource + ?Sized>(
        &self,
        work_id: &str,
        source: &S,
    ) -> Result<ProductDetail> {
        let detail = self.product_detail(work_id).await?;
        if self.credits_checked_recently(&detail).await? {
            return Ok(detail);
        }
        // Separate from transfer locks: a long download must not block metadata.
        let lock = self
            .credit_locks
            .lock()
            .expect("credit lock registry")
            .entry(work_id.to_owned())
            .or_default()
            .clone();
        let _guard = lock.lock().await;
        let detail = self.product_detail(work_id).await?;
        if self.credits_checked_recently(&detail).await? {
            return Ok(detail);
        }

        let response = tokio::time::timeout(
            Duration::from_secs(8),
            source.works(&[WorkId::new(work_id)]),
        )
        .await;
        let works = match response {
            Ok(Ok(works)) => works,
            Ok(Err(error)) => {
                tracing::warn!(
                    failure_code = error.failure_code(),
                    "Public credit lookup failed; using cached metadata"
                );
                return Ok(detail);
            }
            Err(_) => {
                tracing::warn!("Public credit lookup timed out; using cached metadata");
                return Ok(detail);
            }
        };
        let tags = works
            .into_iter()
            .find(|work| work.id.as_ref() == work_id)
            .map(serde_json::to_value)
            .transpose()?
            .map(|value| {
                public_work_tags(&value)
                    .into_iter()
                    .filter(|tag| {
                        tag.get("class")
                            .and_then(Value::as_str)
                            .is_some_and(|class| class.ends_with("_by") || class == "series")
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.storage
            .cache_public_product_credits(work_id, &tags, &now_string())
            .await?;
        self.product_detail(work_id).await
    }

    async fn credits_checked_recently(&self, detail: &ProductDetail) -> Result<bool> {
        let checked_at = self
            .storage
            .product_credits_checked_at(&detail.work_id)
            .await?;
        Ok(checked_at
            .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
            .is_some_and(|time| {
                let age = Utc::now().signed_duration_since(time);
                age >= chrono::Duration::zero() && age < chrono::Duration::hours(24)
            }))
    }
}
