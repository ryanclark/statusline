//! Public external-source protocol exercised through the actual binary, with private scratch state.
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

fn scratch(name: &str) -> PathBuf {
	let root =
		std::env::temp_dir().join(format!("statusline-external-{name}-{}", std::process::id()));
	let _ = fs::remove_dir_all(&root);
	fs::create_dir_all(root.join(".statusline")).unwrap();
	root
}
fn source() -> Value {
	json!({"command":["python3",concat!(env!("CARGO_MANIFEST_DIR"),"/tests/support/usage_provider.py"),"literal ; $(echo unsafe)"],"key_env":"TEST_ACCOUNT_KEY"})
}
fn request(key: &str) -> Value {
	json!({"version":1,"key":key,"session_id":"conversation-1","model_id":"model-1"})
}
fn worker(root: &Path, key: &str, mode: &str) -> Command {
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_statusline"));
	cmd.args([
		"external-usage-refresh",
		"--source",
		&source().to_string(),
		"--request",
		&request(key).to_string(),
	])
	.env("HOME", root)
	.env("TEST_PROVIDER_MODE", mode)
	.stdout(Stdio::piped())
	.stderr(Stdio::piped());
	cmd
}
fn refresh(root: &Path, key: &str, mode: &str) -> Output {
	worker(root, key, mode).output().unwrap()
}
fn render(root: &Path, key: &str, spans: bool) -> Output {
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_statusline"));
	if spans {
		cmd.args(["--format", "spans", "--plugin-data"]);
	}
	let mut child = cmd
		.env("HOME", root)
		.env("STATUSLINE_USAGE_SOURCE", source().to_string())
		.env("TEST_ACCOUNT_KEY", key)
		.env_remove("CLAUDECODE")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.take().unwrap().write_all(br#"{"session_id":"conversation-1","model":{"id":"model-1"},"rate_limits":{"five_hour":{"used_percentage":99,"resets_at":9999999999}}}"#).unwrap();
	child.wait_with_output().unwrap()
}
fn settings(root: &Path) {
	fs::write(root.join(".statusline/settings.json"),json!({"five_hour_reset_threshold":70,"seven_day_reset_threshold":100,"skip_update_check":true,"segments":[{"type":"account","capitalize":false},{"type":"usage_limit","limit_id":"custom","show_countdown":false,"colors":false},"five_hour","model_id"]}).to_string()).unwrap();
}
fn caches(root: &Path) -> Vec<PathBuf> {
	fs::read_dir(root.join(".statusline/external-usage"))
		.unwrap()
		.flatten()
		.map(|e| e.path())
		.filter(|p| {
			p.extension().is_some_and(|s| s == "json")
				&& !p
					.file_name()
					.unwrap()
					.to_string_lossy()
					.ends_with(".attempt.json")
		})
		.collect()
}
fn expire_attempts(root: &Path) {
	for entry in fs::read_dir(root.join(".statusline/external-usage"))
		.unwrap()
		.flatten()
	{
		if entry.path().extension().is_some_and(|s| s == "refresh") {
			fs::File::open(entry.path())
				.unwrap()
				.set_modified(SystemTime::UNIX_EPOCH)
				.unwrap();
		}
	}
}

#[test]
fn external_usage_literal_argv_stdin_cache_and_native_plugin_precedence() {
	let root = scratch("protocol");
	settings(&root);
	let out = refresh(&root, "alpha", "");
	assert!(out.status.success(), "{out:?}");
	let calls = fs::read_to_string(root.join(".statusline/provider-calls")).unwrap();
	let call: Value = serde_json::from_str(calls.trim()).unwrap();
	assert_eq!(call["request"], request("alpha"));
	assert_eq!(call["args"], json!(["literal ; $(echo unsafe)"]));
	for spans in [false, true] {
		let start = Instant::now();
		let out = render(&root, "alpha", spans);
		assert!(start.elapsed() < Duration::from_secs(1));
		assert!(out.status.success());
		let text = String::from_utf8(out.stdout).unwrap();
		assert!(
			text.contains("alpha") && text.contains("Burst 42%") && text.contains("model-1"),
			"{text}"
		);
		assert!(!text.contains("99%"), "native limits leaked: {text}");
		if spans {
			let value: Value = serde_json::from_str(&text).unwrap();
			assert_eq!(value["needs"]["usage"], false);
		}
	}
	assert_eq!(caches(&root).len(), 1);
	fs::remove_dir_all(root).unwrap();
}
#[test]
fn external_usage_keys_and_locks_are_isolated_and_failed_attempts_keep_observation_time() {
	let root = scratch("isolation");
	settings(&root);
	let mut workers: Vec<_> = (0..6)
		.map(|_| worker(&root, "alpha", "slow").spawn().unwrap())
		.collect();
	for child in &mut workers {
		assert!(child.wait().unwrap().success());
	}
	assert_eq!(
		fs::read_to_string(root.join(".statusline/provider-calls"))
			.unwrap()
			.lines()
			.count(),
		1
	);
	let path = caches(&root).pop().unwrap();
	let before = fs::read(&path).unwrap();
	for mode in ["fail", "malformed", "large"] {
		expire_attempts(&root);
		assert!(!refresh(&root, "alpha", mode).status.success());
		assert_eq!(fs::read(&path).unwrap(), before);
	}
	assert!(refresh(&root, "beta", "").status.success());
	assert_eq!(caches(&root).len(), 2);
	let out = String::from_utf8(render(&root, "beta", false).stdout).unwrap();
	assert!(out.contains("beta") && !out.contains("alpha"), "{out}");
	fs::remove_dir_all(root).unwrap();
}
#[test]
fn external_usage_timeout_stops_children_and_is_backed_off() {
	let root = scratch("timeout");
	let start = Instant::now();
	assert!(!refresh(&root, "alpha", "hang").status.success());
	assert!(start.elapsed() < Duration::from_secs(4));
	std::thread::sleep(Duration::from_secs(1));
	assert!(
		!root.join(".statusline/leaked").exists(),
		"descendant escaped: {:?}",
		fs::read_to_string(root.join(".statusline/process-group"))
	);
	assert!(refresh(&root, "alpha", "").status.success());
	assert!(caches(&root).is_empty());
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_usage_pending_and_invalid_sources_never_restore_another_login() {
	let root = scratch("pending");
	settings(&root);
	fs::write(
		root.join(".claude.json"),
		r#"{"oauthAccount":{"emailAddress":"old@example.test","organizationUuid":"old-org"}}"#,
	)
	.unwrap();
	fs::write(root.join(".statusline/accounts.json"),r#"{"accounts":[{"nickname":"old-login","email":"old@example.test","organization_uuid":"old-org","segments":["account"]}]}"#).unwrap();
	for source in [
		json!({"command":["does-not-exist"],"key_env":"TEST_ACCOUNT_KEY"}),
		json!({"command":"invalid"}),
		json!({"command":["unused"],"timeout":"0s"}),
	] {
		let mut child = Command::new(env!("CARGO_BIN_EXE_statusline"))
			.args(["--format", "spans", "--plugin-data"])
			.env("HOME", &root)
			.env("TEST_ACCOUNT_KEY", "new-account")
			.env("STATUSLINE_USAGE_SOURCE", source.to_string())
			.env_remove("CLAUDECODE")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		child.stdin.take().unwrap().write_all(br#"{"session_id":"test","model":{"id":"context-still-works"},"mod":{"usage":{"fetched_at":"2099-01-01T00:00:00Z","body":{"five_hour":{"utilization":99},"extra_usage":{"used_credits":1200,"monthly_limit":5000}}}}}"#).unwrap();
		let out = child.wait_with_output().unwrap();
		assert!(out.status.success());
		let value: Value = serde_json::from_slice(&out.stdout).unwrap();
		assert_eq!(value["needs"]["usage"], false);
		assert_eq!(value["usage_source"], "external");
		let text = value["rows"].to_string();
		assert!(
			text.contains("context-still-works")
				&& !text.contains("old-login")
				&& !text.contains("99%"),
			"{text}"
		);
	}
	assert!(!root.join(".statusline/usage.refresh").exists());
	// Let the detached missing-executable worker release its private cache before cleanup.
	std::thread::sleep(Duration::from_millis(200));
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_usage_an_old_inflight_result_cannot_replace_the_new_key() {
	let root = scratch("inflight");
	settings(&root);
	let mut old = worker(&root, "alpha", "slow").spawn().unwrap();
	assert!(refresh(&root, "beta", "").status.success());
	let before = String::from_utf8(render(&root, "beta", false).stdout).unwrap();
	assert!(before.contains("beta") && !before.contains("alpha"));
	assert!(old.wait().unwrap().success());
	let after = String::from_utf8(render(&root, "beta", false).stdout).unwrap();
	assert_eq!(before, after);
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		for path in caches(&root) {
			assert_eq!(
				fs::metadata(path).unwrap().permissions().mode() & 0o777,
				0o600
			);
		}
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_usage_standalone_example_drives_native_and_plugin_output() {
	let root = scratch("standalone");
	settings(&root);
	let mut fixture: Value = serde_json::from_str(include_str!(
		"../../statusline-core/tests/fixtures/external-usage-v1.json"
	))
	.unwrap();
	fixture["observed_at"] = json!(chrono::Utc::now());
	fixture["limits"][1]["resets_at"] = json!(chrono::Utc::now() + chrono::TimeDelta::hours(1));
	fixture["limits"][1]["id"] = json!("custom");
	fs::write(
		root.join("accounts.json"),
		json!({"work":fixture}).to_string(),
	)
	.unwrap();
	let source = json!({"command":["python3",concat!(env!("CARGO_MANIFEST_DIR"),"/../../docs/examples/usage-file.py"),root.join("accounts.json")],"key_env":"TEST_ACCOUNT_KEY"});
	let out = Command::new(env!("CARGO_BIN_EXE_statusline"))
		.args([
			"external-usage-refresh",
			"--source",
			&source.to_string(),
			"--request",
			&request("work").to_string(),
		])
		.env("HOME", &root)
		.output()
		.unwrap();
	assert!(out.status.success(), "{out:?}");
	for spans in [false, true] {
		let mut command = Command::new(env!("CARGO_BIN_EXE_statusline"));
		if spans {
			command.args(["--format", "spans", "--plugin-data"]);
		}
		let mut child = command
			.env("HOME", &root)
			.env_remove("CLAUDECODE")
			.env("TEST_ACCOUNT_KEY", "work")
			.env("STATUSLINE_USAGE_SOURCE", source.to_string())
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()
			.unwrap();
		child
			.stdin
			.take()
			.unwrap()
			.write_all(br#"{"session_id":"conversation-1"}"#)
			.unwrap();
		let out = child.wait_with_output().unwrap();
		assert!(out.status.success());
		let text = String::from_utf8(out.stdout).unwrap();
		assert!(
			text.contains("Work") && text.contains("Burst 42%"),
			"{text}"
		);
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_usage_session_report_uses_the_same_account_scope() {
	let root = scratch("report");
	settings(&root);
	assert!(refresh(&root, "alpha", "").status.success());
	fs::write(
		root.join(".statusline/usage.json"),
		json!({"usage":"{\"five_hour\":{\"utilization\":99}}"}).to_string(),
	)
	.unwrap();
	let out = Command::new(env!("CARGO_BIN_EXE_statusline"))
		.env("HOME", &root)
		.env("CLAUDECODE", "1")
		.env("CLAUDE_CODE_SESSION_ID", "conversation-1")
		.env("STATUSLINE_USAGE_SOURCE", source().to_string())
		.env("TEST_ACCOUNT_KEY", "alpha")
		.stdin(Stdio::null())
		.output()
		.unwrap();
	let report: Value = serde_json::from_slice(&out.stdout).unwrap();
	assert_eq!(report["account_usage"]["key"], "alpha");
	assert_eq!(report["account_usage"]["limits"][0]["used_percent"], 42.0);
	fs::remove_dir_all(root).unwrap();
}
