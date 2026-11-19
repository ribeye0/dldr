use crate::models::{AppSettings, AppSettingsPatch};
use crate::{
    LanguagePriority, Result, Storage, StorageError, UiLocale, DOWNLOAD_RATE_KEY,
    DOWNLOAD_ROOT_KEY, LANGUAGE_PRIORITY_KEY, LIBRARY_ROOT_KEY, TITLE_LANGUAGE_FOLLOWS_UI_KEY,
};
use sqlx::Row;

impl Storage {
    pub async fn app_settings(&self) -> Result<AppSettings> {
        let rows = sqlx::query("SELECT key, value FROM app_settings")
            .fetch_all(&self.pool)
            .await?;
        let mut settings = AppSettings::default();
        let mut stored_priority = None;
        let mut stored_follow = None;

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
                TITLE_LANGUAGE_FOLLOWS_UI_KEY => {
                    stored_follow = Some(parse_title_follow_mode(&value)?);
                }
                LANGUAGE_PRIORITY_KEY => {
                    stored_priority = Some(LanguagePriority::from_stored(&value)?)
                }
                _ => {}
            }
        }

        settings.title_language_follows_ui = stored_follow.unwrap_or_else(|| {
            infer_title_follow_mode(stored_priority.as_ref(), settings.ui_locale)
        });
        settings.language_priority = if settings.title_language_follows_ui {
            LanguagePriority::for_locale(settings.ui_locale)
        } else {
            stored_priority
                .unwrap_or_else(|| LanguagePriority::for_locale(settings.ui_locale))
                .for_settings(settings.ui_locale)
        };
        Ok(settings)
    }

    pub async fn save_app_settings_patch(&self, patch: &AppSettingsPatch) -> Result<AppSettings> {
        let mut transaction = self.begin_write().await?;
        let connection = transaction
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;
        let rows =
            sqlx::query("SELECT key, value FROM app_settings WHERE key IN (?, ?, 'ui_locale')")
                .bind(LANGUAGE_PRIORITY_KEY)
                .bind(TITLE_LANGUAGE_FOLLOWS_UI_KEY)
                .fetch_all(&mut **connection)
                .await?;
        let mut stored_priority = None;
        let mut stored_follow = None;
        let mut current_locale = UiLocale::default();
        for row in rows {
            let key: String = row.try_get("key")?;
            let value: String = row.try_get("value")?;
            match key.as_str() {
                LANGUAGE_PRIORITY_KEY
                    if patch.language_priority.is_none()
                        && patch.title_language_follows_ui != Some(true) =>
                {
                    stored_priority = Some(LanguagePriority::from_stored(&value)?);
                }
                TITLE_LANGUAGE_FOLLOWS_UI_KEY if patch.title_language_follows_ui.is_none() => {
                    stored_follow = Some(parse_title_follow_mode(&value)?);
                }
                "ui_locale" => current_locale = serde_json::from_str(&value).unwrap_or_default(),
                _ => {}
            }
        }
        // Older callers supplying an explicit order still mean a custom preference.
        // Infer old profiles before applying a new locale, then persist the mode.
        let follows_ui = patch.title_language_follows_ui.unwrap_or_else(|| {
            if patch.language_priority.is_some() {
                false
            } else {
                stored_follow.unwrap_or_else(|| {
                    infer_title_follow_mode(stored_priority.as_ref(), current_locale)
                })
            }
        });
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
        if follows_ui {
            transaction.set_setting(LANGUAGE_PRIORITY_KEY, None).await?;
        } else if let Some(priority) = &patch.language_priority {
            transaction
                .set_setting(LANGUAGE_PRIORITY_KEY, Some(&priority.to_json()))
                .await?;
        }
        if !follows_ui
            && patch.language_priority.is_none()
            && stored_priority
                .as_ref()
                .is_none_or(|priority| priority.is_legacy_default())
        {
            // Switching to custom without an order freezes the current effective preset.
            transaction
                .set_setting(
                    LANGUAGE_PRIORITY_KEY,
                    Some(&LanguagePriority::for_locale(current_locale).to_json()),
                )
                .await?;
        }
        transaction
            .set_setting(
                TITLE_LANGUAGE_FOLLOWS_UI_KEY,
                Some(if follows_ui { "true" } else { "false" }),
            )
            .await?;
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

fn parse_title_follow_mode(value: &str) -> Result<bool> {
    value.parse().map_err(|_| StorageError::InvalidStoredValue {
        field: TITLE_LANGUAGE_FOLLOWS_UI_KEY,
        value: value.to_owned(),
    })
}

fn infer_title_follow_mode(priority: Option<&LanguagePriority>, locale: UiLocale) -> bool {
    priority.is_none_or(|priority| {
        priority.is_legacy_default()
            || priority.clone().for_settings(locale) == LanguagePriority::for_locale(locale)
    })
}
