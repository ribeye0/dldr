use super::*;

impl Storage {
    pub async fn app_settings(&self) -> Result<AppSettings> {
        let rows = sqlx::query("SELECT key, value FROM app_settings")
            .fetch_all(&self.pool)
            .await?;
        let mut settings = AppSettings::default();
        let mut stored_priority = None;

        for row in rows {
            let key: String = row.try_get("key")?;
            let value: String = row.try_get("value")?;

            match key.as_str() {
                "auto_extract" => settings.auto_extract = value.parse().unwrap_or(true),
                LIBRARY_ROOT_KEY => settings.library_root = Some(value),
                DOWNLOAD_ROOT_KEY => settings.download_root = Some(value),
                "ui_locale" => {
                    settings.ui_locale = serde_json::from_str(&value).unwrap_or_default()
                }
                LANGUAGE_PRIORITY_KEY => {
                    stored_priority = Some(LanguagePriority::from_stored(&value)?)
                }
                _ => {}
            }
        }

        settings.language_priority = stored_priority
            .filter(|priority| !priority.is_legacy_default())
            .unwrap_or_else(|| LanguagePriority::for_locale(settings.ui_locale))
            .for_settings(settings.ui_locale);
        Ok(settings)
    }

    pub async fn save_app_settings_patch(&self, patch: &AppSettingsPatch) -> Result<AppSettings> {
        let mut transaction = self.begin_write().await?;
        let priority = if let Some(priority) = &patch.language_priority {
            priority.clone()
        } else {
            let connection = transaction
                .transaction
                .as_mut()
                .ok_or(StorageError::TransactionFinished)?;
            let stored: Option<String> =
                sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
                    .bind(LANGUAGE_PRIORITY_KEY)
                    .fetch_optional(&mut **connection)
                    .await?;
            stored
                .as_deref()
                .map(LanguagePriority::from_stored)
                .transpose()?
                .unwrap_or_default()
        };
        transaction
            .set_setting(LIBRARY_ROOT_KEY, patch.library_root.as_deref())
            .await?;
        transaction
            .set_setting(DOWNLOAD_ROOT_KEY, patch.download_root.as_deref())
            .await?;
        if let Some(auto_extract) = patch.auto_extract {
            transaction
                .set_setting(
                    "auto_extract",
                    Some(if auto_extract { "true" } else { "false" }),
                )
                .await?;
        }
        if patch.language_priority.is_some() {
            transaction
                .set_setting(LANGUAGE_PRIORITY_KEY, Some(&priority.to_json()))
                .await?;
        }
        if let Some(locale) = patch.ui_locale {
            transaction
                .set_setting(
                    "ui_locale",
                    Some(&serde_json::to_string(&locale).expect("locale serializes")),
                )
                .await?;
        }
        transaction.commit().await?;
        self.app_settings().await
    }

    pub async fn download_rate_limit(&self) -> Result<Option<u64>> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
                .bind(DOWNLOAD_RATE_KEY)
                .fetch_optional(&self.pool)
                .await?;
        value
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| StorageError::InvalidStoredValue {
                        field: DOWNLOAD_RATE_KEY,
                        value,
                    })
            })
            .transpose()
            .map(|value| value.filter(|rate| *rate > 0))
    }

    pub async fn save_download_rate_limit(&self, rate: Option<u64>) -> Result<()> {
        let value = rate.filter(|rate| *rate > 0).map(|rate| rate.to_string());
        let mut transaction = self.begin_write().await?;
        transaction
            .set_setting(DOWNLOAD_RATE_KEY, value.as_deref())
            .await?;
        transaction.commit().await
    }
}
