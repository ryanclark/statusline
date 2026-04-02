// All fields in this module's structs are deserialized from JSON input.
// Some fields are not yet read by existing segments but exist for completeness.
#![allow(dead_code)]

use crate::context_window::ContextWindow;
use crate::format::Percentage;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct InputData {
	#[serde(default)]
	pub(crate) cwd: String,
	#[serde(default)]
	pub(crate) session_id: String,
	#[serde(default)]
	pub(crate) model: ModelInfo,
	#[serde(default)]
	pub(crate) workspace: Workspace,
	#[serde(default)]
	pub(crate) version: String,
	#[serde(default)]
	pub(crate) cost: CostInfo,
	#[serde(default)]
	pub(crate) context_window: ContextWindow,
	#[serde(default)]
	pub(crate) rate_limits: RateLimits,
	#[serde(default)]
	pub(crate) vim: VimInfo,
	#[serde(default)]
	pub(crate) agent: AgentInfo,
	#[serde(default)]
	pub(crate) worktree: WorktreeInfo,
	#[serde(default)]
	pub(crate) exceeds_200k_tokens: bool,
}

impl InputData {
	pub(crate) fn from_reader(reader: impl std::io::Read) -> Result<Self, serde_json::Error> {
		serde_json::from_reader(reader)
	}
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ModelInfo {
	#[serde(default)]
	pub(crate) id: String,
	#[serde(default)]
	pub(crate) display_name: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Workspace {
	#[serde(default)]
	pub(crate) current_dir: String,
	#[serde(default)]
	pub(crate) project_dir: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct CostInfo {
	#[serde(default)]
	pub(crate) total_cost_usd: f64,
	#[serde(default)]
	pub(crate) total_duration_ms: u64,
	#[serde(default)]
	pub(crate) total_api_duration_ms: u64,
	#[serde(default)]
	pub(crate) total_lines_added: u64,
	#[serde(default)]
	pub(crate) total_lines_removed: u64,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RateLimits {
	#[serde(default)]
	pub(crate) five_hour: Option<RateLimitPeriod>,
	#[serde(default)]
	pub(crate) seven_day: Option<RateLimitPeriod>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RateLimitPeriod {
	pub(crate) used_percentage: Percentage,
	pub(crate) resets_at: i64,
}

impl RateLimitPeriod {
	pub(crate) fn countdown(&self, now: DateTime<Utc>) -> Option<String> {
		let reset_time = DateTime::from_timestamp(self.resets_at, 0)?;
		let total_secs = reset_time.signed_duration_since(now).num_seconds();
		if total_secs <= 0 {
			return None;
		}
		
		#[allow(clippy::cast_sign_loss)] // guarded by total_secs > 0 above
		Some(crate::format::format_duration_secs(total_secs as u64))
	}
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct VimInfo {
	#[serde(default)]
	pub(crate) mode: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct AgentInfo {
	#[serde(default)]
	pub(crate) name: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct WorktreeInfo {
	#[serde(default)]
	pub(crate) name: String,
	#[serde(default)]
	pub(crate) branch: String,
	#[serde(default)]
	pub(crate) original_branch: String,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_full_stdin() {
		let json = r#"{
			"cwd": "/home/user/project",
			"session_id": "abc123",
			"model": {"id": "claude-opus-4-6", "display_name": "Opus"},
			"workspace": {"current_dir": "/home/user/project", "project_dir": "/home/user/project"},
			"version": "1.0.80",
			"cost": {
				"total_cost_usd": 0.01234,
				"total_duration_ms": 45000,
				"total_api_duration_ms": 2300,
				"total_lines_added": 156,
				"total_lines_removed": 23
			},
			"context_window": {
				"used_percentage": 8,
				"total_output_tokens": 4521,
				"current_usage": {
					"input_tokens": 8500,
					"output_tokens": 1200,
					"cache_creation_input_tokens": 5000,
					"cache_read_input_tokens": 2000
				}
			},
			"rate_limits": {
				"five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
				"seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}
			},
			"vim": {"mode": "NORMAL"},
			"agent": {"name": "security-reviewer"},
			"worktree": {"name": "my-feature", "branch": "worktree-my-feature", "original_branch": "main"}
		}"#;

		let input = InputData::from_reader(json.as_bytes()).unwrap();
		assert_eq!(input.cwd, "/home/user/project");
		assert_eq!(input.session_id, "abc123");
		assert_eq!(input.model.id, "claude-opus-4-6");
		assert_eq!(input.model.display_name, "Opus");
		assert_eq!(input.version, "1.0.80");
		assert_eq!(input.cost.total_cost_usd, 0.01234);
		assert_eq!(input.cost.total_lines_added, 156);
		assert_eq!(input.context_window.used_percentage, 8.0.into());
		assert!(input.rate_limits.five_hour.is_some());
		let five = input.rate_limits.five_hour.unwrap();
		assert_eq!(five.used_percentage, 23.5.into());
		assert_eq!(five.resets_at, 1738425600);
		assert_eq!(input.vim.mode, "NORMAL");
		assert_eq!(input.agent.name, "security-reviewer");
		assert_eq!(input.worktree.name, "my-feature");
	}

	#[test]
	fn parse_empty_json() {
		let input = InputData::from_reader(b"{}" as &[u8]).unwrap();
		assert_eq!(input.cwd, "");
		assert_eq!(input.model.display_name, "");
		assert_eq!(input.cost.total_cost_usd, 0.0);
		assert!(input.rate_limits.five_hour.is_none());
	}

	#[test]
	fn parse_partial_json() {
		let json = r#"{"cwd": "/tmp", "model": {"display_name": "Opus"}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		assert_eq!(input.cwd, "/tmp");
		assert_eq!(input.model.display_name, "Opus");
		assert_eq!(input.model.id, "");
		assert_eq!(input.version, "");
	}

	#[test]
	fn rate_limit_countdown_future() {
		let now = DateTime::from_timestamp(1000000, 0).unwrap();
		let period = RateLimitPeriod {
			used_percentage: 50.0.into(),
			resets_at: 1007200, // 7200 seconds later = 2h0m
		};
		assert_eq!(period.countdown(now), Some("2h0m".to_owned()));
	}

	#[test]
	fn rate_limit_countdown_past() {
		let now = DateTime::from_timestamp(1000000, 0).unwrap();
		let period = RateLimitPeriod {
			used_percentage: 50.0.into(),
			resets_at: 999000,
		};
		assert_eq!(period.countdown(now), None);
	}

	#[test]
	fn rate_limit_countdown_days() {
		let now = DateTime::from_timestamp(1000000, 0).unwrap();
		let period = RateLimitPeriod {
			used_percentage: 50.0.into(),
			resets_at: 1000000 + 90061, // 1d1h
		};
		assert_eq!(period.countdown(now), Some("1d1h".to_owned()));
	}

	#[test]
	fn rate_limit_countdown_seconds() {
		let now = DateTime::from_timestamp(1000000, 0).unwrap();
		let period = RateLimitPeriod {
			used_percentage: 50.0.into(),
			resets_at: 1000030,
		};
		assert_eq!(period.countdown(now), Some("30s".to_owned()));
	}
}
