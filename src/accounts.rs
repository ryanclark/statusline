use crate::browser::Browser;
use crate::segment::SegmentConfig;
use crate::util::{app_data_dir, home_dir};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub(crate) struct AccountsFile {
	#[serde(default)]
	pub(crate) accounts: Vec<AccountEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AccountEntry {
	pub(crate) nickname: String,
	pub(crate) email: String,
	#[serde(default)]
	pub(crate) organization_uuid: String,
	#[serde(default)]
	pub(crate) color: Option<String>,
	#[serde(default)]
	pub(crate) browser: Option<Browser>,
	#[serde(default)]
	pub(crate) profile: Option<String>,
	#[serde(default)]
	pub(crate) segments: Option<Vec<SegmentConfig>>,
}

#[derive(Debug, Deserialize)]
struct ClaudeConfig {
	#[serde(rename = "oauthAccount")]
	oauth_account: Option<OauthAccount>,
}

#[derive(Debug, Deserialize, Default)]
struct OauthAccount {
	#[serde(default, rename = "emailAddress")]
	email_address: String,
	#[serde(default, rename = "organizationUuid")]
	organization_uuid: String,
}

fn claude_config_path() -> Option<PathBuf> {
	if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
		let legacy = PathBuf::from(&dir).join(".config.json");
		if legacy.exists() {
			return Some(legacy);
		}
		return Some(PathBuf::from(dir).join(".claude.json"));
	}
	let home = home_dir().ok()?;
	let legacy = home.join(".claude").join(".config.json");
	if legacy.exists() {
		return Some(legacy);
	}
	Some(home.join(".claude.json"))
}

fn accounts_sidecar_path() -> Option<PathBuf> {
	Some(app_data_dir().ok()?.join("accounts.json"))
}

pub(crate) fn live_identity() -> Option<(String, String)> {
	let path = claude_config_path()?;
	let data = fs::read_to_string(&path).ok()?;
	let cfg: ClaudeConfig = serde_json::from_str(&data).ok()?;
	let oauth = cfg.oauth_account?;
	if oauth.email_address.is_empty() {
		return None;
	}
	Some((oauth.email_address, oauth.organization_uuid))
}

pub(crate) fn load() -> Option<AccountsFile> {
	let path = accounts_sidecar_path()?;
	let data = fs::read_to_string(&path).ok()?;
	serde_json::from_str(&data).ok()
}

pub(crate) fn find_for_identity<'a>(
	file: &'a AccountsFile,
	email: &str,
	org_uuid: &str,
) -> Option<&'a AccountEntry> {
	file
		.accounts
		.iter()
		.find(|a| a.email == email && a.organization_uuid == org_uuid)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_minimal_entry() {
		let raw = r#"{"accounts": [{"nickname": "p", "email": "a@b", "organization_uuid": "u"}]}"#;
		let file: AccountsFile = serde_json::from_str(raw).unwrap();
		assert_eq!(file.accounts.len(), 1);
		assert!(file.accounts[0].browser.is_none());
		assert!(file.accounts[0].profile.is_none());
		assert!(file.accounts[0].segments.is_none());
	}

	#[test]
	fn parses_entry_with_browser_profile_segments() {
		let raw = r#"{
			"accounts": [{
				"nickname": "work",
				"email": "x@y.com",
				"organization_uuid": "abc",
				"browser": "brave",
				"profile": "Profile 2",
				"segments": ["context_percentage", "divider", "extra_usage"]
			}]
		}"#;
		let file: AccountsFile = serde_json::from_str(raw).unwrap();
		let entry = &file.accounts[0];
		assert_eq!(entry.browser, Some(Browser::Brave));
		assert_eq!(entry.profile.as_deref(), Some("Profile 2"));
		assert_eq!(entry.segments.as_ref().unwrap().len(), 3);
	}

	#[test]
	fn find_for_identity_matches() {
		let file: AccountsFile = serde_json::from_str(
			r#"{"accounts": [
				{"nickname": "a", "email": "a@x", "organization_uuid": "1"},
				{"nickname": "b", "email": "b@x", "organization_uuid": "2"}
			]}"#,
		)
		.unwrap();
		let hit = find_for_identity(&file, "b@x", "2").unwrap();
		assert_eq!(hit.nickname, "b");
		assert!(find_for_identity(&file, "b@x", "1").is_none());
	}

	#[test]
	fn parses_claude_config_oauth() {
		let raw =
			r#"{"oauthAccount": {"emailAddress": "ryan@example.com", "organizationUuid": "abc"}}"#;
		let cfg: ClaudeConfig = serde_json::from_str(raw).unwrap();
		let oauth = cfg.oauth_account.unwrap();
		assert_eq!(oauth.email_address, "ryan@example.com");
		assert_eq!(oauth.organization_uuid, "abc");
	}

	#[test]
	fn claude_config_without_oauth_is_none() {
		let cfg: ClaudeConfig = serde_json::from_str("{}").unwrap();
		assert!(cfg.oauth_account.is_none());
	}
}
