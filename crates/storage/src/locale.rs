//! Interface language, independent from work-title and creator name preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UiLocale {
    #[default]
    #[serde(rename = "zh-TW")]
    TraditionalChinese,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "en")]
    English,
}
