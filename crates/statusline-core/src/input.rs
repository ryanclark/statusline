// All fields in this module's structs are deserialized from JSON input.
// Some fields are not yet read by existing segments but exist for completeness.
#![allow(dead_code)]

use crate::context_window::ContextWindow;
use crate::format::{Percentage, Tokens, countdown_to};
use crate::util::null_as_default;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

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
	/// Live activity that only the plugin sends, so segments reading it stay hidden under the native line.
	#[serde(default, rename = "mod")]
	pub mod_info: Option<ModInfo>,
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
/// `prompt_cache` object. Timestamps arrive as epoch seconds.
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
	#[serde(default, deserialize_with = "epoch_secs")]
	pub last_miss_at: Option<DateTime<Utc>>,
	#[serde(default)]
	pub last_miss_cause: Option<MissCause>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub miss_causes: BTreeMap<String, u64>,
	#[serde(default)]
	pub recache_tokens_if_cold: Option<Tokens>,
	/// Each miss, oldest first. Only the plugin sends it.
	#[serde(default, deserialize_with = "epoch_secs_list")]
	pub miss_times: Option<Vec<DateTime<Utc>>>,
}

// A second outside chrono's range is dropped rather than failing the whole input over one timestamp.
fn epoch_secs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
	Ok(Option::<i64>::deserialize(d)?.and_then(|at| DateTime::from_timestamp(at, 0)))
}

fn epoch_secs_list<'de, D: serde::Deserializer<'de>>(
	d: D,
) -> Result<Option<Vec<DateTime<Utc>>>, D::Error> {
	Ok(Option::<Vec<i64>>::deserialize(d)?.map(|times| {
		times
			.into_iter()
			.filter_map(|at| DateTime::from_timestamp(at, 0))
			.collect()
	}))
}

#[derive(Debug, Default, Deserialize)]
pub struct MissCause {
	#[serde(default, deserialize_with = "null_as_default")]
	pub causes: Vec<Code<MissCauseCode>>,
	#[serde(default)]
	pub tools_added: Option<u64>,
	#[serde(default)]
	pub tools_removed: Option<u64>,
	#[serde(default)]
	pub system_char_delta: Option<i64>,
}

/// The plugin's codes plus Claude Code's own closed set (`PROMPT_CACHE_MISS_CAUSES`, as listed in its status line
/// docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissCauseCode {
	TtlExpired,
	#[serde(rename = "ttl_expired_5m")]
	TtlExpired5m,
	#[serde(rename = "ttl_expired_1h")]
	TtlExpired1h,
	ToolsChanged,
	#[serde(alias = "system_prompt_changed")]
	SystemChanged,
	ModelChanged,
	Compacted,
	FastModeChanged,
	CacheScopeOrTtlChanged,
	BetasChanged,
	EffortChanged,
	ThinkingModeChanged,
	ThinkingDisplayChanged,
	AutoModeChanged,
	OverageChanged,
	ExtraBodyChanged,
	DeferLoadingChanged,
	MessagesRewritten,
	LikelyServerSide,
	/// Claude Code could not diagnose the miss.
	#[serde(alias = "")]
	Unknown,
}

/// Session activity from the plugin, whose times and lengths arrive in milliseconds.
#[derive(Debug, Default, Deserialize)]
pub struct ModInfo {
	#[serde(default, deserialize_with = "null_as_default")]
	pub tools: Vec<ToolCall>,
	#[serde(default)]
	pub turn: Option<TurnInfo>,
	#[serde(default)]
	pub permission: Option<PermissionWait>,
	#[serde(default)]
	pub last_error: Option<ApiError>,
	#[serde(default)]
	pub todos: Option<TodoProgress>,
	#[serde(default)]
	pub agents: Option<AgentCounts>,
	/// Background shells, monitors and workflows still running. Subagents are counted in `agents`.
	#[serde(default, deserialize_with = "null_as_default")]
	pub background_tasks: Vec<BackgroundTask>,
	#[serde(default)]
	pub compaction: Option<CompactionInfo>,
	#[serde(default)]
	pub autocompact: Option<AutocompactInfo>,
	#[serde(default)]
	pub usage: Option<PluginUsage>,
}

/// The OAuth usage response the plugin fetched with the session's own login, shared by every open chat.
#[derive(Debug, Default, Deserialize)]
pub struct PluginUsage {
	#[serde(
		default,
		rename = "fetched_at_ms",
		deserialize_with = "lenient_epoch_ms"
	)]
	pub fetched_at: Option<DateTime<Utc>>,
	/// Kept as JSON so a response that drifts from the usage types fails those segments, not the whole input.
	#[serde(default)]
	pub body: serde_json::Value,
}

// The time comes from a file every chat writes, so one that is not a whole i64 costs only the staleness check.
fn lenient_epoch_ms<'de, D: serde::Deserializer<'de>>(
	d: D,
) -> Result<Option<DateTime<Utc>>, D::Error> {
	Ok(serde_json::Value::deserialize(d)?
		.as_i64()
		.and_then(DateTime::from_timestamp_millis))
}

/// A code from a set that grows over time. One this build does not know keeps its text, so it can still be shown.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Code<T> {
	Known(T),
	Other(String),
}

impl<T: Default> Default for Code<T> {
	fn default() -> Self {
		Self::Known(T::default())
	}
}

fn duration_ms<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
	Ok(Option::<u64>::deserialize(d)?.map(Duration::from_millis))
}

#[derive(Debug, Default, Deserialize)]
pub struct ToolCall {
	#[serde(default, deserialize_with = "null_as_default")]
	pub tool: String,
	/// The Bash command, file path or search pattern, already truncated by the plugin.
	#[serde(default, deserialize_with = "null_as_default")]
	pub detail: String,
	#[serde(
		default,
		rename = "started_at_ms",
		with = "chrono::serde::ts_milliseconds_option"
	)]
	pub started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TurnInfo {
	#[serde(
		default,
		rename = "started_at_ms",
		with = "chrono::serde::ts_milliseconds_option"
	)]
	pub started_at: Option<DateTime<Utc>>,
	#[serde(default, rename = "last_duration_ms", deserialize_with = "duration_ms")]
	pub last_duration: Option<Duration>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PermissionWait {
	#[serde(default, deserialize_with = "null_as_default")]
	pub tool: String,
	#[serde(
		default,
		rename = "since_ms",
		with = "chrono::serde::ts_milliseconds_option"
	)]
	pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ApiError {
	#[serde(default, deserialize_with = "null_as_default")]
	pub kind: Code<ErrorKind>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub detail: String,
	#[serde(
		default,
		rename = "at_ms",
		with = "chrono::serde::ts_milliseconds_option"
	)]
	pub at: Option<DateTime<Utc>>,
}

/// An `SDKAssistantMessageError` code, or one of the plugin's own stop reasons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
	AuthenticationFailed,
	OauthOrgNotAllowed,
	AccountOnHold,
	VerificationRequired,
	BillingError,
	RateLimit,
	Overloaded,
	InvalidRequest,
	ModelNotFound,
	ServerError,
	CloudCredentialError,
	#[serde(alias = "max_tokens")]
	MaxOutputTokens,
	Refusal,
	Aborted,
	/// A failure with no code worth showing, which leaves the detail to explain it.
	#[default]
	#[serde(alias = "unknown", alias = "")]
	Error,
}

#[derive(Debug, Default, Deserialize)]
pub struct TodoProgress {
	#[serde(default, deserialize_with = "null_as_default")]
	pub done: u64,
	#[serde(default, deserialize_with = "null_as_default")]
	pub total: u64,
	#[serde(default, deserialize_with = "null_as_default")]
	pub active: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct AgentCounts {
	#[serde(default, deserialize_with = "null_as_default")]
	pub running: u64,
	#[serde(default, deserialize_with = "null_as_default")]
	pub idle: u64,
}

#[derive(Debug, Default, Deserialize)]
pub struct BackgroundTask {
	#[serde(rename = "type", default, deserialize_with = "null_as_default")]
	pub kind: Code<TaskKind>,
	/// The task's description, its command when it has none, or a workflow's name. Already truncated by the plugin.
	#[serde(default, deserialize_with = "null_as_default")]
	pub description: String,
}

/// Claude Code's label for the kind of background task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
	Shell,
	Monitor,
	Workflow,
	#[default]
	#[serde(rename = "")]
	Unlabelled,
}

#[derive(Debug, Default, Deserialize)]
pub struct CompactionInfo {
	#[serde(default, deserialize_with = "null_as_default")]
	pub count: u64,
	#[serde(
		default,
		rename = "last_at_ms",
		with = "chrono::serde::ts_milliseconds_option"
	)]
	pub last_at: Option<DateTime<Utc>>,
	#[serde(default)]
	pub tokens_before: Option<Tokens>,
	#[serde(default)]
	pub tokens_after: Option<Tokens>,
	#[serde(
		default,
		rename = "running_since_ms",
		with = "chrono::serde::ts_milliseconds_option"
	)]
	pub running_since: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AutocompactInfo {
	#[serde(default, deserialize_with = "null_as_default")]
	pub enabled: bool,
	#[serde(default)]
	pub headroom_tokens: Option<Tokens>,
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
		assert_eq!(cache.last_miss_at, DateTime::from_timestamp(1738425230, 0));
		assert_eq!(
			cache.last_miss_cause.unwrap().causes,
			[Code::Known(MissCauseCode::ToolsChanged)]
		);
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
	fn parse_prompt_cache_miss_times() {
		for (json, want) in [
			(r#"{"prompt_cache": {}}"#, None),
			(r#"{"prompt_cache": {"miss_times": null}}"#, None),
			(r#"{"prompt_cache": {"miss_times": []}}"#, Some(vec![])),
			(
				r#"{"prompt_cache": {"miss_times": [1791279000, 1791280100]}}"#,
				Some(vec![1_791_279_000, 1_791_280_100]),
			),
		] {
			let cache = InputData::from_reader(json.as_bytes())
				.unwrap()
				.prompt_cache
				.unwrap();
			let want = want.map(|times| {
				times
					.into_iter()
					.map(|at| DateTime::from_timestamp(at, 0).unwrap())
					.collect()
			});
			assert_eq!(cache.miss_times, want, "{json}");
		}
	}

	#[test]
	fn mod_is_absent_on_the_cli_path_and_null_tolerant() {
		assert!(
			InputData::from_reader(b"{}" as &[u8])
				.unwrap()
				.mod_info
				.is_none()
		);
		assert!(
			InputData::from_reader(r#"{"mod": null}"#.as_bytes())
				.unwrap()
				.mod_info
				.is_none()
		);
		let json = r#"{"mod": {"tools": null, "turn": null, "permission": null, "last_error": null,
			"todos": null, "agents": null, "background_tasks": null, "compaction": null, "autocompact": null,
			"usage": null}}"#;
		let m = InputData::from_reader(json.as_bytes())
			.unwrap()
			.mod_info
			.unwrap();
		assert!(m.tools.is_empty());
		assert!(m.turn.is_none() && m.permission.is_none() && m.last_error.is_none());
		assert!(m.todos.is_none() && m.agents.is_none() && m.background_tasks.is_empty());
		assert!(m.compaction.is_none() && m.autocompact.is_none() && m.usage.is_none());
		let empty = InputData::from_reader(r#"{"mod": {}}"#.as_bytes())
			.unwrap()
			.mod_info
			.unwrap();
		assert!(empty.tools.is_empty() && empty.turn.is_none());

		let json = r#"{"mod": {
			"tools": [{"tool": null, "detail": null, "started_at_ms": null}],
			"turn": {"started_at_ms": null, "last_duration_ms": null, "ended_at_ms": null},
			"permission": {"tool": null, "since_ms": null},
			"last_error": {"kind": null, "detail": null, "at_ms": null},
			"todos": {"done": 0, "total": 2, "active": null},
			"background_tasks": [{"type": null, "description": null}],
			"compaction": {"count": 0, "last_at_ms": null, "tokens_before": null, "tokens_after": null,
				"running_since_ms": null, "trigger": null},
			"autocompact": {"enabled": false, "headroom_tokens": null}
		}}"#;
		let m = InputData::from_reader(json.as_bytes())
			.unwrap()
			.mod_info
			.unwrap();
		assert!(m.tools[0].tool.is_empty() && m.tools[0].started_at.is_none());
		let turn = m.turn.unwrap();
		assert!(turn.started_at.is_none() && turn.last_duration.is_none());
		let permission = m.permission.unwrap();
		assert!(permission.tool.is_empty() && permission.since.is_none());
		let error = m.last_error.unwrap();
		assert_eq!(error.kind, Code::Known(ErrorKind::Error));
		assert!(error.detail.is_empty() && error.at.is_none());
		assert_eq!(m.todos.unwrap().active, "");
		assert_eq!(
			m.background_tasks[0].kind,
			Code::Known(TaskKind::Unlabelled)
		);
		let compaction = m.compaction.unwrap();
		assert!(compaction.last_at.is_none() && compaction.running_since.is_none());
		assert!(compaction.tokens_before.is_none() && compaction.tokens_after.is_none());
		assert!(m.autocompact.unwrap().headroom_tokens.is_none());
	}

	#[test]
	fn parse_mod_from_the_contract_shape() {
		let json = r#"{"mod": {
			"tools": [{"tool": "Bash", "detail": "cargo test -p core", "started_at_ms": 1791280000000},
				{"tool": "Read", "detail": null, "started_at_ms": 1791280001000}],
			"turn": {"started_at_ms": 1791280000000, "last_duration_ms": 130000, "ended_at_ms": 1791279900000},
			"permission": {"tool": "Bash", "since_ms": 1791280000000},
			"last_error": {"kind": "overloaded", "detail": "529 Overloaded", "at_ms": 1791280000000},
			"todos": {"done": 3, "total": 7, "active": "Running tests"},
			"agents": {"running": 3, "idle": 1},
			"background_tasks": [{"type": "shell", "description": "npm run dev"},
				{"type": "monitor", "description": null}, {"type": "remote_agent"}, {"type": null}],
			"compaction": {"count": 2, "last_at_ms": 1791279000000, "tokens_before": 182000,
				"tokens_after": 21000, "running_since_ms": null, "trigger": "auto"},
			"autocompact": {"enabled": true, "headroom_tokens": 38000}
		}}"#;
		let m = InputData::from_reader(json.as_bytes())
			.unwrap()
			.mod_info
			.unwrap();
		assert_eq!(m.tools.len(), 2);
		assert_eq!(m.tools[0].tool, "Bash");
		assert_eq!(m.tools[0].detail, "cargo test -p core");
		let at = DateTime::from_timestamp_millis(1_791_280_000_000);
		assert_eq!(m.tools[0].started_at, at);
		assert_eq!(m.tools[1].detail, "");
		let turn = m.turn.unwrap();
		assert_eq!(turn.started_at, at);
		assert_eq!(turn.last_duration, Some(Duration::from_secs(130)));
		let permission = m.permission.unwrap();
		assert_eq!((permission.tool.as_str(), permission.since), ("Bash", at));
		let error = m.last_error.unwrap();
		assert_eq!(error.kind, Code::Known(ErrorKind::Overloaded));
		assert_eq!(error.detail, "529 Overloaded");
		assert_eq!(error.at, at);
		let todos = m.todos.unwrap();
		assert_eq!(
			(todos.done, todos.total, todos.active.as_str()),
			(3, 7, "Running tests")
		);
		let agents = m.agents.unwrap();
		assert_eq!((agents.running, agents.idle), (3, 1));
		let background: Vec<_> = m
			.background_tasks
			.iter()
			.map(|t| (&t.kind, t.description.as_str()))
			.collect();
		assert_eq!(
			background,
			[
				(&Code::Known(TaskKind::Shell), "npm run dev"),
				(&Code::Known(TaskKind::Monitor), ""),
				(&Code::Other("remote_agent".to_owned()), ""),
				(&Code::Known(TaskKind::Unlabelled), ""),
			]
		);
		let compaction = m.compaction.unwrap();
		assert_eq!(compaction.count, 2);
		assert_eq!(
			compaction.last_at,
			DateTime::from_timestamp_millis(1_791_279_000_000)
		);
		assert_eq!(compaction.tokens_before, Some(182_000.into()));
		assert_eq!(compaction.tokens_after, Some(21_000.into()));
		assert!(compaction.running_since.is_none());
		let autocompact = m.autocompact.unwrap();
		assert!(autocompact.enabled);
		assert_eq!(autocompact.headroom_tokens, Some(38_000.into()));
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
	fn mod_usage_keeps_the_body_as_json_whatever_its_shape() {
		let json = r#"{"mod": {"usage": {"fetched_at_ms": 1791280000000, "body": {"limits": "drifted"}}}}"#;
		let usage = InputData::from_reader(json.as_bytes())
			.unwrap()
			.mod_info
			.unwrap()
			.usage
			.unwrap();
		assert_eq!(
			usage.fetched_at,
			DateTime::from_timestamp_millis(1_791_280_000_000)
		);
		assert_eq!(usage.body["limits"], "drifted");
	}

	#[test]
	fn a_mod_usage_time_that_is_not_an_i64_drops_only_the_time() {
		for at in ["1.5", "1e20", "\"soon\""] {
			let json = format!(
				r#"{{"model": {{"display_name": "Opus"}}, "mod": {{"usage": {{"fetched_at_ms": {at}, "body": {{}}}}}}}}"#
			);
			let input = InputData::from_reader(json.as_bytes()).unwrap();
			assert_eq!(input.model.display_name, "Opus");
			let usage = input.mod_info.unwrap().usage.unwrap();
			assert_eq!(usage.fetched_at, None, "{at}");
			assert!(usage.body.is_object());
		}
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
