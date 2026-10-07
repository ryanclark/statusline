//! Runs the built binary the way Claude Code and the plugin do, against a scratch HOME.

use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FIXTURE: &str = include_str!("fixtures/input.json");

fn scratch_home(name: &str) -> PathBuf {
	let home = std::env::temp_dir().join(format!("statusline-it-{name}-{}", std::process::id()));
	let _ = fs::remove_dir_all(&home);
	fs::create_dir_all(home.join(".statusline")).unwrap();
	fs::write(
		home.join(".statusline/settings.json"),
		json!({
			"five_hour_reset_threshold": 70,
			"seven_day_reset_threshold": 100,
			"segments": [],
			"skip_update_check": true,
			"capture_snapshots": true
		})
		.to_string(),
	)
	.unwrap();
	home
}

fn run(home: &Path, args: &[&str], stdin: &[u8]) -> Output {
	let mut child = Command::new(env!("CARGO_BIN_EXE_statusline"))
		.args(args)
		.env("HOME", home)
		.env_remove("CLAUDECODE")
		.env_remove("CLAUDE_CODE_SESSION_ID")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.take().unwrap().write_all(stdin).unwrap();
	let output = child.wait_with_output().unwrap();
	assert!(output.status.success(), "{output:?}");
	output
}

/// Claude Code's own input, and the plugin's reconstruction of it, which has no model display name.
fn inputs(session_id: &str) -> (Vec<u8>, Vec<u8>) {
	let mut native: Value = serde_json::from_str(FIXTURE).unwrap();
	native["session_id"] = session_id.into();
	let mut spans = native.clone();
	spans["model"]["display_name"] = "".into();
	(
		serde_json::to_vec(&native).unwrap(),
		serde_json::to_vec(&spans).unwrap(),
	)
}

#[test]
fn a_silenced_native_run_still_captures_claude_codes_input() {
	let home = scratch_home("capture");
	let (native, spans) = inputs("capture-1");
	let snapshot = home.join(".statusline/sessions/capture-1.json");

	run(
		&home,
		&["--format", "spans", "--heartbeat-ms", "10000"],
		&spans,
	);
	assert_eq!(
		fs::read_to_string(&snapshot).unwrap(),
		String::from_utf8(spans.clone()).unwrap(),
		"with no native run yet the plugin's input is the only snapshot"
	);

	let out = run(&home, &[], &native);
	assert!(out.stdout.is_empty(), "the plugin draws this session");
	assert_eq!(
		fs::read_to_string(&snapshot).unwrap(),
		String::from_utf8(native.clone()).unwrap(),
		"the native run captures the real input even while silent"
	);

	run(
		&home,
		&["--format", "spans", "--heartbeat-ms", "10000"],
		&spans,
	);
	assert_eq!(
		fs::read_to_string(&snapshot).unwrap(),
		String::from_utf8(native.clone()).unwrap(),
		"the plugin must not overwrite a fresh native snapshot"
	);

	fs::remove_dir_all(&home).unwrap();
}
