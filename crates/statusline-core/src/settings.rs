use crate::browser::Browser;
use crate::format::Percentage;
use crate::segment::SegmentConfig;
use crate::util::app_data_dir;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
	#[error("reading settings: {0}")]
	Io(#[from] std::io::Error),
	#[error("parsing settings: {0}")]
	Parse(#[from] serde_json::Error),
	#[error("no home/data directory")]
	NoDataDir,
}

pub const DEFAULT_FIVE_HOUR_RESET: f64 = 70.0;
pub const DEFAULT_SEVEN_DAY_RESET: f64 = 100.0;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
	pub five_hour_reset_threshold: Percentage,
	pub seven_day_reset_threshold: Percentage,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub segments: Option<Vec<SegmentConfig>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub divider: Option<String>,
	#[serde(default)]
	pub nerd_font: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub browser: Option<Browser>,
	#[serde(default)]
	pub skip_update_check: bool,
	#[serde(flatten)]
	pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Settings {
	pub fn load() -> Result<Self, SettingsError> {
		Self::load_from(&Self::settings_path()?)
	}

	pub fn load_from(path: &std::path::Path) -> Result<Self, SettingsError> {
		let content = std::fs::read_to_string(path)?;

		Ok(serde_json::from_str(&content)?)
	}

	pub fn ensure(
		five_hour_reset_threshold: Percentage,
		seven_day_reset_threshold: Percentage,
	) -> Result<Self, SettingsError> {
		let settings = Self {
			five_hour_reset_threshold,
			seven_day_reset_threshold,
			segments: None,
			divider: None,
			nerd_font: false,
			browser: None,
			skip_update_check: false,
			extra: Default::default(),
		};

		let path = Self::settings_path()?;
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)?;
		}

		let content = serde_json::to_string_pretty(&settings)?;
		std::fs::write(&path, content)?;

		Ok(settings)
	}

	pub fn settings_path() -> Result<std::path::PathBuf, SettingsError> {
		Ok(app_data_dir()?.join("settings.json"))
	}

	pub fn save(&self, path: &std::path::Path) -> Result<(), SettingsError> {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		let json = serde_json::to_string_pretty(self)?;
		let tmp = path.with_extension("json.tmp");
		std::fs::write(&tmp, json)?;
		std::fs::rename(&tmp, path)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn settings_roundtrip_serde() {
		let settings = Settings {
			five_hour_reset_threshold: 70.0.into(),
			seven_day_reset_threshold: 100.0.into(),
			segments: None,
			divider: None,
			nerd_font: false,
			browser: None,
			skip_update_check: false,
			extra: Default::default(),
		};

		let json = serde_json::to_string(&settings).unwrap();
		let loaded: Settings = serde_json::from_str(&json).unwrap();

		assert_eq!(loaded.five_hour_reset_threshold, 70.0.into());
		assert_eq!(loaded.seven_day_reset_threshold, 100.0.into());
		assert!(!loaded.skip_update_check);
	}

	#[test]
	fn settings_deserializes_pretty_json() {
		let json = r#"{
			"five_hour_reset_threshold": 50,
			"seven_day_reset_threshold": 80
		}"#;

		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert_eq!(loaded.five_hour_reset_threshold, 50.0.into());
		assert_eq!(loaded.seven_day_reset_threshold, 80.0.into());
	}

	#[test]
	fn settings_ignores_legacy_org_id() {
		let json = r#"{
			"org_id": "legacy-value",
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert_eq!(loaded.five_hour_reset_threshold, 70.0.into());
	}

	#[test]
	fn settings_rejects_missing_required_fields() {
		let json = r#"{}"#;
		let err = serde_json::from_str::<Settings>(json).unwrap_err();
		let msg = err.to_string();
		assert!(
			msg.contains("five_hour_reset_threshold"),
			"error should mention the missing field, got: {msg}"
		);
	}

	#[test]
	fn settings_backward_compat_no_segments() {
		let json = r#"{
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert!(loaded.segments.is_none());
		assert!(loaded.divider.is_none());
	}

	#[test]
	fn settings_with_segments() {
		let json = r#"{
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100,
			"segments": ["context_percentage", "divider", "model"],
			"divider": "|"
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert_eq!(loaded.segments.as_ref().unwrap().len(), 3);
		assert_eq!(loaded.divider.as_deref(), Some("|"));
	}

	#[test]
	fn settings_segments_not_serialized_when_none() {
		let settings = Settings {
			five_hour_reset_threshold: 70.0.into(),
			seven_day_reset_threshold: 100.0.into(),
			segments: None,
			divider: None,
			nerd_font: false,
			browser: None,
			skip_update_check: false,
			extra: Default::default(),
		};
		let json = serde_json::to_string(&settings).unwrap();
		assert!(!json.contains("segments"));
		assert!(!json.contains("divider"));
		assert!(!json.contains("browser"));
	}

	#[test]
	fn settings_skip_update_check_default_false() {
		let json = r#"{
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert!(!loaded.skip_update_check);
	}

	#[test]
	fn settings_skip_update_check_explicit_true() {
		let json = r#"{
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100,
			"skip_update_check": true
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert!(loaded.skip_update_check);
	}

	#[test]
	fn settings_with_browser() {
		let json = r#"{
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100,
			"browser": "brave"
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert_eq!(loaded.browser, Some(Browser::Brave));
	}

	#[test]
	fn settings_backward_compat_no_browser() {
		let json = r#"{
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100
		}"#;
		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert!(loaded.browser.is_none());
	}

	#[test]
	fn load_from_reads_a_settings_file() {
		let dir = std::env::temp_dir().join("statusline-core-load-from-test");
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("settings.json");
		std::fs::write(
			&path,
			r#"{"five_hour_reset_threshold":72.5,"seven_day_reset_threshold":100}"#,
		)
		.unwrap();

		let s = Settings::load_from(&path).unwrap();
		assert_eq!(s.five_hour_reset_threshold, 72.5.into());
		assert_eq!(s.seven_day_reset_threshold, 100.0.into());

		std::fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn settings_preserves_unknown_keys() {
		let json = r#"{"five_hour_reset_threshold":70,"seven_day_reset_threshold":100,"future_key":"keep me"}"#;
		let s: Settings = serde_json::from_str(json).unwrap();
		let out = serde_json::to_string(&s).unwrap();
		assert!(out.contains("future_key"), "unknown key dropped: {out}");
		assert!(out.contains("keep me"));
	}
}
