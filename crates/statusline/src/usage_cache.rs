use crate::usage::{PrepaidCredits, UsageError, UsageResponse, fetch_credits_raw, fetch_usage_raw};
use serde::Deserialize;
use statusline_core::browser::Browser;
use statusline_core::input::PluginUsage;
use statusline_core::usage_bridge::{ERROR_NOT_LOGGED_IN, UsageReply, UsageRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const REFRESH_TTL: Duration = Duration::from_secs(15);
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(7);
// The plugin refetches every minute, so data this old means its fetches are failing or backing off.
const PLUGIN_STALE_MS: i64 = 5 * 60_000;
// A little ahead is skew between chats' clock reads. Much more means the clock stepped back since the fetch.
const PLUGIN_AHEAD_MS: i64 = 60_000;

type UsageResult = Option<Result<UsageResponse, UsageError>>;
type CreditsResult = Option<Result<PrepaidCredits, UsageError>>;

pub struct Resolved {
	pub usage: UsageResult,
	pub credits: CreditsResult,
	pub stale: bool,
}

/// The plugin's usage wins whenever it is sent, and then the cookie cache is not read at all. A null body is a login
/// whose first fetch has not landed, which hides the segments as an empty cookie cache does.
pub fn resolve(
	plugin: Option<&PluginUsage>,
	cookie: impl FnOnce() -> Option<UsageReply>,
	needs_usage: bool,
	needs_credits: bool,
	now_ms: i64,
) -> Resolved {
	let Some(plugin) = plugin else {
		let (usage, credits) = results(cookie().as_ref(), needs_usage, needs_credits);
		return Resolved {
			usage,
			credits,
			stale: false,
		};
	};

	let parsed = (!plugin.body.is_null() && (needs_usage || needs_credits)).then(|| {
		UsageResponse::deserialize(&plugin.body)
			.map_err(|e| UsageError::Other(format!("parsing usage: {e}")))
	});
	let credits = needs_credits
		.then(|| parsed.as_ref()?.as_ref().ok()?.credits().map(Ok))
		.flatten();
	Resolved {
		usage: parsed.filter(|_| needs_usage),
		credits,
		stale: plugin
			.fetched_at_ms
			.is_none_or(|at| now_ms - at > PLUGIN_STALE_MS || at - now_ms > PLUGIN_AHEAD_MS),
	}
}

#[must_use]
pub fn read() -> Option<UsageReply> {
	let path = cache_path()?;

	serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()
}

#[must_use]
pub fn results(
	cached: Option<&UsageReply>,
	needs_usage: bool,
	needs_credits: bool,
) -> (UsageResult, CreditsResult) {
	let usage = needs_usage.then(|| cached.and_then(parse_usage)).flatten();
	let credits = needs_credits
		.then(|| cached.and_then(parse_credits))
		.flatten();

	(usage, credits)
}

fn parse_usage(reply: &UsageReply) -> UsageResult {
	if reply.is_not_logged_in() {
		return Some(Err(UsageError::NotLoggedIn));
	}

	reply.usage.as_deref().map(|body| {
		serde_json::from_str(body).map_err(|e| UsageError::Other(format!("parsing usage: {e}")))
	})
}

fn parse_credits(reply: &UsageReply) -> CreditsResult {
	if reply.is_not_logged_in() {
		return Some(Err(UsageError::NotLoggedIn));
	}

	reply.credits.as_deref().map(|body| {
		serde_json::from_str(body).map_err(|e| UsageError::Other(format!("parsing credits: {e}")))
	})
}

pub fn maybe_spawn_refresh(org_id: &str, browser: Browser, profile: Option<&str>) {
	if recently_attempted() {
		return;
	}

	let Ok(exe) = std::env::current_exe() else {
		return;
	};

	touch_stamp();

	let mut cmd = std::process::Command::new(exe);
	cmd.arg("usage-refresh")
		.arg("--org")
		.arg(org_id)
		.arg("--browser")
		.arg(browser_arg(browser))
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null());

	if let Some(p) = profile {
		cmd.arg("--profile").arg(p);
	}

	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;

		cmd.process_group(0);
	}

	let _ = cmd.spawn();
}

pub fn run_refresh(org_id: &str, browser: Browser, profile: Option<&str>) {
	let reply = fetch_reply(org_id, browser, profile);

	write_cache(&reply);
}

fn fetch_reply(org_id: &str, browser: Browser, profile: Option<&str>) -> UsageReply {
	match std::env::var("CLANKERBOX_USAGE_PORT")
		.ok()
		.filter(|p| !p.is_empty())
	{
		Some(port) => bridge_fetch(&port, org_id, browser, profile),
		None => direct_fetch(org_id, browser, profile),
	}
}

fn bridge_fetch(port: &str, org_id: &str, browser: Browser, profile: Option<&str>) -> UsageReply {
	let req = UsageRequest::new(org_id, browser, profile.map(str::to_owned));

	bridge_roundtrip(port, &req).unwrap_or_else(|e| UsageReply::error(format!("usage bridge: {e}")))
}

fn bridge_roundtrip(port: &str, req: &UsageRequest) -> std::io::Result<UsageReply> {
	use std::io::{BufRead, BufReader, Write};
	use std::net::{TcpStream, ToSocketAddrs};

	let addr = format!("host.docker.internal:{port}")
		.to_socket_addrs()?
		.next()
		.ok_or_else(|| {
			std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"host.docker.internal did not resolve",
			)
		})?;
	let stream = TcpStream::connect_timeout(&addr, BRIDGE_TIMEOUT)?;
	stream.set_read_timeout(Some(BRIDGE_TIMEOUT))?;
	stream.set_write_timeout(Some(BRIDGE_TIMEOUT))?;

	let mut writer = &stream;
	let mut body = serde_json::to_string(req)
		.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
	body.push('\n');
	writer.write_all(body.as_bytes())?;
	writer.flush()?;

	let mut reader = BufReader::new(&stream);
	let mut line = String::new();
	reader.read_line(&mut line)?;
	serde_json::from_str(line.trim())
		.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn direct_fetch(org_id: &str, browser: Browser, profile: Option<&str>) -> UsageReply {
	match fetch_usage_raw(org_id, browser, profile) {
		Ok(usage) => UsageReply {
			usage: Some(usage),
			credits: fetch_credits_raw(org_id, browser, profile).ok(),
			error: None,
		},
		Err(UsageError::NotLoggedIn) => UsageReply::error(ERROR_NOT_LOGGED_IN),
		Err(e) => UsageReply::error(e.to_string()),
	}
}

fn write_cache(reply: &UsageReply) {
	if reply
		.error
		.as_deref()
		.is_some_and(|e| e != ERROR_NOT_LOGGED_IN)
	{
		return;
	}

	let Some(path) = cache_path() else {
		return;
	};

	if let Some(parent) = path.parent() {
		let _ = fs::create_dir_all(parent);
	}

	if let Ok(body) = serde_json::to_string(reply) {
		let _ = fs::write(&path, body);
	}
}

pub fn cache_path() -> Option<PathBuf> {
	crate::util::app_data_dir()
		.ok()
		.map(|d| d.join("usage.json"))
}

fn stamp_path() -> Option<PathBuf> {
	crate::util::app_data_dir()
		.ok()
		.map(|d| d.join("usage.refresh"))
}

fn recently_attempted() -> bool {
	let Some(path) = stamp_path() else {
		return false;
	};

	fs::metadata(&path)
		.and_then(|m| m.modified())
		.ok()
		.and_then(|m| SystemTime::now().duration_since(m).ok())
		.is_some_and(|age| age < REFRESH_TTL)
}

fn touch_stamp() {
	let Some(path) = stamp_path() else {
		return;
	};

	if let Some(parent) = path.parent() {
		let _ = fs::create_dir_all(parent);
	}

	let _ = fs::write(&path, b"");
}

fn browser_arg(browser: Browser) -> &'static str {
	match browser {
		Browser::Chrome => "chrome",
		Browser::Brave => "brave",
		Browser::Firefox => "firefox",
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn usage_body() -> String {
		r#"{"extra_usage":{"monthly_limit":5000,"used_credits":1200}}"#.to_owned()
	}

	const NOW_MS: i64 = 1_791_280_000_000;

	// Every top-level key the OAuth usage endpoint answers with, with a Fable row among the limits.
	fn oauth_body() -> serde_json::Value {
		serde_json::json!({
			"five_hour": {"utilization": 12.0, "resets_at": "2026-10-06T13:00:00+00:00"},
			"seven_day": {"utilization": 40.0, "resets_at": "2026-10-10T09:00:00+00:00"},
			"seven_day_oauth_apps": null,
			"seven_day_opus": null,
			"seven_day_sonnet": {"utilization": 3.0, "resets_at": "2026-10-10T09:00:00+00:00"},
			"extra_usage": {
				"is_enabled": true,
				"monthly_limit": 10000,
				"used_credits": 2500.0,
				"utilization": 25.0,
				"currency": "USD",
				"disabled_reason": null
			},
			"limits": [
				{"kind": "session", "group": "plan", "percent": 12, "resets_at": "2026-10-06T13:00:00+00:00",
				 "severity": "normal", "is_active": true, "scope": null},
				{"kind": "weekly_all", "group": "plan", "percent": 40, "resets_at": "2026-10-10T09:00:00+00:00",
				 "severity": "normal", "is_active": true, "scope": null},
				{"kind": "weekly_scoped", "group": "plan", "percent": 63, "resets_at": "2026-10-10T09:00:00+00:00",
				 "severity": "warning", "is_active": true,
				 "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}}
			],
			"spend": {
				"used": {"amount_minor": 3690, "currency": "USD", "exponent": 2},
				"balance": {"amount_minor": 4210, "currency": "USD", "exponent": 2}
			}
		})
	}

	fn plugin(fetched_at_ms: Option<i64>) -> PluginUsage {
		PluginUsage {
			fetched_at_ms,
			body: oauth_body(),
		}
	}

	#[test]
	fn plugin_usage_feeds_fable_extra_usage_and_credits() {
		let r = resolve(
			Some(&plugin(Some(NOW_MS - 30_000))),
			|| None,
			true,
			true,
			NOW_MS,
		);
		let usage = r.usage.unwrap().unwrap();
		assert_eq!(usage.fable().unwrap().percent, 63.0.into());
		assert_eq!(
			usage.extra_usage.unwrap().format(false).unwrap(),
			"$25/$100"
		);
		assert_eq!(r.credits.unwrap().unwrap().balance().to_string(), "$42");
		assert!(!r.stale);
	}

	#[test]
	fn plugin_usage_wins_over_the_cookie_cache_without_reading_it() {
		let r = resolve(
			Some(&plugin(Some(NOW_MS))),
			|| panic!("the cookie cache was read"),
			true,
			true,
			NOW_MS,
		);
		assert_eq!(r.credits.unwrap().unwrap().balance().to_string(), "$42");

		let cookie = UsageReply {
			usage: Some(usage_body()),
			credits: Some(r#"{"amount":3304}"#.to_owned()),
			error: None,
		};
		let r = resolve(None, || Some(cookie), true, true, NOW_MS);
		assert_eq!(r.credits.unwrap().unwrap().balance().to_string(), "$33");
		assert!(r.usage.unwrap().unwrap().fable().is_none());
		assert!(!r.stale);
	}

	#[test]
	fn a_plugin_login_with_no_body_yet_hides_the_segments_without_cookies() {
		let pending = PluginUsage {
			fetched_at_ms: None,
			body: serde_json::Value::Null,
		};
		let r = resolve(
			Some(&pending),
			|| panic!("the cookie cache was read"),
			true,
			true,
			NOW_MS,
		);
		assert!(r.usage.is_none() && r.credits.is_none());
	}

	#[test]
	fn plugin_usage_older_than_five_minutes_is_stale() {
		let fresh = resolve(
			Some(&plugin(Some(NOW_MS - 300_000))),
			|| None,
			true,
			false,
			NOW_MS,
		);
		assert!(!fresh.stale);
		let old = resolve(
			Some(&plugin(Some(NOW_MS - 300_001))),
			|| None,
			true,
			false,
			NOW_MS,
		);
		assert!(old.stale);
		assert!(resolve(Some(&plugin(None)), || None, true, false, NOW_MS).stale);
	}

	#[test]
	fn plugin_usage_dated_ahead_of_the_clock_is_stale() {
		let skewed = resolve(
			Some(&plugin(Some(NOW_MS + 5_000))),
			|| None,
			true,
			false,
			NOW_MS,
		);
		assert!(!skewed.stale);
		let stepped_back = resolve(
			Some(&plugin(Some(NOW_MS + 2 * 3_600_000))),
			|| None,
			true,
			false,
			NOW_MS,
		);
		assert!(stepped_back.stale);
	}

	#[test]
	fn a_plugin_body_that_does_not_parse_fails_only_the_usage_segments() {
		let odd = PluginUsage {
			fetched_at_ms: Some(NOW_MS),
			body: serde_json::json!({"limits": "not a list"}),
		};
		let r = resolve(Some(&odd), || None, true, true, NOW_MS);
		assert!(matches!(r.usage, Some(Err(UsageError::Other(_)))));
		assert!(r.credits.is_none());
	}

	#[test]
	fn plugin_usage_honors_the_needs_flags() {
		let r = resolve(Some(&plugin(Some(NOW_MS))), || None, false, false, NOW_MS);
		assert!(r.usage.is_none() && r.credits.is_none());
		let r = resolve(Some(&plugin(Some(NOW_MS))), || None, false, true, NOW_MS);
		assert!(r.usage.is_none() && r.credits.is_some());
	}

	#[test]
	fn results_parse_usage_and_credits_from_a_good_reply() {
		let reply = UsageReply {
			usage: Some(usage_body()),
			credits: Some(r#"{"amount":3304}"#.to_owned()),
			error: None,
		};
		let (usage, credits) = results(Some(&reply), true, true);
		assert!(matches!(usage, Some(Ok(_))), "usage should parse");
		assert!(matches!(credits, Some(Ok(_))), "credits should parse");
	}

	#[test]
	fn results_honor_the_needs_flags() {
		let reply = UsageReply {
			usage: Some(usage_body()),
			credits: Some(r#"{"amount":1}"#.to_owned()),
			error: None,
		};
		let (u, c) = results(Some(&reply), false, false);
		assert!(u.is_none() && c.is_none());
		let (u, c) = results(Some(&reply), true, false);
		assert!(u.is_some() && c.is_none());
	}

	#[test]
	fn results_map_not_logged_in_to_the_error_variant() {
		let reply = UsageReply::error(ERROR_NOT_LOGGED_IN);
		let (usage, credits) = results(Some(&reply), true, true);
		assert!(matches!(usage, Some(Err(UsageError::NotLoggedIn))));
		assert!(matches!(credits, Some(Err(UsageError::NotLoggedIn))));
	}

	#[test]
	fn results_with_no_cache_yield_none() {
		let (usage, credits) = results(None, true, true);
		assert!(
			usage.is_none() && credits.is_none(),
			"no cache ⇒ nothing to render yet"
		);
	}

	#[test]
	fn results_with_a_transient_error_reply_show_nothing() {
		let reply = UsageReply::error("network down");
		let (usage, credits) = results(Some(&reply), true, true);
		assert!(usage.is_none() && credits.is_none());
	}

	#[test]
	fn browser_arg_is_the_lowercase_clap_token() {
		assert_eq!(browser_arg(Browser::Chrome), "chrome");
		assert_eq!(browser_arg(Browser::Brave), "brave");
		assert_eq!(browser_arg(Browser::Firefox), "firefox");
	}
}
