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

#[test]
fn plugin_reports_only_enabled_optional_data_including_account_overrides() {
	let scratch = scratch_home("plugin-needs");
	let settings_path = scratch.join(".statusline/settings.json");
	let mut settings: Value =
		serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
	settings["segments"] = json!(["five_hour", "seven_day",
		{"type": "extra_usage", "enabled": false}, {"type": "autocompact_headroom", "enabled": false}]);
	fs::write(&settings_path, settings.to_string()).unwrap();
	let args = ["--format", "spans", "--plugin-data"];
	let input = br#"{"mod":{"usage":{"body":null}}}"#;
	let output: Value = serde_json::from_slice(&run(&scratch, &args, input).stdout).unwrap();
	assert_eq!(output["needs"], json!({"usage":false,"autocompact":false}));

	settings["segments"] = json!(["credits", "autocompact_headroom"]);
	fs::write(&settings_path, settings.to_string()).unwrap();
	let output: Value = serde_json::from_slice(&run(&scratch, &args, input).stdout).unwrap();
	assert_eq!(output["needs"], json!({"usage":true,"autocompact":true}));

	fs::write(
		scratch.join(".claude.json"),
		r#"{"oauthAccount":{"emailAddress":"test@example.test","organizationUuid":"test-org"}}"#,
	)
	.unwrap();
	fs::write(scratch.join(".statusline/accounts.json"), r#"{"accounts":[{"nickname":"test","email":"test@example.test","organization_uuid":"test-org","segments":["model"]}]}"#).unwrap();
	let output: Value = serde_json::from_slice(&run(&scratch, &args, input).stdout).unwrap();
	assert_eq!(output["needs"], json!({"usage":false,"autocompact":false}));
	fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn plugin_update_notice_requires_an_empty_chat_and_enabled_checks() {
	let scratch = scratch_home("plugin-updates");
	let settings_path = scratch.join(".statusline/settings.json");
	let mut settings: Value =
		serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
	settings["skip_update_check"] = false.into();
	fs::write(&settings_path, settings.to_string()).unwrap();
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_secs();
	fs::write(
		scratch.join(".statusline/latest_version"),
		format!("99.0.0\n{now}"),
	)
	.unwrap();
	let args = ["--format", "spans", "--plugin-data"];
	let empty = br#"{"mod":{"show_update":true}}"#;
	let output: Value = serde_json::from_slice(&run(&scratch, &args, empty).stdout).unwrap();
	assert!(
		output["update"]
			.as_str()
			.unwrap()
			.contains("v99.0.0 available")
	);
	assert!(output["rows"].is_array());

	// Zero context usage is not evidence that no user message has been sent.
	let output: Value = serde_json::from_slice(
		&run(
			&scratch,
			&args,
			br#"{"context_window":{"used_percentage":0},"mod":{"show_update":false}}"#,
		)
		.stdout,
	)
	.unwrap();
	assert!(output["update"].is_null());

	settings["skip_update_check"] = true.into();
	fs::write(&settings_path, settings.to_string()).unwrap();
	let output: Value = serde_json::from_slice(&run(&scratch, &args, empty).stdout).unwrap();
	assert!(output["update"].is_null());
	fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn git_cache_refreshes_in_the_background_and_clears_removed_directories() {
	let scratch = scratch_home("background-git");
	let repo = scratch.join("repo");
	fs::create_dir(&repo).unwrap();
	let output = Command::new("git")
		.args(["init", "-q", "-b", "background-test"])
		.current_dir(&repo)
		.output()
		.unwrap();
	assert!(output.status.success(), "{output:?}");
	fs::write(repo.join("untracked"), "dirty").unwrap();
	let settings_path = scratch.join(".statusline/settings.json");
	let mut settings: Value =
		serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
	settings["segments"] = json!([{"type": "git_branch", "dirty": true}]);
	fs::write(&settings_path, settings.to_string()).unwrap();
	let input = serde_json::to_vec(&json!({"cwd": repo})).unwrap();
	let args = ["--format", "spans"];
	let cold = run(&scratch, &args, &input);
	assert!(
		!String::from_utf8(cold.stdout)
			.unwrap()
			.contains("background-test")
	);
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
	loop {
		let warm = run(&scratch, &args, &input);
		let rows: Value = serde_json::from_slice(&warm.stdout).unwrap();
		let text = rows
			.as_array()
			.unwrap()
			.iter()
			.flat_map(|row| row.as_array().unwrap())
			.filter_map(|span| span["text"].as_str())
			.collect::<String>();
		if text.contains("background-test✱") {
			break;
		}
		assert!(std::time::Instant::now() < deadline, "{text}");
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
	fs::remove_dir_all(&repo).unwrap();
	run(
		&scratch,
		&[
			"git-refresh",
			"--cwd",
			repo.to_str().unwrap(),
			"--branch",
			"--dirty",
		],
		b"",
	);
	let cleared: Value = serde_json::from_slice(&run(&scratch, &args, &input).stdout).unwrap();
	assert_eq!(
		cleared,
		json!([[]]),
		"a removed worktree must not retain its old branch"
	);
	fs::remove_dir_all(scratch).unwrap();
}
