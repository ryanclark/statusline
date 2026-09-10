// All fields in this module's structs are deserialized from JSON input.
// Some fields are not yet read by existing segments but exist for completeness.
#![allow(dead_code)]

use crate::context_window::ContextWindow;
use crate::format::{Percentage, Tokens, countdown_to};
use crate::util::null_as_default;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Deserialize)]
pub struct InputData {
	#[serde(default)]
	pub cwd: String,
	#[serde(default)]
	pub session_id: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub session_name: String,
	#[serde(default)]
	pub model: ModelInfo,
	#[serde(default)]
	pub workspace: Workspace,
	#[serde(default)]
	pub version: String,
	#[serde(default)]
	pub cost: CostInfo,
	#[serde(default)]
	pub context_window: ContextWindow,
	#[serde(default)]
	pub rate_limits: RateLimits,
	#[serde(default)]
	pub vim: VimInfo,
	#[serde(default)]
	pub agent: AgentInfo,
	#[serde(default)]
	pub worktree: WorktreeInfo,
	#[serde(default)]
	pub exceeds_200k_tokens: bool,
	#[serde(default, deserialize_with = "null_as_default")]
	pub fast_mode: bool,
	#[serde(default, deserialize_with = "null_as_default")]
	pub effort: EffortInfo,
	#[serde(default, deserialize_with = "null_as_default")]
	pub thinking: ThinkingInfo,
	#[serde(default, deserialize_with = "null_as_default")]
	pub pr: PrInfo,
	/// Absent until the main conversation's first API response.
	#[serde(default)]
	pub prompt_cache: Option<PromptCache>,
}

impl InputData {
	pub fn from_reader(reader: impl std::io::Read) -> Result<Self, serde_json::Error> {
		serde_json::from_reader(reader)
	}
}

#[derive(Debug, Default, Deserialize)]
pub struct ModelInfo {
	#[serde(default)]
	pub id: String,
	#[serde(default)]
	pub display_name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Workspace {
	#[serde(default)]
	pub current_dir: String,
	#[serde(default)]
	pub project_dir: String,
	/// Set for any linked git worktree, unlike `worktree.*`, which only exists in a worktree session.
	#[serde(default, deserialize_with = "null_as_default")]
	pub git_worktree: String,
	#[serde(default)]
	pub repo: Option<RepoInfo>,
}

/// Repository identity parsed from the `origin` remote. `owner` may contain slashes for nested
/// GitLab groups.
#[derive(Debug, Default, Deserialize)]
pub struct RepoInfo {
	#[serde(default, deserialize_with = "null_as_default")]
	pub host: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub owner: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct CostInfo {
	#[serde(default)]
	pub total_cost_usd: f64,
	#[serde(default)]
	pub total_duration_ms: u64,
	#[serde(default)]
	pub total_api_duration_ms: u64,
	#[serde(default)]
	pub total_lines_added: u64,
	#[serde(default)]
	pub total_lines_removed: u64,
}

#[derive(Debug, Default, Deserialize)]
pub struct RateLimits {
	#[serde(default)]
	pub five_hour: Option<RateLimitPeriod>,
	#[serde(default)]
	pub seven_day: Option<RateLimitPeriod>,
	#[serde(default)]
	pub spend_limit: Option<RateLimitPeriod>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitPeriod {
	pub used_percentage: Percentage,
	pub resets_at: i64,
}

impl RateLimitPeriod {
	#[must_use]
	pub fn countdown(&self, now: DateTime<Utc>) -> Option<String> {
		countdown_to(self.resets_at, now)
	}
}

#[derive(Debug, Default, Deserialize)]
pub struct VimInfo {
	#[serde(default)]
	pub mode: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct AgentInfo {
	#[serde(default)]
	pub name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct EffortInfo {
	#[serde(default, deserialize_with = "null_as_default")]
	pub level: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ThinkingInfo {
	#[serde(default, deserialize_with = "null_as_default")]
	pub enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct PrInfo {
	#[serde(default)]
	pub number: Option<u64>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub url: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub review_state: String,
	/// `mr` when the entry describes a GitLab merge request; absent for GitHub pull requests.
	#[serde(default, deserialize_with = "null_as_default")]
	pub kind: String,
}

/// The session's prompt cache statistics, as documented under Claude Code's status line
/// `prompt_cache` object. Timestamps are epoch seconds.
#[derive(Debug, Default, Deserialize)]
pub struct PromptCache {
	#[serde(default, deserialize_with = "null_as_default")]
	pub warm: bool,
	#[serde(default, deserialize_with = "null_as_default")]
	pub caching_observed: bool,
	#[serde(default, deserialize_with = "null_as_default")]
	pub ttl: String,
	#[serde(default)]
	pub expires_at: Option<i64>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub requests: u64,
	#[serde(default, deserialize_with = "null_as_default")]
	pub misses: u64,
	#[serde(default, deserialize_with = "null_as_default")]
	pub expected_rebuilds: u64,
	#[serde(default)]
	pub hit_ratio: Option<f64>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub cache_write_tokens: Tokens,
	#[serde(default, deserialize_with = "null_as_default")]
	pub miss_recache_tokens: Tokens,
	#[serde(default)]
	pub last_miss_at: Option<i64>,
	#[serde(default)]
	pub last_miss_cause: Option<MissCause>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub miss_causes: BTreeMap<String, u64>,
	#[serde(default)]
	pub recache_tokens_if_cold: Option<Tokens>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MissCause {
	#[serde(default, deserialize_with = "null_as_default")]
	pub causes: Vec<String>,
	#[serde(default)]
	pub tools_added: Option<u64>,
	#[serde(default)]
	pub tools_removed: Option<u64>,
	#[serde(default)]
	pub system_char_delta: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WorktreeInfo {
	#[serde(default)]
	pub name: String,
	#[serde(default)]
	pub branch: String,
	#[serde(default)]
	pub original_branch: String,
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
			"fast_mode": true,
			"effort": {"level": "high"},
			"thinking": {"enabled": true},
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
		assert!((input.cost.total_cost_usd - 0.01234).abs() < f64::EPSILON);
		assert_eq!(input.cost.total_lines_added, 156);
		assert_eq!(input.context_window.used_percentage, 8.0.into());
		assert!(input.rate_limits.five_hour.is_some());
		let five = input.rate_limits.five_hour.unwrap();
		assert_eq!(five.used_percentage, 23.5.into());
		assert_eq!(five.resets_at, 1738425600);
		assert_eq!(input.vim.mode, "NORMAL");
		assert!(input.fast_mode);
		assert_eq!(input.effort.level, "high");
		assert!(input.thinking.enabled);
		assert_eq!(input.agent.name, "security-reviewer");
		assert_eq!(input.worktree.name, "my-feature");
	}

	#[test]
	fn parse_spend_limit_window() {
		let json = r#"{"rate_limits": {"spend_limit": {"used_percentage": 62.8, "resets_at": 1740787200}}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		let spend = input
			.rate_limits
			.spend_limit
			.expect("spend_limit window should parse");
		assert_eq!(spend.used_percentage, 62.8.into());
		assert_eq!(spend.resets_at, 1740787200);
		assert!(input.rate_limits.five_hour.is_none());
	}

	#[test]
	fn null_context_usage_does_not_fail_the_whole_parse() {
		let json = r#"{"cwd": "/tmp/x", "context_window": {"used_percentage": null, "remaining_percentage": null, "total_input_tokens": 0, "total_output_tokens": 0, "context_window_size": 200000, "current_usage": null}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		assert_eq!(input.cwd, "/tmp/x");
		assert_eq!(
			input.context_window.current_usage.cache_read_input_tokens,
			0.into()
		);
	}

	#[test]
	fn parse_pull_request() {
		let json = r#"{"pr": {"number": 1234, "url": "https://github.com/anthropics/claude-code/pull/1234", "review_state": "pending"}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		assert_eq!(input.pr.number, Some(1234));
		assert_eq!(
			input.pr.url,
			"https://github.com/anthropics/claude-code/pull/1234"
		);
		assert_eq!(input.pr.review_state, "pending");
		assert_eq!(input.pr.kind, "");

		let mr =
			InputData::from_reader(r#"{"pr": {"number": 12, "kind": "mr"}}"#.as_bytes()).unwrap();
		assert_eq!(mr.pr.number, Some(12));
		assert_eq!(mr.pr.kind, "mr");

		let none = InputData::from_reader(r#"{"pr": null}"#.as_bytes()).unwrap();
		assert!(none.pr.number.is_none());
	}

	#[test]
	fn parse_prompt_cache_from_the_documented_shape() {
		let json = r#"{"prompt_cache": {
			"warm": true, "caching_observed": true, "ttl": "1h", "expires_at": 1738429200,
			"requests": 14, "misses": 2, "expected_rebuilds": 1, "hit_ratio": 0.91,
			"cache_write_tokens": 352000, "miss_recache_tokens": 310200, "last_miss_at": 1738425230,
			"last_miss_cause": {"causes": ["tools_changed"], "tools_added": 2, "tools_removed": 0},
			"miss_causes": {"tools_changed": 2}, "recache_tokens_if_cold": 45000
		}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		let cache = input.prompt_cache.expect("prompt_cache should parse");
		assert!(cache.warm);
		assert!(cache.caching_observed);
		assert_eq!(cache.ttl, "1h");
		assert_eq!(cache.expires_at, Some(1738429200));
		assert_eq!((cache.requests, cache.misses), (14, 2));
		assert_eq!(cache.hit_ratio, Some(0.91));
		assert_eq!(cache.last_miss_at, Some(1738425230));
		assert_eq!(cache.last_miss_cause.unwrap().causes, vec!["tools_changed"]);
	}

	#[test]
	fn prompt_cache_absent_null_or_partial_still_parses() {
		assert!(
			InputData::from_reader(b"{}" as &[u8])
				.unwrap()
				.prompt_cache
				.is_none()
		);
		assert!(
			InputData::from_reader(r#"{"prompt_cache": null}"#.as_bytes())
				.unwrap()
				.prompt_cache
				.is_none()
		);
		let json = r#"{"prompt_cache": {"warm": false, "caching_observed": true, "expires_at": null, "hit_ratio": null, "last_miss_at": null, "last_miss_cause": null, "recache_tokens_if_cold": null}}"#;
		let cache = InputData::from_reader(json.as_bytes())
			.unwrap()
			.prompt_cache
			.unwrap();
		assert!(!cache.warm);
		assert!(cache.hit_ratio.is_none());
		assert!(cache.last_miss_cause.is_none());
	}

	#[test]
	fn parse_session_name_repo_and_git_worktree() {
		let json = r#"{"session_name": "my-session", "workspace": {"current_dir": "/x", "git_worktree": "feature-xyz", "repo": {"host": "github.com", "owner": "anthropics", "name": "claude-code"}}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		assert_eq!(input.session_name, "my-session");
		assert_eq!(input.workspace.git_worktree, "feature-xyz");
		let repo = input.workspace.repo.expect("repo should parse");
		assert_eq!(
			(repo.host.as_str(), repo.owner.as_str(), repo.name.as_str()),
			("github.com", "anthropics", "claude-code")
		);

		let bare = InputData::from_reader(
			r#"{"session_name": null, "workspace": {"repo": null, "git_worktree": null}}"#
				.as_bytes(),
		)
		.unwrap();
		assert!(bare.session_name.is_empty());
		assert!(bare.workspace.repo.is_none());
		assert!(bare.workspace.git_worktree.is_empty());
	}

	#[test]
	fn null_effort_level_and_thinking_flag_parse_as_defaults() {
		let json = r#"{"effort": {"level": null}, "thinking": {"enabled": null}}"#;
		let input = InputData::from_reader(json.as_bytes()).unwrap();
		assert_eq!(input.effort.level, "");
		assert!(!input.thinking.enabled);
	}

	#[test]
	fn parse_empty_json() {
		let input = InputData::from_reader(b"{}" as &[u8]).unwrap();
		assert_eq!(input.cwd, "");
		assert_eq!(input.model.display_name, "");
		assert!((input.cost.total_cost_usd - 0.0).abs() < f64::EPSILON);
		assert!(input.rate_limits.five_hour.is_none());
		assert!(!input.fast_mode);
		assert!(!input.thinking.enabled);
		assert_eq!(input.effort.level, "");
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
