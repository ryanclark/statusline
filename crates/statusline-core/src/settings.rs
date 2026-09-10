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
	/// Layout for the agent panel rows (`statusline subagent`); absent means not set up.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub subagent_segments: Option<Vec<SegmentConfig>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub divider: Option<String>,
	#[serde(default)]
	pub nerd_font: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub browser: Option<Browser>,
	#[serde(default)]
	pub skip_update_check: bool,
	/// Lay the agent panel rows out as aligned columns instead of one free-form line per task.
	#[serde(default = "enabled")]
	pub subagent_grid: bool,
	#[serde(flatten)]
	pub extra: serde_json::Map<String, serde_json::Value>,
}

fn enabled() -> bool {
	true
}

impl Default for Settings {
	fn default() -> Self {
		Self {
			five_hour_reset_threshold: DEFAULT_FIVE_HOUR_RESET.into(),
			seven_day_reset_threshold: DEFAULT_SEVEN_DAY_RESET.into(),
			segments: None,
			subagent_segments: None,
			divider: None,
			nerd_font: false,
			browser: None,
			skip_update_check: false,
			subagent_grid: true,
			extra: serde_json::Map::default(),
		}
	}
}

impl Settings {
	pub fn load() -> Result<Self, SettingsError> {
		Self::load_from(&Self::settings_path()?)
	}

	pub fn load_from(path: &std::path::Path) -> Result<Self, SettingsError> {
		let content = std::fs::read_to_string(path)?;

		Ok(serde_json::from_str(&content)?)
	}

	/// Creates the settings file with defaults when it is missing and otherwise keeps it as it is,
	/// changing only the thresholds given explicitly. A file that fails to parse is an error rather
	/// than something to replace, since it may hold a layout the user spent time on.
	pub fn ensure_at(
		path: &std::path::Path,
		five_hour_reset_threshold: Option<Percentage>,
		seven_day_reset_threshold: Option<Percentage>,
	) -> Result<Self, SettingsError> {
		let (mut settings, mut changed) = match Self::load_from(path) {
			Ok(existing) => (existing, false),
			Err(SettingsError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
				(Self::default(), true)
			}
			Err(e) => return Err(e),
		};

		if let Some(five) = five_hour_reset_threshold
			&& five != settings.five_hour_reset_threshold
		{
			settings.five_hour_reset_threshold = five;
			changed = true;
		}
		if let Some(seven) = seven_day_reset_threshold
			&& seven != settings.seven_day_reset_threshold
		{
			settings.seven_day_reset_threshold = seven;
			changed = true;
		}

		if changed {
			settings.save(path)?;
		}

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
	fn subagent_segments_round_trip_and_stay_absent_by_default() {
		let json = r#"{"five_hour_reset_threshold": 70, "seven_day_reset_threshold": 100, "subagent_segments": ["task_name", "divider", "model"]}"#;
		let settings: Settings = serde_json::from_str(json).unwrap();
		assert_eq!(settings.subagent_segments.as_ref().map(Vec::len), Some(3));
		let back = serde_json::to_string(&settings).unwrap();
		assert!(
			back.contains(r#""subagent_segments":["task_name","divider","model"]"#),
			"{back}"
		);

		let plain: Settings = serde_json::from_str(
			r#"{"five_hour_reset_threshold": 70, "seven_day_reset_threshold": 100}"#,
		)
		.unwrap();
		assert!(plain.subagent_segments.is_none());
		assert!(
			!serde_json::to_string(&plain)
				.unwrap()
				.contains("subagent_segments")
		);
	}

	#[test]
	fn settings_roundtrip_serde() {
		let settings = Settings {
			five_hour_reset_threshold: 70.0.into(),
			seven_day_reset_threshold: 100.0.into(),
			segments: None,
			subagent_segments: None,
			divider: None,
			nerd_font: false,
			browser: None,
			skip_update_check: false,
			subagent_grid: true,
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
	fn subagent_grid_is_on_unless_the_file_turns_it_off() {
		let absent: Settings = serde_json::from_str(
			r#"{"five_hour_reset_threshold":70,"seven_day_reset_threshold":100}"#,
		)
		.unwrap();
		assert!(absent.subagent_grid);
		let off: Settings = serde_json::from_str(
			r#"{"five_hour_reset_threshold":70,"seven_day_reset_threshold":100,"subagent_grid":false}"#,
		)
		.unwrap();
		assert!(!off.subagent_grid);
		let json = serde_json::to_string(&off).unwrap();
		assert!(json.contains(r#""subagent_grid":false"#), "{json}");
	}

	#[test]
	fn settings_segments_not_serialized_when_none() {
		let settings = Settings {
			five_hour_reset_threshold: 70.0.into(),
			seven_day_reset_threshold: 100.0.into(),
			segments: None,
			subagent_segments: None,
			divider: None,
			nerd_font: false,
			browser: None,
			skip_update_check: false,
			subagent_grid: true,
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

	fn scratch(name: &str) -> std::path::PathBuf {
		let dir = std::env::temp_dir().join(format!("statusline-core-{name}"));
		std::fs::remove_dir_all(&dir).ok();
		dir.join("nested").join("settings.json")
	}

	const EXISTING: &str = r#"{"five_hour_reset_threshold":20,"seven_day_reset_threshold":50,"nerd_font":true,"segments":["model","cwd"]}"#;

	#[test]
	fn ensure_at_creates_the_file_with_defaults_when_missing() {
		let path = scratch("ensure-missing");
		let s = Settings::ensure_at(&path, Some(50.0.into()), None).unwrap();
		assert_eq!(s.five_hour_reset_threshold, 50.0.into());
		assert_eq!(s.seven_day_reset_threshold, DEFAULT_SEVEN_DAY_RESET.into());
		assert!(s.segments.is_none());
		let reloaded = Settings::load_from(&path).unwrap();
		assert_eq!(reloaded.five_hour_reset_threshold, 50.0.into());
	}

	#[test]
	fn ensure_at_leaves_an_existing_file_untouched() {
		let path = scratch("ensure-keep");
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, EXISTING).unwrap();
		let s = Settings::ensure_at(&path, None, None).unwrap();
		assert_eq!(s.segments.as_ref().map(Vec::len), Some(2));
		assert!(s.nerd_font);
		assert_eq!(s.five_hour_reset_threshold, 20.0.into());
		assert_eq!(
			std::fs::read_to_string(&path).unwrap(),
			EXISTING,
			"file must not be rewritten"
		);
	}

	#[test]
	fn ensure_at_applies_only_the_thresholds_given() {
		let path = scratch("ensure-thresholds");
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, EXISTING).unwrap();
		Settings::ensure_at(&path, Some(65.0.into()), None).unwrap();
		let reloaded = Settings::load_from(&path).unwrap();
		assert_eq!(reloaded.five_hour_reset_threshold, 65.0.into());
		assert_eq!(reloaded.seven_day_reset_threshold, 50.0.into());
		assert_eq!(reloaded.segments.as_ref().map(Vec::len), Some(2));
		assert!(reloaded.nerd_font);
	}

	#[test]
	fn ensure_at_refuses_to_replace_an_unreadable_file() {
		let path = scratch("ensure-corrupt");
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, "not json").unwrap();
		assert!(matches!(
			Settings::ensure_at(&path, None, None),
			Err(SettingsError::Parse(_))
		));
		assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
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
