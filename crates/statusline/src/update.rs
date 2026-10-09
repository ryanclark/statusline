use crate::util::app_data_dir;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY_INTERVAL: Duration = Duration::from_secs(60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) struct UpdateAvailable {
	pub(crate) version: String,
}

pub(crate) fn check() -> Option<UpdateAvailable> {
	let cache_path = app_data_dir().ok()?.join("latest_version");
	check_at(&cache_path, || spawn_refresh(&cache_path))
}

fn check_at(path: &Path, spawn: impl FnOnce()) -> Option<UpdateAvailable> {
	let cached = read_cache(path);
	if cached.as_ref().is_none_or(|(_, fresh)| !fresh) {
		spawn();
	}
	let (latest, _) = cached?;

	if is_newer(&latest, CURRENT_VERSION) {
		Some(UpdateAvailable { version: latest })
	} else {
		None
	}
}

fn spawn_refresh(path: &Path) {
	let Some(lock) = crate::background::claim(path, RETRY_INTERVAL) else {
		return;
	};
	let Ok(exe) = std::env::current_exe() else {
		return;
	};
	let mut command = Command::new(exe);
	command
		.arg("update-refresh")
		.stdin(Stdio::from(lock))
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		command.process_group(0);
	}
	let _ = command.spawn();
}

pub(crate) fn refresh() {
	let Some(path) = app_data_dir().ok().map(|dir| dir.join("latest_version")) else {
		return;
	};
	// Another chat can finish between the initial cache read and acquiring the worker lock.
	if read_cache(&path).is_some_and(|(_, fresh)| fresh) {
		return;
	}
	if let Some(latest) = fetch_latest() {
		write_cache(&path, &latest);
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
		now.checked_sub(timestamp)
			.is_some_and(|age| age < CHECK_INTERVAL.as_secs()),
	))
}

fn write_cache(path: &Path, version: &str) {
	let Ok(now) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
		return;
	};
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
	if std::fs::write(&tmp, format!("{version}\n{}", now.as_secs()))
		.and_then(|()| std::fs::rename(&tmp, path))
		.is_err()
	{
		let _ = std::fs::remove_file(tmp);
	}
}

fn fetch_latest() -> Option<String> {
	fetch_via_gh().or_else(fetch_via_api)
}

fn fetch_via_gh() -> Option<String> {
	let output = crate::background::bounded_output(
		Command::new("gh").args([
			"api",
			"repos/ryanclark/statusline/releases/latest",
			"--jq",
			".tag_name",
		]),
		Instant::now() + FETCH_TIMEOUT,
	)?;

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

	#[test]
	fn fresh_checks_are_shared_and_stale_results_remain_available_without_waiting() {
		let dir = std::env::temp_dir().join(format!("statusline-update-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("latest_version");
		write_cache(&path, "99.0.0");
		for _ in 0..2 {
			assert_eq!(
				check_at(&path, || panic!("fresh cache must not fetch"))
					.unwrap()
					.version,
				"99.0.0"
			);
		}
		std::fs::write(&path, "99.0.0\n0").unwrap();
		let mut scheduled = false;
		assert_eq!(
			check_at(&path, || {
				scheduled = true;
			})
			.unwrap()
			.version,
			"99.0.0"
		);
		assert!(scheduled);
		// Offline attempts are shared too, but can retry sooner than a successful daily check.
		let lock = crate::background::claim(&path, RETRY_INTERVAL).unwrap();
		assert!(crate::background::claim(&path, RETRY_INTERVAL).is_none());
		drop(lock);
		assert!(crate::background::claim(&path, RETRY_INTERVAL).is_none());
		std::fs::remove_dir_all(dir).unwrap();
	}
}
