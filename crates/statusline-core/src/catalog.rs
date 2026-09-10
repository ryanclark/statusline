use crate::segment::SegmentType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
	Context,
	RateLimits,
	Cost,
	Git,
	Environment,
	Account,
	Layout,
	Subagent,
}

impl Category {
	#[must_use]
	pub fn label(self) -> &'static str {
		match self {
			Self::Context => "Context window",
			Self::RateLimits => "Rate limits",
			Self::Cost => "Cost & performance",
			Self::Git => "Git",
			Self::Environment => "Environment",
			Self::Account => "Account",
			Self::Layout => "Layout",
			Self::Subagent => "Subagent",
		}
	}
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OptionSet {
	pub colors: bool,
	pub icon: bool,
	pub label: bool,
	pub style: bool,
	pub dirty: bool,
	pub capitalize: bool,
	/// Warm and cold colour options, only meaningful for the prompt cache state segment.
	pub cache_state: bool,
}

#[derive(Debug, Clone)]
pub struct SegmentMeta {
	pub ty: SegmentType,
	pub id: &'static str,
	pub label: &'static str,
	pub category: Category,
	pub description: &'static str,
	pub options: OptionSet,
}

const ICON_TEXT: OptionSet = OptionSet {
	colors: true,
	icon: true,
	label: true,
	style: true,
	dirty: false,
	capitalize: false,
	cache_state: false,
};

const COLORED_TEXT: OptionSet = OptionSet {
	colors: true,
	icon: false,
	label: false,
	style: true,
	dirty: false,
	capitalize: false,
	cache_state: false,
};

const NO_OPTIONS: OptionSet = OptionSet {
	colors: false,
	icon: false,
	label: false,
	style: false,
	dirty: false,
	capitalize: false,
	cache_state: false,
};

const STYLED_TEXT: OptionSet = OptionSet {
	colors: false,
	icon: false,
	label: false,
	style: true,
	dirty: false,
	capitalize: false,
	cache_state: false,
};

#[must_use]
pub fn catalog() -> &'static [SegmentMeta] {
	CATALOG
}

static CATALOG: &[SegmentMeta] = &[
	SegmentMeta {
		ty: SegmentType::ContextPercentage,
		id: "context_percentage",
		label: "Context %",
		category: Category::Context,
		description: "Context window used % (colored)",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::ContextRemaining,
		id: "context_remaining",
		label: "Context remaining",
		category: Category::Context,
		description: "Remaining context % (colored)",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::ContextWindowSize,
		id: "context_window_size",
		label: "Context window size",
		category: Category::Context,
		description: "Total context size (e.g. 200k)",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::TotalInputTokens,
		id: "total_input_tokens",
		label: "Total input tokens",
		category: Category::Context,
		description: "Current context input tokens (input + cache creation + cache read) with ↑ icon",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::InputTokens,
		id: "input_tokens",
		label: "Input tokens",
		category: Category::Context,
		description: "Cumulative input tokens across the session with ↑ icon",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::OutputTokens,
		id: "output_tokens",
		label: "Output tokens",
		category: Category::Context,
		description: "Total output tokens with ↓ icon",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::CacheReadTokens,
		id: "cache_read_tokens",
		label: "Cache read tokens",
		category: Category::Context,
		description: "Cache read tokens with ↻ icon",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::CacheHitRatio,
		id: "cache_hit_ratio",
		label: "Cache hit ratio",
		category: Category::Context,
		description: "Cache read as % of total input",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::CacheWarm,
		id: "cache_warm",
		label: "Cache warm",
		category: Category::Context,
		description: "Prompt cache state (warm with time until cold, or cold)",
		options: OptionSet {
			cache_state: true,
			..ICON_TEXT
		},
	},
	SegmentMeta {
		ty: SegmentType::SessionCacheHitRatio,
		id: "session_cache_hit_ratio",
		label: "Session cache hit ratio",
		category: Category::Context,
		description: "Cache reads as % of all input tokens this session",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::CacheMisses,
		id: "cache_misses",
		label: "Cache misses",
		category: Category::Context,
		description: "Prompt cache misses this session",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::CacheLastMiss,
		id: "cache_last_miss",
		label: "Last cache miss",
		category: Category::Context,
		description: "Cause of the last prompt cache miss (or `miss` when undiagnosed) and how long ago",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Exceeds200k,
		id: "exceeds200k",
		label: "Exceeds 200k",
		category: Category::Context,
		description: "Warning indicator when context exceeds 200k tokens",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::FiveHour,
		id: "five_hour",
		label: "5-hour limit",
		category: Category::RateLimits,
		description: "5-hour rate limit % with optional reset countdown",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::SevenDay,
		id: "seven_day",
		label: "7-day limit",
		category: Category::RateLimits,
		description: "7-day rate limit % with optional reset countdown",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::SpendLimit,
		id: "spend_limit",
		label: "Spend limit",
		category: Category::RateLimits,
		description: "Spend limit % with reset countdown (Claude apps gateway only)",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::FableUsage,
		id: "fable_usage",
		label: "Fable usage",
		category: Category::RateLimits,
		description: "Fable weekly rate limit % with reset countdown (calls the API)",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::ExtraUsage,
		id: "extra_usage",
		label: "Extra usage",
		category: Category::RateLimits,
		description: "Extra usage $used/$limit (calls the API)",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Credits,
		id: "credits",
		label: "Prepaid credits",
		category: Category::RateLimits,
		description: "Prepaid credit balance with ◉ icon (calls the API)",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Cost,
		id: "cost",
		label: "Cost",
		category: Category::Cost,
		description: "Total session cost in USD",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::CostRate,
		id: "cost_rate",
		label: "Cost rate",
		category: Category::Cost,
		description: "Cost per minute ($/m)",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Duration,
		id: "duration",
		label: "Duration",
		category: Category::Cost,
		description: "Total session duration",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::ApiDuration,
		id: "api_duration",
		label: "API duration",
		category: Category::Cost,
		description: "Total API call time",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::TokensPerSecond,
		id: "tokens_per_second",
		label: "Tokens / second",
		category: Category::Cost,
		description: "Output tokens per second of API time",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::LinesAdded,
		id: "lines_added",
		label: "Lines added",
		category: Category::Cost,
		description: "Lines added with + icon",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::LinesRemoved,
		id: "lines_removed",
		label: "Lines removed",
		category: Category::Cost,
		description: "Lines removed with - icon",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::GitBranch,
		id: "git_branch",
		label: "Git branch",
		category: Category::Git,
		description: "Current git branch name with optional dirty marker",
		options: OptionSet {
			colors: true,
			icon: false,
			label: false,
			style: true,
			dirty: true,
			capitalize: false,
			cache_state: false,
		},
	},
	SegmentMeta {
		ty: SegmentType::GitAheadBehind,
		id: "git_ahead_behind",
		label: "Git ahead/behind",
		category: Category::Git,
		description: "Commits ahead/behind upstream (e.g. ↑3 ↓1)",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::GitStash,
		id: "git_stash",
		label: "Git stash",
		category: Category::Git,
		description: "Stash count with ⚑ icon",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Pr,
		id: "pr",
		label: "Pull request",
		category: Category::Git,
		description: "Open PR number (! for merge requests), colored by review state; linked to the PR when colors are on",
		options: ICON_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Repo,
		id: "repo",
		label: "Repository",
		category: Category::Git,
		description: "Repository owner/name from the origin remote; linked to its web page when colors are on",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Cwd,
		id: "cwd",
		label: "Working directory",
		category: Category::Environment,
		description: "Current working directory (shortened with ~)",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::ProjectDir,
		id: "project_dir",
		label: "Project directory",
		category: Category::Environment,
		description: "Project directory",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Model,
		id: "model",
		label: "Model",
		category: Category::Environment,
		description: "Model display name",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::ModelId,
		id: "model_id",
		label: "Model ID",
		category: Category::Environment,
		description: "Full model ID",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Version,
		id: "version",
		label: "Version",
		category: Category::Environment,
		description: "Claude Code version",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::SessionId,
		id: "session_id",
		label: "Session ID",
		category: Category::Environment,
		description: "Session ID",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::SessionName,
		id: "session_name",
		label: "Session name",
		category: Category::Environment,
		description: "Session name set with --name or /rename, else the generated title",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::VimMode,
		id: "vim_mode",
		label: "Vim mode",
		category: Category::Environment,
		description: "Vim mode (NORMAL, INSERT, etc.)",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::AgentName,
		id: "agent_name",
		label: "Agent name",
		category: Category::Environment,
		description: "Active agent name",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Effort,
		id: "effort",
		label: "Effort",
		category: Category::Environment,
		description: "Reasoning effort level (low, medium, high, xhigh, max), colored by level",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Thinking,
		id: "thinking",
		label: "Thinking",
		category: Category::Environment,
		description: "Indicator when extended thinking is enabled",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::FastMode,
		id: "fast_mode",
		label: "Fast mode",
		category: Category::Environment,
		description: "Indicator when fast mode is on",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Worktree,
		id: "worktree",
		label: "Worktree",
		category: Category::Environment,
		description: "Worktree name (worktree session, or any linked git worktree)",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::Account,
		id: "account",
		label: "Account",
		category: Category::Account,
		description: "Current Claude account nickname (colored per entry)",
		options: OptionSet {
			colors: true,
			icon: false,
			label: false,
			style: true,
			dirty: false,
			capitalize: true,
			cache_state: false,
		},
	},
	SegmentMeta {
		ty: SegmentType::Divider,
		id: "divider",
		label: "Divider",
		category: Category::Layout,
		description: "Separator character (default •)",
		options: OptionSet {
			colors: true,
			icon: false,
			label: false,
			style: false,
			dirty: false,
			capitalize: false,
			cache_state: false,
		},
	},
	SegmentMeta {
		ty: SegmentType::Newline,
		id: "newline",
		label: "New line",
		category: Category::Layout,
		description: "Line break: segments after it render on the next row",
		options: NO_OPTIONS,
	},
	SegmentMeta {
		ty: SegmentType::TaskName,
		id: "task_name",
		label: "Task name",
		category: Category::Subagent,
		description: "Subagent name",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::TaskStatus,
		id: "task_status",
		label: "Task status",
		category: Category::Subagent,
		description: "Task status (running, completed, failed, pending), colored",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::TaskDescription,
		id: "task_description",
		label: "Task description",
		category: Category::Subagent,
		description: "Task description, dimmed",
		options: COLORED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::TaskElapsed,
		id: "task_elapsed",
		label: "Task elapsed",
		category: Category::Subagent,
		description: "Time since the task started",
		options: STYLED_TEXT,
	},
	SegmentMeta {
		ty: SegmentType::TaskTokens,
		id: "task_tokens",
		label: "Task tokens",
		category: Category::Subagent,
		description: "Tokens the task has used",
		options: STYLED_TEXT,
	},
];

#[must_use]
pub fn meta(ty: &SegmentType) -> &'static SegmentMeta {
	CATALOG
		.iter()
		.find(|m| m.ty == *ty)
		.expect("every SegmentType has a catalog entry")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_segment_type_has_catalog_entry() {
		let all = catalog();

		for m in all {
			assert!(!m.label.is_empty(), "{} has empty label", m.id);
			assert!(!m.description.is_empty(), "{} has empty description", m.id);
		}

		for m in all {
			let parsed: SegmentType = serde_json::from_str(&format!("\"{}\"", m.id))
				.unwrap_or_else(|e| panic!("id {} did not parse as a SegmentType: {e}", m.id));
			assert_eq!(parsed, m.ty, "id {} != its type", m.id);
		}

		let mut ids: Vec<&str> = all.iter().map(|m| m.id).collect();
		ids.sort_unstable();

		let unique = ids.len();
		ids.dedup();

		assert_eq!(unique, ids.len(), "duplicate ids in catalog");

		assert_eq!(
			all.len(),
			SegmentType::all().len(),
			"catalog count drifted from SegmentType variants"
		);
	}

	#[test]
	fn catalog_covers_every_segment_type() {
		for ty in SegmentType::all() {
			assert!(
				catalog().iter().any(|m| m.ty == ty),
				"{ty:?} has no catalog entry"
			);
		}
	}

	#[test]
	fn meta_looks_up_each_entry() {
		for m in catalog() {
			let looked_up = meta(&m.ty);
			assert_eq!(looked_up.id, m.id);
			assert_eq!(looked_up.options, m.options);
		}
	}

	#[test]
	fn divider_supports_colors_only() {
		assert_eq!(
			meta(&SegmentType::Divider).options,
			OptionSet {
				colors: true,
				..OptionSet::default()
			}
		);
	}

	#[test]
	fn newline_has_no_options() {
		assert_eq!(meta(&SegmentType::Newline).options, OptionSet::default());
	}

	#[test]
	fn cache_warm_is_the_only_cache_state_segment() {
		let with: Vec<&'static str> = catalog()
			.iter()
			.filter(|m| m.options.cache_state)
			.map(|m| m.id)
			.collect();
		assert_eq!(with, vec!["cache_warm"]);
	}

	#[test]
	fn git_branch_is_the_only_dirty_segment() {
		let dirty: Vec<&'static str> = catalog()
			.iter()
			.filter(|m| m.options.dirty)
			.map(|m| m.id)
			.collect();
		assert_eq!(dirty, vec!["git_branch"]);
	}

	#[test]
	fn label_implies_icon_for_every_segment() {
		for m in catalog() {
			assert!(
				!m.options.label || m.options.icon,
				"{} has label without icon",
				m.id
			);
		}
	}

	#[test]
	fn account_is_the_only_capitalize_segment() {
		let cap: Vec<&'static str> = catalog()
			.iter()
			.filter(|m| m.options.capitalize)
			.map(|m| m.id)
			.collect();
		assert_eq!(cap, vec!["account"]);
	}

	#[test]
	fn category_labels_non_empty() {
		for c in [
			Category::Context,
			Category::RateLimits,
			Category::Cost,
			Category::Git,
			Category::Environment,
			Category::Account,
			Category::Layout,
		] {
			assert!(!c.label().is_empty());
		}
	}
}
