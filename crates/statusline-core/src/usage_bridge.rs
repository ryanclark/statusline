use crate::browser::Browser;
use serde::{Deserialize, Serialize};

pub const ERROR_NOT_LOGGED_IN: &str = "not_logged_in";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRequest {
	pub org_id: String,
	pub browser: Browser,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub profile: Option<String>,
}

impl UsageRequest {
	#[must_use]
	pub fn new(org_id: impl Into<String>, browser: Browser, profile: Option<String>) -> Self {
		Self {
			org_id: org_id.into(),
			browser,
			profile,
		}
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageReply {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub usage: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub credits: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

impl UsageReply {
	#[must_use]
	pub fn error(message: impl Into<String>) -> Self {
		Self {
			usage: None,
			credits: None,
			error: Some(message.into()),
		}
	}

	#[must_use]
	pub fn is_not_logged_in(&self) -> bool {
		self.error.as_deref() == Some(ERROR_NOT_LOGGED_IN)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn request_round_trips_with_lowercase_browser_and_optional_profile() {
		let req = UsageRequest::new("org-123", Browser::Chrome, Some("Profile 2".to_owned()));
		let json = serde_json::to_string(&req).unwrap();
		assert!(json.contains("\"browser\":\"chrome\""), "{json}");
		assert!(json.contains("\"org_id\":\"org-123\""), "{json}");
		assert!(json.contains("\"profile\":\"Profile 2\""), "{json}");
		let back: UsageRequest = serde_json::from_str(&json).unwrap();
		assert_eq!(back.org_id, "org-123");
		assert_eq!(back.browser, Browser::Chrome);
		assert_eq!(back.profile.as_deref(), Some("Profile 2"));
	}

	#[test]
	fn request_omits_absent_profile_and_parses_it_back_as_none() {
		let req = UsageRequest::new("o", Browser::Brave, None);
		let json = serde_json::to_string(&req).unwrap();
		assert!(
			!json.contains("profile"),
			"absent profile must be omitted: {json}"
		);
		let back: UsageRequest = serde_json::from_str(&json).unwrap();
		assert!(back.profile.is_none());
	}

	#[test]
	fn reply_skips_none_fields_and_round_trips() {
		let reply = UsageReply {
			usage: Some("{\"extra_usage\":null}".to_owned()),
			credits: None,
			error: None,
		};
		let json = serde_json::to_string(&reply).unwrap();
		assert!(json.contains("usage"), "{json}");
		assert!(
			!json.contains("credits"),
			"none credits must be omitted: {json}"
		);
		assert!(
			!json.contains("error"),
			"none error must be omitted: {json}"
		);
		let back: UsageReply = serde_json::from_str(&json).unwrap();
		assert_eq!(back.usage.as_deref(), Some("{\"extra_usage\":null}"));
		assert!(back.credits.is_none() && back.error.is_none());
	}

	#[test]
	fn error_reply_helper_and_not_logged_in_detection() {
		let nope = UsageReply::error(ERROR_NOT_LOGGED_IN);
		assert!(nope.is_not_logged_in());
		assert!(nope.usage.is_none() && nope.credits.is_none());
		let other = UsageReply::error("boom");
		assert!(!other.is_not_logged_in());
	}
}
