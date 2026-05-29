use crate::util::app_data_dir;
use std::path::Path;
use std::time::{Duration, SystemTime};

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) struct UpdateAvailable {
	pub(crate) version: String,
}

pub(crate) fn check() -> Option<UpdateAvailable> {
	let cache_path = app_data_dir().ok()?.join("latest_version");

	let latest = match read_cache(&cache_path) {
		Some((version, true)) => version,
		Some((version, false)) => {
			let fetched = fetch_latest().unwrap_or(version);
			write_cache(&cache_path, &fetched);
			fetched
		}
		None => {
			let fetched = fetch_latest()?;
			write_cache(&cache_path, &fetched);
			fetched
		}
	};

	if is_newer(&latest, CURRENT_VERSION) {
		Some(UpdateAvailable { version: latest })
	} else {
		None
	}
}

fn read_cache(path: &Path) -> Option<(String, bool)> {
	let content = std::fs::read_to_string(path).ok()?;
	let mut lines = content.lines();
	let version = lines.next()?.to_owned();
	let timestamp: u64 = lines.next()?.parse().ok()?;

	let now = SystemTime::now()
		.duration_since(SystemTime::UNIX_EPOCH)
		.ok()?
		.as_secs();

	Some((
		version,
		now.saturating_sub(timestamp) < CHECK_INTERVAL.as_secs(),
	))
}

fn write_cache(path: &Path, version: &str) {
	let Ok(now) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
		return;
	};
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	let _ = std::fs::write(path, format!("{version}\n{}", now.as_secs()));
}

fn fetch_latest() -> Option<String> {
	fetch_via_gh().or_else(fetch_via_api)
}

fn fetch_via_gh() -> Option<String> {
	let output = std::process::Command::new("gh")
		.args([
			"api",
			"repos/ryanclark/statusline/releases/latest",
			"--jq",
			".tag_name",
		])
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	let tag = String::from_utf8(output.stdout).ok()?;
	let tag = tag.trim();
	if tag.is_empty() {
		return None;
	}
	Some(tag.strip_prefix('v').unwrap_or(tag).to_owned())
}

fn fetch_via_api() -> Option<String> {
	let response = ureq::get("https://api.github.com/repos/ryanclark/statusline/releases/latest")
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "statusline-update-check")
		.config()
		.timeout_global(Some(FETCH_TIMEOUT))
		.build()
		.call()
		.ok()?;

	let body = response.into_body().read_to_string().ok()?;
	let json: serde_json::Value = serde_json::from_str(&body).ok()?;
	let tag = json["tag_name"].as_str()?;
	Some(tag.strip_prefix('v').unwrap_or(tag).to_owned())
}

fn is_newer(latest: &str, current: &str) -> bool {
	let Ok(latest) = semver::Version::parse(latest) else {
		return false;
	};
	let Ok(current) = semver::Version::parse(current) else {
		return false;
	};
	latest > current
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn newer_patch() {
		assert!(is_newer("0.1.3", "0.1.2"));
	}

	#[test]
	fn newer_minor() {
		assert!(is_newer("0.2.0", "0.1.2"));
	}

	#[test]
	fn newer_major() {
		assert!(is_newer("1.0.0", "0.1.2"));
	}

	#[test]
	fn same_version() {
		assert!(!is_newer("0.1.2", "0.1.2"));
	}

	#[test]
	fn older_version() {
		assert!(!is_newer("0.1.1", "0.1.2"));
	}

	#[test]
	fn cache_roundtrip() {
		let dir = std::env::temp_dir().join("statusline_test_cache");
		let _ = std::fs::create_dir_all(&dir);
		let path = dir.join("latest_version");

		write_cache(&path, "1.2.3");
		let (version, fresh) = read_cache(&path).unwrap();
		assert_eq!(version, "1.2.3");
		assert!(fresh);

		let _ = std::fs::remove_dir_all(&dir);
	}
}
