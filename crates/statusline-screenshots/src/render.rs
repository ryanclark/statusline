use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eyre::{Result, WrapErr, bail, eyre};
use serde_json::{Map, Value, json};
use statusline_configure::Key;
use statusline_core::segment::{DirtyConfig, SegmentConfig, SegmentType, default_segments};
use statusline_core::settings::Settings;
use statusline_core::spans::{Span, ansi_to_spans};

use crate::repo;
use crate::scenario::{Clock, ConfigureSpec, Scenario, resolve_times};

const TIMEOUT: Duration = Duration::from_secs(30);

pub type Row = Vec<Span>;
pub type Rows = Vec<Row>;

/// What the binary drew for one frame, and the input it was given, which the page header reads.
pub struct Frame {
	pub line: Rows,
	pub panel: Rows,
	pub input: Map<String, Value>,
}

/// Runs the real binary for `scenario` under the scratch `home`, as of `now`.
pub fn frame(binary: &Path, scenario: &Scenario, home: &Path, now: Clock) -> Result<Frame> {
	let config = home.join(".statusline");
	fs::create_dir_all(&config)?;
	fs::write(
		config.join("settings.json"),
		serde_json::to_string(&settings(scenario))?,
	)?;

	let cwd = home.join(&scenario.cwd);
	repo::make(&cwd, &scenario.git)?;
	let cwd_str = cwd.to_string_lossy().into_owned();
	// A screenshot captures just one frame from a fresh HOME. Populate its Git cache before drawing that frame,
	// rather than capturing the intentionally empty first render of the live, asynchronous status line.
	warm_git(binary, scenario, home, &cwd_str)?;

	let mut input = scenario.input.clone();
	for value in input.values_mut() {
		resolve_times(value, now)?;
	}
	input.entry("session_id").or_insert_with(|| "demo".into());
	input.insert("cwd".to_owned(), cwd_str.clone().into());
	input
		.entry("workspace")
		.or_insert_with(|| json!({"current_dir": cwd_str, "project_dir": cwd_str}));

	let run = |args: &[&str], stdin: &Value| spans(binary, home, args, stdin);
	let line = run(&[], &Value::Object(input.clone()))?;
	let panel = match &scenario.tasks {
		Some(tasks) => {
			let mut tasks = tasks.clone();
			resolve_times(&mut tasks, now)?;
			let stdin =
				json!({"session_id": "demo", "cwd": cwd_str, "columns": 160, "tasks": tasks});
			run(&["subagent"], &stdin)?
		}
		None => Vec::new(),
	};
	Ok(Frame { line, panel, input })
}

fn warm_git(binary: &Path, scenario: &Scenario, home: &Path, cwd: &str) -> Result<()> {
	let segments: Option<Vec<SegmentConfig>> = serde_json::from_value(scenario.segments.clone())?;
	let mut branch = false;
	let mut dirty = false;
	let mut ahead = false;
	let mut stash = false;
	for segment in segments
		.unwrap_or_else(default_segments)
		.iter()
		.filter(|s| s.enabled())
	{
		match segment.segment_type() {
			SegmentType::GitBranch => {
				branch = true;
				if let SegmentConfig::Advanced(options) = segment {
					dirty |= match &options.dirty {
						DirtyConfig::Off => false,
						DirtyConfig::On => true,
						DirtyConfig::Custom(text) => !text.is_empty(),
					};
				}
			}
			SegmentType::GitAheadBehind => ahead = true,
			SegmentType::GitStash => stash = true,
			_ => {}
		}
	}
	let flags: Vec<_> = [
		(branch, "--branch"),
		(dirty, "--dirty"),
		(ahead, "--ahead-behind"),
		(stash, "--stash"),
	]
	.into_iter()
	.filter_map(|(needed, flag)| needed.then_some(flag))
	.collect();
	if !flags.is_empty() {
		let cache = home.join(".statusline/cache");
		fs::create_dir_all(&cache)?;
		let mut args = vec!["git-refresh", "--cwd", cwd];
		args.extend(flags);
		spans(binary, home, &args, &json!({}))?;
		let ready = fs::read_dir(cache)?.filter_map(Result::ok).any(|e| {
			e.file_name().to_string_lossy().starts_with("git-") && e.path().extension().is_none()
		});
		if !ready {
			bail!("Git refresh did not populate the screenshot's cache");
		}
	}
	Ok(())
}

/// What `statusline configure` paints for `scenario`, drawn in process since the editor needs no session or repo.
pub fn configure(scenario: &Scenario, spec: &ConfigureSpec, work: &Path) -> Result<Rows> {
	let settings: Settings = serde_json::from_value(Value::Object(settings(scenario)))
		.wrap_err("parsing the scenario settings")?;
	// A wired subagent line, as on a real install, so the tab row carries no "not installed" notice.
	let claude = work.join("claude-settings.json");
	let subagent =
		json!({ "subagentStatusLine": { "type": "command", "command": "statusline subagent" } });
	fs::write(&claude, subagent.to_string()).wrap_err("writing the Claude settings")?;
	let keys: Vec<Key> = spec.keys.iter().map(|&name| name.into()).collect();
	let text =
		statusline_configure::snapshot(&settings, Some(&claude), &keys, spec.width, spec.rows)?;
	Ok(ansi_to_spans(&text))
}

fn settings(scenario: &Scenario) -> Map<String, Value> {
	let mut settings = Map::new();
	settings.insert("nerd_font".to_owned(), true.into());
	settings.insert("skip_update_check".to_owned(), true.into());
	settings.insert("five_hour_reset_threshold".to_owned(), 70.into());
	settings.insert("seven_day_reset_threshold".to_owned(), 100.into());
	settings.extend(scenario.settings.clone());
	settings.insert("segments".to_owned(), scenario.segments.clone());
	if let Some(segments) = &scenario.subagent_segments {
		settings.insert("subagent_segments".to_owned(), segments.clone());
	}
	settings
}

fn spans(binary: &Path, home: &Path, args: &[&str], stdin: &Value) -> Result<Rows> {
	let mut child = Command::new(binary)
		.args(["--format", "spans"])
		.args(args)
		.env("HOME", home)
		// The scratch HOME is what keeps the real Claude config out of reach, so nothing may point back at it.
		.env_remove("CLAUDE_CONFIG_DIR")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.wrap_err_with(|| format!("running {}", binary.display()))?;
	let mut pipe = child.stdin.take().expect("stdin is piped");
	pipe.write_all(serde_json::to_string(stdin)?.as_bytes())?;
	drop(pipe);

	// Both pipes drain on threads of their own, since output past the pipe buffer would otherwise block the child
	// and look like a hang.
	let stdout = drain(child.stdout.take().expect("stdout is piped"));
	let stderr = drain(child.stderr.take().expect("stderr is piped"));
	let started = Instant::now();
	let status = loop {
		if let Some(status) = child.try_wait()? {
			break status;
		}
		if started.elapsed() >= TIMEOUT {
			let _ = child.kill();
			let _ = child.wait();
			bail!(
				"statusline {} did not exit within {TIMEOUT:?}",
				args.join(" ")
			);
		}
		thread::sleep(Duration::from_millis(10));
	};
	let stdout = collect(stdout)?;
	let stderr = collect(stderr)?;
	if !status.success() {
		bail!(
			"statusline {} failed: {}",
			args.join(" "),
			String::from_utf8_lossy(&stderr)
		);
	}
	if stdout.trim_ascii().is_empty() {
		return Ok(Vec::new());
	}
	serde_json::from_slice(&stdout).wrap_err("parsing statusline spans")
}

fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<io::Result<Vec<u8>>> {
	thread::spawn(move || {
		let mut buf = Vec::new();
		pipe.read_to_end(&mut buf).map(|_| buf)
	})
}

fn collect(handle: JoinHandle<io::Result<Vec<u8>>>) -> Result<Vec<u8>> {
	handle
		.join()
		.map_err(|_| eyre!("reading statusline output panicked"))?
		.wrap_err("reading statusline output")
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Renders a configure scenario as plain text, printed so `--nocapture` shows the frame the screenshot draws.
	fn configure_text(name: &str) -> Vec<String> {
		let path = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join(format!("../../screenshots/scenarios/{name}.json"));
		let scenario = Scenario::load(&path).unwrap();
		let work = std::env::temp_dir().join(format!(
			"statusline-screenshots-test-{name}-{}",
			std::process::id()
		));
		fs::create_dir_all(&work).unwrap();
		let rows = configure(&scenario, scenario.configure.as_ref().unwrap(), &work).unwrap();
		fs::remove_dir_all(&work).unwrap();
		let lines: Vec<String> = rows
			.iter()
			.map(|row| row.iter().map(|span| span.text.as_str()).collect())
			.collect();
		println!("{name}:\n{}", lines.join("\n"));
		lines
	}

	fn cursor_row(lines: &[String]) -> &str {
		lines
			.iter()
			.find(|line| line.contains('\u{276f}'))
			.expect("a row carries the cursor marker")
	}

	#[test]
	fn configure_scenarios_land_on_their_segments() {
		let lines = configure_text("configure");
		assert!(cursor_row(&lines).contains("git_branch"), "{lines:#?}");
		assert!(
			!lines.iter().any(|line| line.contains(" more")),
			"{lines:#?}"
		);

		let lines = configure_text("configure-options");
		let cursor = cursor_row(&lines);
		assert!(
			cursor.contains("cache_warm") && cursor.contains('\u{25be}'),
			"{lines:#?}"
		);
		assert!(
			lines.iter().any(|line| line.contains('\u{2514}')),
			"{lines:#?}"
		);
	}
}
