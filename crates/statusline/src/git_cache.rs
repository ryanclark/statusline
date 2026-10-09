//! The renderer reads cached Git data. A detached copy of the binary refreshes it, sharing a nonblocking lock
//! across chats and inheriting that lock on stdin until the worker exits.

use crate::background::{bounded_output, claim};
use crate::segment::{DirtyConfig, GitCache, SegmentConfig, SegmentType};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const MAX_AGE: Duration = Duration::from_secs(5);
const REFRESH_BUDGET: Duration = Duration::from_secs(10);
const BRANCH_BUDGET: Duration = Duration::from_secs(1);
const RETRY_MIN: Duration = Duration::from_secs(30);
const RETRY_MAX: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, clap::Args)]
pub struct Fields {
	#[arg(long)]
	branch: bool,
	#[arg(long)]
	dirty: bool,
	#[arg(long)]
	ahead_behind: bool,
	#[arg(long)]
	stash: bool,
}

impl Fields {
	fn for_segments(segments: &[SegmentConfig]) -> Self {
		let mut fields = Self::default();
		for segment in segments.iter().filter(|s| s.enabled()) {
			match segment.segment_type() {
				SegmentType::GitBranch => {
					fields.branch = true;
					if let SegmentConfig::Advanced(options) = segment {
						fields.dirty |= match &options.dirty {
							DirtyConfig::Off => false,
							DirtyConfig::On => true,
							DirtyConfig::Custom(s) => !s.is_empty(),
						};
					}
				}
				SegmentType::GitAheadBehind => fields.ahead_behind = true,
				SegmentType::GitStash => fields.stash = true,
				_ => {}
			}
		}
		fields
	}

	fn args(self) -> impl Iterator<Item = &'static str> {
		[
			(self.branch, "--branch"),
			(self.dirty, "--dirty"),
			(self.ahead_behind, "--ahead-behind"),
			(self.stash, "--stash"),
		]
		.into_iter()
		.filter_map(|(needed, flag)| needed.then_some(flag))
	}
}

fn cache_name(cwd: &str, fields: Fields) -> String {
	let mut hash = std::collections::hash_map::DefaultHasher::new();
	(cwd, fields).hash(&mut hash);
	// Different layouts cannot mistake fields never requested for freshly fetched zeroes.
	format!("git-{:x}", hash.finish())
}

fn cache_path(cwd: &str, fields: Fields) -> Option<PathBuf> {
	Some(
		crate::util::app_data_dir()
			.ok()?
			.join("cache")
			.join(cache_name(cwd, fields)),
	)
}

fn fresh(path: &Path) -> bool {
	fs::metadata(path)
		.and_then(|m| m.modified())
		.is_ok_and(|at| at.elapsed().is_ok_and(|age| age < MAX_AGE))
}

fn read(path: &Path) -> Option<GitCache> {
	serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub fn load(cwd: &str, segments: &[SegmentConfig]) -> Option<GitCache> {
	let fields = Fields::for_segments(segments);
	if cwd.is_empty() || fields == Fields::default() {
		return None;
	}
	let path = cache_path(cwd, fields)?;
	load_at(&path, || {
		if let Ok(exe) = std::env::current_exe() {
			spawn_refresh(&path, &exe, cwd, fields);
		}
	})
}

fn load_at(path: &Path, spawn: impl FnOnce()) -> Option<GitCache> {
	let cached = read(path);
	if cached.is_none() || !fresh(path) {
		spawn();
	}
	cached
}

fn spawn_refresh(path: &Path, exe: &Path, cwd: &str, fields: Fields) {
	let Some(lock) = claim(path, retry_interval(path)) else {
		return;
	};
	let mut command = Command::new(exe);
	command
		.arg("git-refresh")
		.arg("--cwd")
		.arg(cwd)
		.args(fields.args())
		.stdin(Stdio::from(lock))
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		command.process_group(0);
	}
	let _ = command.spawn();
}

pub fn refresh(cwd: &str, fields: Fields) {
	if cwd.is_empty() || fields == Fields::default() {
		return;
	}
	let Some(path) = cache_path(cwd, fields) else {
		return;
	};
	match fs::metadata(cwd) {
		Ok(m) if !m.is_dir() => {
			write(&path, &GitCache::default());
			clear_retry(&path);
			return;
		}
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			write(&path, &GitCache::default());
			clear_retry(&path);
			return;
		}
		_ => {}
	}
	let deadline = Instant::now() + REFRESH_BUDGET;
	refresh_at(&path, fields, |args| {
		let mut command = Command::new("git");
		command
			.arg("--no-optional-locks")
			.args(args)
			.current_dir(cwd);
		// A status display must never prompt or fetch missing objects from a partial clone.
		command
			.env("GIT_TERMINAL_PROMPT", "0")
			.env("GIT_NO_LAZY_FETCH", "1");
		let deadline = if args.first() == Some(&"symbolic-ref") {
			deadline.min(Instant::now() + BRANCH_BUDGET)
		} else {
			deadline
		};
		bounded_output(&mut command, deadline)
	});
}

fn refresh_at(path: &Path, fields: Fields, mut git: impl FnMut(&[&str]) -> Option<Output>) {
	let cached = read(path);
	// Publish a cheap branch lookup before a cold or previously timed-out scan. Healthy warm refreshes still use
	// only one status command. Preserve the last dirty/counter values only while the branch is unchanged.
	if fields.dirty
		&& fields.branch
		&& (cached.is_none() || retry_interval(path) > MAX_AGE)
		&& let Some(mut seed) = collect(
			Fields {
				branch: true,
				..Fields::default()
			},
			&mut git,
		) {
		if seed.branch.is_none() {
			write(path, &seed);
			clear_retry(path);
			return;
		}
		if let Some(old) = cached
			&& old.branch == seed.branch
		{
			seed = old;
		}
		write(path, &seed);
	}
	if let Some(cache) = collect(fields, git) {
		write(path, &cache);
		clear_retry(path);
	} else {
		let interval = retry_interval(path)
			.saturating_mul(2)
			.clamp(RETRY_MIN, RETRY_MAX);
		let _ = fs::write(
			path.with_extension("backoff"),
			interval.as_secs().to_string(),
		);
	}
}

fn retry_interval(path: &Path) -> Duration {
	fs::read_to_string(path.with_extension("backoff"))
		.ok()
		.and_then(|s| s.parse::<u64>().ok())
		.map_or(MAX_AGE, |secs| {
			Duration::from_secs(secs).clamp(RETRY_MIN, RETRY_MAX)
		})
}

fn clear_retry(path: &Path) {
	let _ = fs::remove_file(path.with_extension("backoff"));
}

fn write(path: &Path, cache: &GitCache) {
	let Ok(json) = serde_json::to_vec(cache) else {
		return;
	};
	let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
	// Readers see either complete result, even while another session refreshes it.
	if fs::write(&tmp, json)
		.and_then(|()| fs::rename(&tmp, path))
		.is_err()
	{
		let _ = fs::remove_file(tmp);
	}
}

fn collect(fields: Fields, mut git: impl FnMut(&[&str]) -> Option<Output>) -> Option<GitCache> {
	if fields.dirty {
		// Status already knows the branch and can count upstream/stash entries. Disabling rename detection
		// preserves the dirty bit without its extra work.
		let mut args = vec![
			"status",
			"--porcelain=v2",
			"-z",
			"--branch",
			"--no-renames",
			"--ignore-submodules=dirty",
		];
		if fields.ahead_behind {
			args.push("--ahead-behind");
		} else {
			args.push("--no-ahead-behind");
		}
		if fields.stash {
			args.push("--show-stash");
		}
		let out = git(&args)?;
		return match out.status.code() {
			Some(0) => {
				let (mut cache, has_stash) = parse_status(&out.stdout);
				// Git before 2.35 accepts --show-stash but omits the porcelain-v2 stash header.
				if fields.stash && !has_stash {
					cache.stash_count = count_stash(&mut git)?;
				}
				Some(cache)
			}
			// An explicit Git error (e.g. a removed repo) invalidates the old display. Timeouts/launch failures
			// return None from the runner and keep the previous cache instead.
			Some(_) => Some(GitCache::default()),
			None => None,
		};
	}

	let mut cache = GitCache::default();
	if fields.branch {
		// This never examines tracked or untracked files and works before the first commit.
		let out = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
		cache.branch = match out.status.code() {
			Some(0) => Some(String::from_utf8(out.stdout).ok()?.trim().to_owned()),
			Some(1) => Some("HEAD".to_owned()), // Detached, matching the previous display.
			Some(_) => return Some(GitCache::default()),
			None => return None,
		};
	}
	if fields.ahead_behind {
		let out = git(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])?;
		// No upstream or no commits yet is a valid zero.
		if out.status.success() {
			let text = String::from_utf8(out.stdout).ok()?;
			let (ahead, behind) = text.trim().split_once('\t')?;
			cache.ahead = ahead.parse().ok()?;
			cache.behind = behind.parse().ok()?;
		}
	}
	if fields.stash {
		cache.stash_count = count_stash(&mut git)?;
	}
	Some(cache)
}

fn count_stash(git: &mut impl FnMut(&[&str]) -> Option<Output>) -> Option<u64> {
	let out = git(&["rev-list", "--walk-reflogs", "--count", "refs/stash"])?;
	if out.status.success() {
		String::from_utf8(out.stdout).ok()?.trim().parse().ok()
	} else {
		Some(0)
	}
}

fn parse_status(output: &[u8]) -> (GitCache, bool) {
	let mut cache = GitCache::default();
	let mut has_stash = false;
	// NUL records keep filenames containing newlines from being mistaken for metadata.
	for record in output.split(|&b| b == 0) {
		if let Some(branch) = record.strip_prefix(b"# branch.head ") {
			let branch = String::from_utf8_lossy(branch);
			cache.branch = Some(if branch == "(detached)" {
				"HEAD".to_owned()
			} else {
				branch.into_owned()
			});
		} else if let Some(counts) = record.strip_prefix(b"# branch.ab ") {
			let text = String::from_utf8_lossy(counts);
			if let Some((ahead, behind)) = text.split_once(' ') {
				cache.ahead = ahead.trim_start_matches('+').parse().unwrap_or(0);
				cache.behind = behind.trim_start_matches('-').parse().unwrap_or(0);
			}
		} else if let Some(count) = record.strip_prefix(b"# stash ") {
			has_stash = true;
			cache.stash_count = String::from_utf8_lossy(count).parse().unwrap_or(0);
		} else if matches!(record.first(), Some(b'1' | b'2' | b'u' | b'?')) {
			cache.dirty = true;
		}
	}
	(cache, has_stash)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs::File;
	use std::sync::atomic::{AtomicU64, Ordering};
	use std::time::SystemTime;

	struct Scratch(PathBuf);

	impl Scratch {
		fn new() -> Self {
			static NEXT: AtomicU64 = AtomicU64::new(0);
			let dir = std::env::temp_dir().join(format!(
				"statusline-git-{}-{}",
				std::process::id(),
				NEXT.fetch_add(1, Ordering::Relaxed)
			));
			fs::create_dir_all(&dir).unwrap();
			Self(dir)
		}

		fn git(&self, args: &[&str]) -> Output {
			let output = Command::new("git")
				.arg("--no-optional-locks")
				.args(args)
				.current_dir(&self.0)
				.env("GIT_CONFIG_NOSYSTEM", "1")
				.env("GIT_CONFIG_GLOBAL", "/dev/null")
				.output()
				.unwrap();
			assert!(output.status.success(), "{args:?}: {output:?}");
			output
		}

		fn init(&self) {
			self.git(&["init", "-q", "-b", "main"]);
			self.git(&["config", "user.name", "Statusline Test"]);
			self.git(&["config", "user.email", "statusline@example.test"]);
			self.git(&["config", "commit.gpgsign", "false"]);
		}

		fn collect(&self, fields: Fields) -> GitCache {
			collect(fields, |args| Some(self.git(args))).unwrap()
		}
	}

	impl Drop for Scratch {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	fn fields(json: &str) -> Fields {
		Fields::for_segments(&serde_json::from_str::<Vec<SegmentConfig>>(json).unwrap())
	}

	fn age(path: &Path) {
		File::open(path)
			.unwrap()
			.set_modified(SystemTime::now() - MAX_AGE * 2)
			.unwrap();
	}

	fn wait_until(mut ready: impl FnMut() -> bool) {
		let deadline = Instant::now() + Duration::from_secs(3);
		while !ready() {
			assert!(Instant::now() < deadline, "background work did not settle");
			std::thread::sleep(Duration::from_millis(10));
		}
	}

	#[test]
	fn only_enabled_git_segments_request_work() {
		assert_eq!(
			fields(r#"["cwd", "pr", "repo", {"type":"git_branch","enabled":false,"dirty":true}]"#),
			Fields::default()
		);
		let branch = fields(r#"["git_branch"]"#);
		assert_eq!(
			branch,
			Fields {
				branch: true,
				..Fields::default()
			}
		);
		assert!(!fields(r#"[{"type":"git_branch","dirty":""}]"#).dirty);
		assert!(fields(r#"[{"type":"git_branch","dirty":"!"}]"#).dirty);
		assert!(load("/unused", &[]).is_none());
		// A newly enabled counter must not reuse an entry that never fetched it.
		assert_ne!(
			cache_name("/repo", branch),
			cache_name("/repo", fields(r#"["git_branch","git_stash"]"#))
		);
	}

	#[test]
	fn branch_only_never_scans_the_worktree() {
		let repo = Scratch::new();
		repo.init();
		fs::write(repo.0.join("untracked"), "data").unwrap();
		let mut calls = Vec::new();
		let cache = collect(fields(r#"["git_branch"]"#), |args| {
			calls.push(args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
			Some(repo.git(args))
		})
		.unwrap();
		assert_eq!(cache.branch.as_deref(), Some("main"));
		assert!(!cache.dirty);
		assert_eq!(
			calls,
			vec![vec!["symbolic-ref", "--quiet", "--short", "HEAD"]]
		);
	}

	#[test]
	fn dirty_layout_combines_requested_fields_in_one_status() {
		let repo = Scratch::new();
		repo.init();
		repo.git(&["commit", "-q", "--allow-empty", "-m", "initial"]);
		repo.git(&["branch", "upstream"]);
		repo.git(&["branch", "--set-upstream-to=upstream"]);
		repo.git(&["commit", "-q", "--allow-empty", "-m", "ahead"]);
		fs::write(repo.0.join("stash-me"), "data").unwrap();
		repo.git(&["stash", "push", "-qu"]);
		fs::write(repo.0.join("odd\n# branch.head false"), "data").unwrap();

		let mut calls = Vec::new();
		let cache = collect(
			fields(r#"[{"type":"git_branch","dirty":true},"git_ahead_behind","git_stash"]"#),
			|args| {
				calls.push(args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
				Some(repo.git(args))
			},
		)
		.unwrap();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0][0], "status");
		assert!(!calls[0].iter().any(|s| s == "--no-ahead-behind"));
		assert_eq!(cache.branch.as_deref(), Some("main"));
		assert!(cache.dirty);
		assert_eq!((cache.ahead, cache.behind, cache.stash_count), (1, 0, 1));

		// Older Git omits the stash header even when --show-stash is accepted.
		let legacy = collect(
			fields(r#"[{"type":"git_branch","dirty":true},"git_stash"]"#),
			|args| {
				let mut out = repo.git(args);
				if args[0] == "status" {
					out.stdout = out
						.stdout
						.split(|&b| b == 0)
						.filter(|record| !record.starts_with(b"# stash "))
						.flat_map(|record| record.iter().copied().chain(std::iter::once(0)))
						.collect();
				}
				Some(out)
			},
		)
		.unwrap();
		assert_eq!(legacy.stash_count, 1);

		// Without the dirty indicator, no status scan is needed, even when counters are shown.
		let counters = repo.collect(fields(r#"["git_ahead_behind","git_stash"]"#));
		assert_eq!(
			(counters.ahead, counters.behind, counters.stash_count),
			(1, 0, 1)
		);
		assert!(counters.branch.is_none());
	}

	#[test]
	fn dirty_only_skips_upstream_and_stash_work_and_handles_unborn_and_detached() {
		let repo = Scratch::new();
		repo.init();
		let wanted = fields(r#"[{"type":"git_branch","dirty":true}]"#);
		let cache = collect(wanted, |args| {
			assert!(args.contains(&"--no-ahead-behind"));
			assert!(!args.contains(&"--show-stash"));
			Some(repo.git(args))
		})
		.unwrap();
		assert_eq!(cache.branch.as_deref(), Some("main"));
		assert!(!cache.dirty);
		repo.git(&["commit", "-q", "--allow-empty", "-m", "initial"]);
		repo.git(&["checkout", "-q", "--detach"]);
		assert_eq!(repo.collect(wanted).branch.as_deref(), Some("HEAD"));
		// symbolic-ref returns 1 for a detached HEAD, which is an expected result.
		let cache = collect(fields(r#"["git_branch"]"#), |args| {
			Some(
				Command::new("git")
					.args(args)
					.current_dir(&repo.0)
					.output()
					.unwrap(),
			)
		})
		.unwrap();
		assert_eq!(cache.branch.as_deref(), Some("HEAD"));
	}

	#[test]
	fn fresh_cache_needs_no_worker_and_stale_data_is_kept_while_refreshing() {
		let dir = Scratch::new();
		let path = dir.0.join("cache");
		write(
			&path,
			&GitCache {
				branch: Some("last-good".to_owned()),
				..GitCache::default()
			},
		);
		assert_eq!(
			load_at(&path, || panic!("fresh cache launched a worker"))
				.unwrap()
				.branch
				.as_deref(),
			Some("last-good")
		);
		age(&path);
		let mut requested = false;
		let cached = load_at(&path, || {
			requested = true;
		})
		.unwrap();
		assert!(requested);
		assert_eq!(cached.branch.as_deref(), Some("last-good"));
		assert!(
			load_at(&dir.0.join("cold"), || {
				requested = true;
			})
			.is_none()
		);
	}

	#[test]
	fn cold_slow_scans_publish_the_branch_and_back_off_until_recovery() {
		let repo = Scratch::new();
		repo.init();
		let dir = Scratch::new();
		let path = dir.0.join("cache");
		let wanted = fields(r#"[{"type":"git_branch","dirty":true}]"#);
		let mut branches = 0;
		for seconds in [30, 60, 120, 240, 300, 300] {
			refresh_at(&path, wanted, |args| {
				if args[0] == "symbolic-ref" {
					branches += 1;
					Some(repo.git(args))
				} else {
					// The status deadline expires, but its branch was already published.
					assert_eq!(read(&path).unwrap().branch.as_deref(), Some("main"));
					None
				}
			});
			assert_eq!(retry_interval(&path), Duration::from_secs(seconds));
		}
		assert_eq!(branches, 6);
		refresh_at(&path, wanted, |args| Some(repo.git(args)));
		assert_eq!(retry_interval(&path), MAX_AGE);
		let mut calls = Vec::new();
		refresh_at(&path, wanted, |args| {
			calls.push(args[0].to_owned());
			Some(repo.git(args))
		});
		assert_eq!(
			calls,
			["status"],
			"healthy warm scans need no extra branch lookup"
		);
	}

	#[test]
	fn dirty_scans_respect_the_repositories_untracked_file_setting() {
		let repo = Scratch::new();
		repo.init();
		repo.git(&["config", "status.showUntrackedFiles", "no"]);
		fs::write(repo.0.join("untracked"), "data").unwrap();
		assert!(
			!repo
				.collect(fields(r#"[{"type":"git_branch","dirty":true}]"#))
				.dirty
		);
	}

	#[test]
	fn failed_refreshes_back_off_and_live_workers_cannot_be_replaced() {
		let dir = Scratch::new();
		let path = dir.0.join("cache");
		let lock = claim(&path, MAX_AGE).unwrap();
		assert!(claim(&path, MAX_AGE).is_none());
		age(&path.with_extension("refresh"));
		assert!(
			claim(&path, MAX_AGE).is_none(),
			"an old timestamp cannot supersede a live lock"
		);
		drop(lock);
		// Another test may be forking at this instant, briefly inheriting our descriptor before exec closes it.
		wait_until(|| claim(&path, MAX_AGE).is_some());
		assert!(
			claim(&path, MAX_AGE).is_none(),
			"a failed refresh must still back off"
		);
	}

	#[cfg(unix)]
	#[test]
	fn worker_inherits_lock_without_holding_up_the_render() {
		use std::os::unix::fs::PermissionsExt;

		let dir = Scratch::new();
		let path = dir.0.join("cache");
		let exe = dir.0.join("worker");
		fs::write(
			&exe,
			"#!/bin/sh\n: > \"$3/started\"\nwhile [ ! -f \"$3/release\" ]; do sleep 0.01; done\n",
		)
		.unwrap();
		fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
		write(
			&path,
			&GitCache {
				branch: Some("last-good".to_owned()),
				..GitCache::default()
			},
		);
		age(&path);
		let cached = load_at(&path, || {
			spawn_refresh(
				&path,
				&exe,
				dir.0.to_str().unwrap(),
				fields(r#"["git_branch"]"#),
			);
		});
		// The worker cannot exit until the test releases it, yet the renderer already has its cached result.
		assert_eq!(cached.unwrap().branch.as_deref(), Some("last-good"));
		wait_until(|| dir.0.join("started").exists());
		age(&path.with_extension("refresh"));
		assert!(
			claim(&path, MAX_AGE).is_none(),
			"the parent's exit must not release the worker's lock"
		);
		fs::write(dir.0.join("release"), "").unwrap();
		wait_until(|| claim(&path, MAX_AGE).is_some());
	}

	#[cfg(unix)]
	#[test]
	fn slow_commands_and_inherited_output_pipes_have_a_deadline() {
		let start = Instant::now();
		let deadline = start + Duration::from_millis(150);
		assert!(
			bounded_output(
				Command::new("/bin/sh").args(["-c", "exec sleep 5"]),
				deadline
			)
			.is_none()
		);
		assert!(start.elapsed() < Duration::from_secs(2));

		// A child that has exited may leave its pipe open in a descendant.
		let start = Instant::now();
		assert!(
			bounded_output(
				Command::new("/bin/sh").args(["-c", "sleep 0.4 &"]),
				start + Duration::from_millis(100)
			)
			.is_none()
		);
		assert!(start.elapsed() < Duration::from_secs(2));
	}

	#[test]
	fn timeout_keeps_the_last_good_result() {
		let dir = Scratch::new();
		let path = dir.0.join("cache");
		write(
			&path,
			&GitCache {
				branch: Some("last-good".to_owned()),
				..GitCache::default()
			},
		);
		refresh_at(
			&path,
			fields(r#"[{"type":"git_branch","dirty":true}]"#),
			|_| None,
		);
		assert_eq!(read(&path).unwrap().branch.as_deref(), Some("last-good"));
	}

	#[test]
	fn removing_a_repository_clears_its_cached_branch_and_counters() {
		let repo = Scratch::new();
		repo.init();
		let dir = Scratch::new();
		let path = dir.0.join("cache");
		fs::remove_dir_all(repo.0.join(".git")).unwrap();
		for layout in [
			r#"["git_branch"]"#,
			r#"[{"type":"git_branch","dirty":true}]"#,
		] {
			write(
				&path,
				&GitCache {
					branch: Some("old-repo".into()),
					dirty: true,
					ahead: 2,
					stash_count: 1,
					..GitCache::default()
				},
			);
			refresh_at(&path, fields(layout), |args| {
				Some(
					Command::new("git")
						.args(args)
						.current_dir(&repo.0)
						.output()
						.unwrap(),
				)
			});
			let cache = read(&path).unwrap();
			assert!(cache.branch.is_none());
			assert!(!cache.dirty);
			assert_eq!((cache.ahead, cache.stash_count), (0, 0));
		}
	}
}
