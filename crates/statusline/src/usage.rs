use crate::browser;
use statusline_core::browser::Browser;
use std::time::Duration;

#[allow(unused_imports)]
pub(crate) use statusline_core::usage::{ExtraUsage, PrepaidCredits, UsageError, UsageResponse};

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36";

pub(crate) fn fetch_usage(
	org_id: &str,
	browser: Browser,
	profile: Option<&str>,
) -> Result<UsageResponse, UsageError> {
	let url = format!("https://claude.ai/api/organizations/{org_id}/usage");
	let body = fetch_authenticated(&url, browser, profile)?;
	serde_json::from_str(&body)
		.map_err(|e| UsageError::Other(format!("parsing usage response: {e}")))
}

pub(crate) fn fetch_credits(
	org_id: &str,
	browser: Browser,
	profile: Option<&str>,
) -> Result<PrepaidCredits, UsageError> {
	let url = format!("https://claude.ai/api/organizations/{org_id}/prepaid/credits");
	let body = fetch_authenticated(&url, browser, profile)?;
	serde_json::from_str(&body)
		.map_err(|e| UsageError::Other(format!("parsing credits response: {e}")))
}

fn fetch_authenticated(
	url: &str,
	browser: Browser,
	profile: Option<&str>,
) -> Result<String, UsageError> {
	let session_key = browser::load_session_key(browser, profile).map_err(|e| {
		if e.to_string().contains("sessionKey cookie not found") {
			UsageError::NotLoggedIn
		} else {
			UsageError::Other(e.to_string())
		}
	})?;

	let response = ureq::get(url)
		.header("Cookie", &format!("sessionKey={session_key}"))
		.header("User-Agent", USER_AGENT)
		.header("Accept", "application/json")
		.config()
		.timeout_global(Some(FETCH_TIMEOUT))
		.build()
		.call()
		.map_err(|e| UsageError::Other(format!("fetching {url}: {e}")))?;

	response
		.into_body()
		.read_to_string()
		.map_err(|e| UsageError::Other(format!("reading response body: {e}")))
}
