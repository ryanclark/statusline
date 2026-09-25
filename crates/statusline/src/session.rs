use crate::usage_cache;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use statusline_core::format::countdown_to;
use statusline_core::usage_bridge::UsageReply;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Keys of the piped-in JSON handed back to Claude. The rest (cwd, workspace, worktree, vim mode) it already
/// knows or has no use for.
const REPORTED_KEYS: [&str; 4] = ["model", "context_window", "cost", "exceeds_200k_tokens"];

/// Snapshots untouched this long belong to abandoned sessions. A resumed session renders before Claude can ask
/// for its report, so it never finds its own snapshot pruned.
const SNAPSHOT_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The session to report on when Claude runs `statusline` itself. Claude Code's own status line invocation may
/// inherit `CLAUDECODE` too, but it always pipes JSON, while the Bash tool's stdin is empty.
#[must_use]
pub fn requested(stdin: &[u8]) -> Option<String> {
	agent_session(
		std::env::var_os("CLAUDECODE").is_some_and(|v| !v.is_empty()),
		std::env::var("CLAUDE_CODE_SESSION_ID").ok(),
		stdin,
	)
}

fn agent_session(claudecode: bool, session_id: Option<String>, stdin: &[u8]) -> Option<String> {
	if !claudecode || !stdin.trim_ascii().is_empty() {
		return None;
	}
	session_id.filter(|id| !id.is_empty())
}

/// Only the id is read, so a render whose input the full parse rejects, such as a field Claude Code starts
/// sending as null, is still captured.
#[derive(Deserialize)]
struct Envelope {
	session_id: String,
}

/// Best effort, so a failed write never costs the status line its render.
pub fn capture(raw: &[u8]) {
	if let Some(dir) = sessions_dir() {
		capture_in(&dir, raw, SystemTime::now());
	}
}

fn capture_in(dir: &Path, raw: &[u8], now: SystemTime) {
	let Ok(Envelope { session_id }) = serde_json::from_slice(raw) else {
		return;
	};
	let Some(path) = snapshot_path(dir, &session_id) else {
		return;
	};
	if !path.exists() {
		prepare(dir, now);
	}

	// Renames are atomic, so a concurrent `statusline` run from Claude never reads a half-written file. The pid
	// keeps overlapping renders from sharing a temp file.
	let tmp = dir.join(format!(".{session_id}.{}.tmp", std::process::id()));
	if fs::write(&tmp, raw)
		.and_then(|()| fs::rename(&tmp, &path))
		.is_err()
	{
		let _ = fs::remove_file(&tmp);
	}
}

/// Runs once per session rather than per render. Snapshots carry paths and costs, so the directory is private,
/// and abandoned sessions and killed renders would otherwise leave files behind forever.
fn prepare(dir: &Path, now: SystemTime) {
	let _ = fs::create_dir_all(dir);

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;

		let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
	}

	let (Ok(entries), Some(cutoff)) = (fs::read_dir(dir), now.checked_sub(SNAPSHOT_TTL)) else {
		return;
	};
	for entry in entries.flatten() {
		let path = entry.path();
		let ours = path
			.extension()
			.is_some_and(|ext| ext == "json" || ext == "tmp");
		let stale = entry
			.metadata()
			.and_then(|m| m.modified())
			.is_ok_and(|at| at < cutoff);
		if ours && stale {
			let _ = fs::remove_file(&path);
		}
	}
}

pub fn print_report(session_id: &str) {
	let snapshot = sessions_dir()
		.and_then(|dir| snapshot_path(&dir, session_id))
		.and_then(|path| Snapshot::load(&path));
	// usage.json is rewritten in place, so stat it first: a refresh landing in between then overstates the age
	// rather than passing old limits off as fresh.
	let updated_at = usage_cache::cache_path().and_then(|p| modified(&p));
	let usage = usage_cache::read().zip(updated_at);
	let report = build_report(
		session_id,
		snapshot.as_ref(),
		usage.as_ref().map(|(reply, at)| (reply, *at)),
		Utc::now(),
	);

	println!("{report:#}");
}

struct Snapshot {
	json: Value,
	recorded_at: DateTime<Utc>,
}

impl Snapshot {
	fn load(path: &Path) -> Option<Self> {
		// One handle for both, so a render renaming over the path can't pair old content with a newer mtime.
		let file = File::open(path).ok()?;
		let recorded_at = file.metadata().and_then(|m| m.modified()).ok()?.into();

		Some(Self {
			json: serde_json::from_reader(BufReader::new(file)).ok()?,
			recorded_at,
		})
	}
}

fn build_report(
	session_id: &str,
	snapshot: Option<&Snapshot>,
	usage: Option<(&UsageReply, DateTime<Utc>)>,
	now: DateTime<Utc>,
) -> Value {
	let mut report = Map::new();
	report.insert("session_id".to_owned(), session_id.into());

	match snapshot {
		Some(snapshot) => {
			report.insert(
				"recorded_at".to_owned(),
				timestamp(snapshot.recorded_at).into(),
			);
			report.insert(
				"age_secs".to_owned(),
				age_secs(snapshot.recorded_at, now).into(),
			);
			for key in REPORTED_KEYS {
				if let Some(value) = snapshot.json.get(key) {
					report.insert(key.to_owned(), value.clone());
				}
			}
			if let Some(limits) = snapshot.json.get("rate_limits") {
				report.insert("rate_limits".to_owned(), rate_limits(limits.clone(), now));
			}
		}
		None => {
			report.insert(
				"error".to_owned(),
				"no status line snapshot for this session yet. Set \"capture_snapshots\": true in \
				 ~/.statusline/settings.json, then wait for the status line to refresh"
					.into(),
			);
		}
	}

	if let Some((reply, updated_at)) = usage {
		report.insert(
			"account_usage".to_owned(),
			account_usage(reply, updated_at, now),
		);
	}

	report.into()
}

/// Claude Code sends `resets_at` as epoch seconds, which Claude cannot turn into a wall-clock time or a
/// countdown without shelling out again.
fn rate_limits(mut limits: Value, now: DateTime<Utc>) -> Value {
	let periods = limits
		.as_object_mut()
		.into_iter()
		.flat_map(Map::values_mut)
		.filter_map(Value::as_object_mut);
	for fields in periods {
		if let Some(secs) = fields.get("resets_at").and_then(Value::as_i64) {
			insert_reset(fields, secs, now);
		}
	}
	limits
}

fn insert_reset(fields: &mut Map<String, Value>, secs: i64, now: DateTime<Utc>) {
	let Some(at) = DateTime::from_timestamp(secs, 0) else {
		return;
	};
	fields.insert("resets_at".to_owned(), timestamp(at).into());
	if let Some(countdown) = countdown_to(secs, now) {
		fields.insert("resets_in".to_owned(), countdown.into());
	}
}

fn account_usage(reply: &UsageReply, updated_at: DateTime<Utc>, now: DateTime<Utc>) -> Value {
	let mut out = Map::new();
	out.insert("updated_at".to_owned(), timestamp(updated_at).into());
	out.insert("age_secs".to_owned(), age_secs(updated_at, now).into());

	let (usage, credits) = usage_cache::results(Some(reply), true, true);
	match usage {
		Some(Ok(usage)) => {
			let limits: Vec<Value> = usage
				.limits
				.iter()
				.map(|limit| {
					let mut entry = Map::new();
					entry.insert("label".to_owned(), limit.label().into());
					entry.insert("percent".to_owned(), json!(limit.percent));
					if let Some(secs) = limit.resets_at_epoch() {
						insert_reset(&mut entry, secs, now);
					}
					Value::Object(entry)
				})
				.collect();
			out.insert("limits".to_owned(), limits.into());

			if let Some(extra) = &usage.extra_usage
				&& let (Some(used), Some(limit)) = (extra.used_credits, extra.monthly_limit)
			{
				out.insert(
					"extra_usage".to_owned(),
					json!({
						"used": used.to_string(),
						"limit": limit.to_string(),
						"used_percentage": used.as_percentage_of(limit),
					}),
				);
			}
		}
		Some(Err(e)) => {
			out.insert("error".to_owned(), e.to_string().into());
		}
		None => {}
	}

	if let Some(Ok(credits)) = credits {
		out.insert("credits".to_owned(), credits.balance().to_string().into());
	}

	out.into()
}

fn age_secs(at: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
	(now - at).num_seconds().max(0)
}

fn timestamp(at: DateTime<Utc>) -> String {
	at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn modified(path: &Path) -> Option<DateTime<Utc>> {
	fs::metadata(path)
		.and_then(|m| m.modified())
		.ok()
		.map(DateTime::from)
}

fn sessions_dir() -> Option<PathBuf> {
	crate::util::app_data_dir().ok().map(|d| d.join("sessions"))
}

/// The id comes from stdin or the environment and becomes a file name, so anything beyond a plain token
/// could escape the sessions directory.
fn snapshot_path(dir: &Path, session_id: &str) -> Option<PathBuf> {
	let valid = !session_id.is_empty()
		&& session_id.len() <= 128
		&& session_id
			.bytes()
			.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
	valid.then(|| dir.join(format!("{session_id}.json")))
}

#[cfg(test)]
mod tests {
	use super::*;
	use statusline_core::input::InputData;

	const FIXTURE: &str = include_str!("../tests/fixtures/input.json");
	const HOUR: Duration = Duration::from_secs(60 * 60);

	fn at(secs: i64) -> DateTime<Utc> {
		DateTime::from_timestamp(secs, 0).unwrap()
	}

	fn temp_dir(name: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("statusline-{name}-{}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		dir
	}

	fn input(session_id: &str) -> Value {
		let mut input: Value = serde_json::from_str(FIXTURE).unwrap();
		input["session_id"] = session_id.into();
		input
	}

	fn piped(session_id: &str) -> Vec<u8> {
		serde_json::to_vec(&input(session_id)).unwrap()
	}

	fn names(dir: &Path) -> Vec<String> {
		let mut names: Vec<String> = fs::read_dir(dir)
			.unwrap()
			.map(|e| e.unwrap().file_name().into_string().unwrap())
			.collect();
		names.sort();
		names
	}

	fn fixture_snapshot(recorded_at: DateTime<Utc>) -> Snapshot {
		Snapshot {
			json: serde_json::from_str(FIXTURE).unwrap(),
			recorded_at,
		}
	}

	fn usage_reply() -> UsageReply {
		UsageReply {
			usage: Some(
				r#"{
					"limits": [
						{"kind": "session", "percent": 12, "resets_at": "2025-02-01T16:30:00+00:00", "scope": null},
						{"kind": "weekly_scoped", "percent": 37, "resets_at": null,
						 "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}}
					],
					"extra_usage": {"monthly_limit": 5000, "used_credits": 1200}
				}"#
				.to_owned(),
			),
			credits: Some(r#"{"amount":3304}"#.to_owned()),
			error: None,
		}
	}

	#[test]
	fn agent_session_needs_claudecode_a_session_id_and_empty_stdin() {
		let id = || Some("0dac468e-e71a-4f01-bc11-e5f8bf4370e9".to_owned());
		assert_eq!(agent_session(true, id(), b""), id());
		assert_eq!(agent_session(true, id(), b" \n"), id());
		assert_eq!(
			agent_session(true, id(), FIXTURE.as_bytes()),
			None,
			"piped JSON means Claude Code is rendering the status line"
		);
		assert_eq!(agent_session(false, id(), b""), None);
		assert_eq!(agent_session(true, None, b""), None);
		assert_eq!(agent_session(true, Some(String::new()), b""), None);
	}

	#[test]
	fn capture_writes_the_piped_bytes_verbatim() {
		let dir = temp_dir("capture");
		let raw = piped("abc-123_x");
		capture_in(&dir, &raw, SystemTime::now());

		assert_eq!(fs::read(dir.join("abc-123_x.json")).unwrap(), raw);
		assert_eq!(
			names(&dir),
			["abc-123_x.json"],
			"the temp file must be renamed away"
		);

		let smaller = br#"{"session_id":"abc-123_x"}"#;
		capture_in(&dir, smaller, SystemTime::now());
		assert_eq!(fs::read(dir.join("abc-123_x.json")).unwrap(), smaller);

		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn capture_keeps_renders_the_full_input_parse_rejects() {
		let dir = temp_dir("capture-strict");
		let mut drifted = input("drifted");
		drifted["vim"] = Value::Null;
		let raw = serde_json::to_vec(&drifted).unwrap();
		assert!(InputData::from_reader(raw.as_slice()).is_err());

		capture_in(&dir, &raw, SystemTime::now());
		assert_eq!(fs::read(dir.join("drifted.json")).unwrap(), raw);

		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn capture_ignores_input_without_a_plain_token_id() {
		let dir = temp_dir("capture-unsafe");
		for id in ["", "../escape", "a/b", "a.b", "..", &"x".repeat(129)] {
			capture_in(&dir, &piped(id), SystemTime::now());
		}
		capture_in(&dir, b"not json", SystemTime::now());
		capture_in(&dir, br#"{"cwd":"/tmp"}"#, SystemTime::now());

		assert!(!dir.exists(), "nothing should have been written");
	}

	#[test]
	fn a_new_session_prunes_stale_snapshots_and_orphaned_temp_files() {
		let dir = temp_dir("capture-prune");
		let now = SystemTime::now();
		capture_in(&dir, &piped("live"), now);

		let stale = now - SNAPSHOT_TTL - HOUR;
		for (name, modified) in [
			("gone.json", stale),
			(".gone.123.tmp", stale),
			("recent.json", now - HOUR),
			("notes.txt", stale),
		] {
			File::create(dir.join(name))
				.unwrap()
				.set_modified(modified)
				.unwrap();
		}

		capture_in(&dir, &piped("live"), now);
		assert!(
			dir.join("gone.json").exists(),
			"a render of a known session must not scan the directory"
		);

		capture_in(&dir, &piped("next"), now);
		assert_eq!(
			names(&dir),
			["live.json", "next.json", "notes.txt", "recent.json"]
		);

		fs::remove_dir_all(&dir).unwrap();
	}

	#[cfg(unix)]
	#[test]
	fn a_new_session_makes_the_directory_private() {
		use std::os::unix::fs::PermissionsExt;

		let dir = temp_dir("capture-private");
		fs::create_dir_all(&dir).unwrap();
		fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

		capture_in(&dir, &piped("abc"), SystemTime::now());
		let mode = fs::metadata(&dir).unwrap().permissions().mode();
		assert_eq!(mode & 0o777, 0o700);

		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn snapshot_load_reads_content_and_mtime() {
		let dir = temp_dir("snapshot-load");
		let raw = piped("abc");
		capture_in(&dir, &raw, SystemTime::now());
		let path = dir.join("abc.json");
		let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_738_400_000);
		File::options()
			.write(true)
			.open(&path)
			.unwrap()
			.set_modified(written)
			.unwrap();

		let snapshot = Snapshot::load(&path).unwrap();
		assert_eq!(snapshot.json, input("abc"));
		assert_eq!(snapshot.recorded_at, at(1_738_400_000));
		assert!(Snapshot::load(&dir.join("missing.json")).is_none());

		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn report_keeps_what_claude_needs_and_drops_the_rest() {
		let report = build_report(
			"abc",
			Some(&fixture_snapshot(at(1_738_400_000))),
			None,
			at(1_738_400_042),
		);

		assert_eq!(report["session_id"], "abc");
		assert_eq!(report["recorded_at"], "2025-02-01T08:53:20Z");
		assert_eq!(report["age_secs"], 42);
		assert_eq!(report["model"]["display_name"], "Opus");
		assert_eq!(report["context_window"]["used_percentage"], 8);
		assert_eq!(report["cost"]["total_lines_added"], 156);
		assert_eq!(report["exceeds_200k_tokens"], false);
		for dropped in [
			"cwd",
			"workspace",
			"transcript_path",
			"version",
			"output_style",
			"vim",
			"agent",
			"worktree",
			"error",
			"account_usage",
		] {
			assert!(
				report.get(dropped).is_none(),
				"{dropped} should not be reported: {report}"
			);
		}
	}

	#[test]
	fn report_turns_rate_limit_resets_into_timestamps_and_countdowns() {
		// The fixture's five-hour window resets at 1738425600 and the seven-day one at 1738857600.
		let now = at(1_738_425_600 - 7200);
		let report = build_report("abc", Some(&fixture_snapshot(now)), None, now);

		let five = &report["rate_limits"]["five_hour"];
		assert_eq!(five["used_percentage"], 23.5);
		assert_eq!(five["resets_at"], "2025-02-01T16:00:00Z");
		assert_eq!(five["resets_in"], "2h0m");
		assert_eq!(report["rate_limits"]["seven_day"]["resets_in"], "5d2h");

		let later = build_report("abc", Some(&fixture_snapshot(now)), None, at(1_738_500_000));
		assert!(
			later["rate_limits"]["five_hour"].get("resets_in").is_none(),
			"a window that already reset has no countdown"
		);
	}

	#[test]
	fn report_without_a_snapshot_says_how_to_enable_capture() {
		let report = build_report("abc", None, None, at(0));
		assert!(
			report["error"]
				.as_str()
				.unwrap()
				.contains("capture_snapshots"),
			"{report}"
		);
		assert!(report.get("model").is_none());
	}

	#[test]
	fn report_carries_account_usage_from_the_cache() {
		let now = DateTime::parse_from_rfc3339("2025-02-01T14:30:00Z")
			.unwrap()
			.with_timezone(&Utc);
		let reply = usage_reply();
		let report = build_report("abc", None, Some((&reply, at(now.timestamp() - 30))), now);

		let usage = &report["account_usage"];
		assert_eq!(usage["age_secs"], 30);
		assert_eq!(usage["limits"][0]["label"], "5-hour session");
		assert_eq!(usage["limits"][0]["percent"], 12.0);
		assert_eq!(usage["limits"][0]["resets_at"], "2025-02-01T16:30:00Z");
		assert_eq!(usage["limits"][0]["resets_in"], "2h0m");
		assert_eq!(usage["limits"][1]["label"], "Fable (weekly)");
		assert_eq!(usage["limits"][1]["percent"], 37.0);
		assert!(usage["limits"][1].get("resets_at").is_none());
		assert_eq!(usage["extra_usage"]["used"], "$12");
		assert_eq!(usage["extra_usage"]["limit"], "$50");
		assert_eq!(usage["extra_usage"]["used_percentage"], 24.0);
		assert_eq!(usage["credits"], "$33");
		assert!(usage.get("error").is_none());
		assert!(
			report.get("error").is_some(),
			"account usage is reported even without a session snapshot"
		);
	}

	#[test]
	fn account_usage_reports_a_cached_login_failure() {
		let reply = UsageReply::error(statusline_core::usage_bridge::ERROR_NOT_LOGGED_IN);
		let usage = account_usage(&reply, at(0), at(0));
		assert_eq!(usage["error"], "not logged in to claude.ai");
		assert!(usage.get("limits").is_none());
	}
}
