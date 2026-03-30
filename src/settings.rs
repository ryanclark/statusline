use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::format::Percentage;
use crate::util::app_data_dir;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Settings {
	pub(crate) org_id: String,
	pub(crate) five_hour_reset_threshold: Percentage,
	pub(crate) seven_day_reset_threshold: Percentage,
}

impl Settings {
	pub(crate) fn load() -> Result<Self> {
		let content = std::fs::read_to_string(settings_path()?)
			.context("reading settings file")?;

		serde_json::from_str(&content).context("parsing settings file")
	}

	pub(crate) fn ensure(org_id: &str, five_hour_reset_threshold: Percentage, seven_day_reset_threshold: Percentage) -> Result<Self> {
		let settings = Self {
			org_id: org_id.to_owned(),
			five_hour_reset_threshold,
			seven_day_reset_threshold,
		};

		let path = settings_path()?;
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).context("creating settings directory")?;
		}

		let content = serde_json::to_string_pretty(&settings).context("serializing settings")?;
		std::fs::write(&path, content).context("writing settings file")?;

		Ok(settings)
	}
}

fn settings_path() -> Result<std::path::PathBuf> {
	Ok(app_data_dir()?.join("settings.json"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn settings_roundtrip_serde() {
		let settings = Settings {
			org_id: "org-abc123".to_owned(),
			five_hour_reset_threshold: 70.0.into(),
			seven_day_reset_threshold: 100.0.into(),
		};

		let json = serde_json::to_string(&settings).unwrap();
		let loaded: Settings = serde_json::from_str(&json).unwrap();

		assert_eq!(loaded.org_id, "org-abc123");
		assert_eq!(loaded.five_hour_reset_threshold, 70.0.into());
		assert_eq!(loaded.seven_day_reset_threshold, 100.0.into());
	}

	#[test]
	fn settings_deserializes_pretty_json() {
		let json = r#"{
			"org_id": "org-test",
			"five_hour_reset_threshold": 50,
			"seven_day_reset_threshold": 80
		}"#;

		let loaded: Settings = serde_json::from_str(json).unwrap();
		assert_eq!(loaded.org_id, "org-test");
		assert_eq!(loaded.five_hour_reset_threshold, 50.0.into());
		assert_eq!(loaded.seven_day_reset_threshold, 80.0.into());
	}

	#[test]
	fn settings_rejects_missing_fields() {
		let json = r#"{"org_id": "org-test"}"#;
		let err = serde_json::from_str::<Settings>(json).unwrap_err();
		let msg = err.to_string();
		assert!(
			msg.contains("five_hour_reset_threshold"),
			"error should mention the missing field, got: {msg}"
		);
	}
}
