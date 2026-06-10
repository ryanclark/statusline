use crate::usage::{PrepaidCredits, UsageError, UsageResponse, fetch_credits_raw, fetch_usage_raw};
use statusline_core::browser::Browser;
use statusline_core::usage_bridge::{ERROR_NOT_LOGGED_IN, UsageReply, UsageRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const REFRESH_TTL: Duration = Duration::from_secs(15);
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(7);

type UsageResult = Option<Result<UsageResponse, UsageError>>;
type CreditsResult = Option<Result<PrepaidCredits, UsageError>>;

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

fn cache_path() -> Option<PathBuf> {
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
