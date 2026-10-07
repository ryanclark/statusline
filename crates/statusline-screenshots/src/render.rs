use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eyre::{Result, WrapErr, bail, eyre};
use serde_json::{Map, Value, json};
use statusline_core::spans::Span;

use crate::repo;
use crate::scenario::{Clock, Scenario, resolve_times};

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
	let config = home.join(".statusline");
	fs::create_dir_all(&config)?;
	fs::write(
		config.join("settings.json"),
		serde_json::to_string(&settings)?,
	)?;

	let cwd = home.join(&scenario.cwd);
	repo::make(&cwd, &scenario.git)?;
	let cwd_str = cwd.to_string_lossy().into_owned();

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
