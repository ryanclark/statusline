use crate::browser::Browser;
use crate::util::home_dir;
use eyre::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

pub(crate) struct ProfileRow {
	pub(crate) id: String,
	pub(crate) user: String,
	pub(crate) name: String,
}

pub(crate) fn list(browser: Browser) -> Result<Vec<ProfileRow>> {
	match browser {
		Browser::Chrome => chromium_profiles("Library/Application Support/Google/Chrome"),
		Browser::Brave => {
			chromium_profiles("Library/Application Support/BraveSoftware/Brave-Browser")
		}
		Browser::Firefox => firefox_profiles(),
	}
}

pub(crate) fn print(rows: &[ProfileRow]) {
	let id_w = rows.iter().map(|r| r.id.len()).max().unwrap_or(0).max(9);
	let user_w = rows.iter().map(|r| r.user.len()).max().unwrap_or(0).max(4);
	println!("{:id_w$}  {:user_w$}  NAME", "DIRECTORY", "USER");
	for r in rows {
		println!("{:id_w$}  {:user_w$}  {}", r.id, r.user, r.name);
	}
}

#[derive(Deserialize)]
struct LocalState {
	profile: ProfileBlock,
}

#[derive(Deserialize)]
struct ProfileBlock {
	info_cache: HashMap<String, InfoCacheEntry>,
}

#[derive(Deserialize, Default)]
struct InfoCacheEntry {
	#[serde(default)]
	name: String,
	#[serde(default)]
	user_name: String,
}

fn chromium_profiles(base_rel: &str) -> Result<Vec<ProfileRow>> {
	let home = home_dir()?;
	let path = home.join(base_rel).join("Local State");
	let data = fs::read_to_string(&path).context(format!("reading {}", path.display()))?;
	parse_chromium_local_state(&data)
}

fn parse_chromium_local_state(data: &str) -> Result<Vec<ProfileRow>> {
	let state: LocalState = serde_json::from_str(data).context("parsing Local State JSON")?;
	let mut rows: Vec<ProfileRow> = state
		.profile
		.info_cache
		.into_iter()
		.map(|(id, entry)| ProfileRow {
			id,
			user: entry.user_name,
			name: entry.name,
		})
		.collect();
	rows.sort_by(|a, b| a.id.cmp(&b.id));
	Ok(rows)
}

fn firefox_profiles() -> Result<Vec<ProfileRow>> {
	let home = home_dir()?;
	let ini_path = home
		.join("Library/Application Support/Firefox")
		.join("profiles.ini");
	let data = fs::read_to_string(&ini_path).context(format!("reading {}", ini_path.display()))?;
	Ok(parse_firefox_profiles(&data))
}

fn parse_firefox_profiles(content: &str) -> Vec<ProfileRow> {
	let mut rows = Vec::new();
	let mut current_section = String::new();
	let mut name = String::new();
	let mut path = String::new();

	for line in content.lines() {
		let line = line.trim();
		if line.starts_with('[') && line.ends_with(']') {
			flush_firefox_section(&mut rows, &current_section, &mut name, &mut path);
			current_section = line[1..line.len() - 1].to_owned();
			continue;
		}
		if let Some((k, v)) = line.split_once('=') {
			let k = k.trim();
			let v = v.trim();
			if current_section.starts_with("Profile") {
				match k {
					"Name" => name = v.to_owned(),
					"Path" => path = v.to_owned(),
					_ => {}
				}
			}
		}
	}
	flush_firefox_section(&mut rows, &current_section, &mut name, &mut path);
	rows.sort_by(|a, b| a.id.cmp(&b.id));
	rows
}

fn flush_firefox_section(
	rows: &mut Vec<ProfileRow>,
	section: &str,
	name: &mut String,
	path: &mut String,
) {
	if section.starts_with("Profile") && !path.is_empty() {
		rows.push(ProfileRow {
			id: std::mem::take(path),
			user: String::new(),
			name: std::mem::take(name),
		});
	} else {
		name.clear();
		path.clear();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_chromium_local_state() {
		let data = r#"{
			"profile": {
				"info_cache": {
					"Default": {"name": "Work", "user_name": "ryan@work.com"},
					"Profile 1": {"name": "Personal", "user_name": "ryan@home.com"}
				}
			}
		}"#;
		let rows = parse_chromium_local_state(data).unwrap();
		assert_eq!(rows.len(), 2);
		assert_eq!(rows[0].id, "Default");
		assert_eq!(rows[0].user, "ryan@work.com");
		assert_eq!(rows[0].name, "Work");
		assert_eq!(rows[1].id, "Profile 1");
		assert_eq!(rows[1].user, "ryan@home.com");
	}

	#[test]
	fn chromium_tolerates_missing_fields() {
		let data = r#"{"profile": {"info_cache": {"Default": {}}}}"#;
		let rows = parse_chromium_local_state(data).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].user, "");
		assert_eq!(rows[0].name, "");
	}

	#[test]
	fn parses_firefox_profiles_all_sections() {
		let data = "\
[Install123]
Default=Profiles/abc.default-release

[Profile0]
Name=default
Path=Profiles/xyz.default

[Profile1]
Name=default-release
Path=Profiles/abc.default-release
Default=1
";
		let rows = parse_firefox_profiles(data);
		assert_eq!(rows.len(), 2);
		assert_eq!(rows[0].id, "Profiles/abc.default-release");
		assert_eq!(rows[0].name, "default-release");
		assert_eq!(rows[1].id, "Profiles/xyz.default");
		assert_eq!(rows[1].name, "default");
	}
}
