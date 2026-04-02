use crate::util::home_dir;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Browser {
	Chrome,
	Brave,
	Firefox,
}

struct ChromiumConfig {
	db_rel_path: &'static str,
	keychain_service: &'static str,
	keychain_user: &'static str,
	cache_name: &'static str,
}

const CHROME_CONFIG: ChromiumConfig = ChromiumConfig {
	db_rel_path: "Library/Application Support/Google/Chrome/Default/Cookies",
	keychain_service: "Chrome Safe Storage",
	keychain_user: "Chrome",
	cache_name: "chrome_key",
};

const BRAVE_CONFIG: ChromiumConfig = ChromiumConfig {
	db_rel_path: "Library/Application Support/BraveSoftware/Brave-Browser/Default/Cookies",
	keychain_service: "Brave Safe Storage",
	keychain_user: "Brave",
	cache_name: "brave_key",
};

impl Browser {
	pub(crate) fn detect_or_cached() -> Result<Self> {
		use std::time::{Duration, SystemTime};

		const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

		let cache_path = crate::util::app_data_dir()?.join("browser");

		if let Ok(meta) = std::fs::metadata(&cache_path) {
			let fresh = meta
				.modified()
				.ok()
				.and_then(|m| SystemTime::now().duration_since(m).ok())
				.is_some_and(|age| age < CACHE_TTL);

			if fresh {
				if let Ok(cached) = std::fs::read_to_string(&cache_path) {
					let cached = cached.trim();
					if let Ok(browser) = serde_json::from_str::<Self>(&format!("\"{cached}\"")) {
						return Ok(browser);
					}
				}
			}
		}

		let browser = Self::detect()?;

		if let Some(parent) = cache_path.parent() {
			std::fs::create_dir_all(parent)?;
		}

		std::fs::write(&cache_path, format!("{browser}"))?;

		Ok(browser)
	}

	fn detect() -> Result<Self> {
		let home = home_dir()?;
		let plist_path = home
			.join("Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure");

		let output = std::process::Command::new("defaults")
			.args(["read", &plist_path.to_string_lossy(), "LSHandlers"])
			.output()
			.context("running defaults command")?;

		let text = String::from_utf8_lossy(&output.stdout);

		for block in text.split('{') {
			if !block.contains("LSHandlerURLScheme = https") {
				continue;
			}

			if let Some(browser) = parse_bundle_id(block) {
				return Ok(browser);
			}
		}

		Ok(Self::Chrome)
	}

	pub(crate) fn load_session_key(self) -> Result<String> {
		match self {
			Self::Chrome => chromium_session_key(&CHROME_CONFIG),
			Self::Brave => chromium_session_key(&BRAVE_CONFIG),
			Self::Firefox => firefox_session_key(),
		}
	}
}

impl fmt::Display for Browser {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Chrome => write!(f, "Chrome"),
			Self::Brave => write!(f, "Brave"),
			Self::Firefox => write!(f, "Firefox"),
		}
	}
}

fn parse_bundle_id(block: &str) -> Option<Browser> {
	for line in block.lines() {
		let line = line.trim();
		if let Some(rest) = line.strip_prefix("LSHandlerRoleAll = ") {
			let bundle_id = rest.trim_end_matches(';').trim().trim_matches('"');
			return match bundle_id {
				"com.google.chrome" => Some(Browser::Chrome),
				"com.brave.browser" | "com.brave.Browser" => Some(Browser::Brave),
				"org.mozilla.firefox" => Some(Browser::Firefox),
				_ => None,
			};
		}
	}
	None
}

fn chromium_session_key(config: &ChromiumConfig) -> Result<String> {
	let home = home_dir()?;

	let db_path = home.join(config.db_rel_path);
	let conn = rusqlite::Connection::open_with_flags(
		&db_path,
		rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
	)
	.context(format!("opening {} cookies database", config.keychain_user))?;

	let (value, encrypted_value): (String, Vec<u8>) = conn.prepare(
		"SELECT value, encrypted_value FROM cookies WHERE host_key IN ('.claude.ai', 'claude.ai') AND name = 'sessionKey' LIMIT 1",
	)?.query_row([], |row| Ok((row.get(0)?, row.get(1)?)))
		.context("sessionKey cookie not found")?;

	if !value.is_empty() {
		return Ok(value);
	}

	let password = keychain_password(config)?;

	decrypt_cookie(&password, &encrypted_value)
}

fn firefox_session_key() -> Result<String> {
	let home = home_dir()?;
	let firefox_dir = home.join("Library/Application Support/Firefox");
	let profiles_ini = firefox_dir.join("profiles.ini");

	let profile_path = default_firefox_profile(&profiles_ini)?;
	let db_path = firefox_dir.join(profile_path).join("cookies.sqlite");

	let conn = rusqlite::Connection::open_with_flags(
		&db_path,
		rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
	)
	.context("opening Firefox cookies database")?;

	let value: String = conn.prepare(
		"SELECT value FROM moz_cookies WHERE host IN ('.claude.ai', 'claude.ai') AND name = 'sessionKey' LIMIT 1",
	)?.query_row([], |row| row.get(0))
		.context("sessionKey cookie not found in Firefox")?;

	Ok(value)
}

fn default_firefox_profile(profiles_ini: &Path) -> Result<String> {
	let content = std::fs::read_to_string(profiles_ini).context("reading Firefox profiles.ini")?;

	let mut current_section = String::new();
	let mut install_default: Option<String> = None;
	let mut first_profile_path: Option<String> = None;

	let mut section_path: Option<String> = None;
	let mut section_is_default = false;
	let mut default_profile_path: Option<String> = None;

	for line in content.lines() {
		let line = line.trim();

		if line.starts_with('[') && line.ends_with(']') {
			if current_section.starts_with("Profile") {
				if let Some(ref path) = section_path {
					if first_profile_path.is_none() {
						first_profile_path = Some(path.clone());
					}
					if section_is_default {
						default_profile_path = Some(path.clone());
					}
				}
			}

			current_section = line[1..line.len() - 1].to_owned();
			section_path = None;
			section_is_default = false;

			continue;
		}

		if let Some((key, val)) = line.split_once('=') {
			let key = key.trim();
			let val = val.trim();

			if current_section.starts_with("Install") && key == "Default" {
				install_default = Some(val.to_owned());
			} else if current_section.starts_with("Profile") {
				match key {
					"Path" => section_path = Some(val.to_owned()),
					"Default" if val == "1" => section_is_default = true,
					_ => {}
				}
			}
		}
	}

	if current_section.starts_with("Profile") {
		if let Some(ref path) = section_path {
			if first_profile_path.is_none() {
				first_profile_path = Some(path.clone());
			}
			if section_is_default {
				default_profile_path = Some(path.clone());
			}
		}
	}

	install_default
		.or(default_profile_path)
		.or(first_profile_path)
		.ok_or_else(|| eyre::eyre!("no Firefox profile found in profiles.ini"))
}

fn password_from_keychain(service: &str, user: &str) -> Result<Vec<u8>> {
	security_framework::passwords::get_generic_password(service, user)
		.context(format!("reading {service} from Keychain"))
}

#[cfg(feature = "codesigned")]
fn keychain_password(config: &ChromiumConfig) -> Result<Vec<u8>> {
	password_from_keychain(config.keychain_service, config.keychain_user)
}

#[cfg(not(feature = "codesigned"))]
fn keychain_password(config: &ChromiumConfig) -> Result<Vec<u8>> {
	use crate::util::app_data_dir;

	let cache = app_data_dir()?.join(config.cache_name);

	match std::fs::read(&cache) {
		Ok(cached) => return Ok(cached),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
		Err(e) => return Err(e).context(format!("reading cached {} key", config.keychain_user)),
	}

	let password = password_from_keychain(config.keychain_service, config.keychain_user)?;

	use std::os::unix::fs::PermissionsExt;

	if let Some(parent) = cache.parent() {
		std::fs::create_dir_all(parent)?;
		std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
	}

	std::fs::write(&cache, &password)?;
	std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o600))?;

	Ok(password)
}

fn decrypt_cookie(password: &[u8], encrypted_value: &[u8]) -> Result<String> {
	if encrypted_value.len() < 4 {
		return Err(eyre::eyre!("encrypted cookie value too short"));
	}

	let prefix = &encrypted_value[..3];
	match prefix {
		b"v10" | b"v11" => {
			let mut key = [0u8; 16];
			pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, b"saltysalt", 1003, &mut key);

			let mut buf = encrypted_value[3..].to_vec();
			let iv = [0x20u8; 16];

			let plaintext = cbc::Decryptor::<aes::Aes128>::new(&key.into(), &iv.into())
				.decrypt_padded_mut::<Pkcs7>(&mut buf)
				.map_err(|_| eyre::eyre!("cookie decryption failed"))?;

			match String::from_utf8(plaintext.to_vec()) {
				Ok(s) => Ok(s),
				Err(_) if plaintext.len() > 32 => {
					String::from_utf8(plaintext[32..].to_vec()).context("decrypted cookie is not valid UTF-8")
				}
				Err(e) => Err(e).context("decrypted cookie is not valid UTF-8"),
			}
		}
		_ => Err(eyre::eyre!(
			"unsupported cookie encryption version: {:?}",
			std::str::from_utf8(prefix).unwrap_or("<binary>")
		)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn decrypt_cookie_rejects_too_short() {
		let result = decrypt_cookie(b"password", &[0, 1]);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("too short"));
	}

	#[test]
	fn decrypt_cookie_rejects_unknown_prefix() {
		let result = decrypt_cookie(b"password", b"v99extradata");
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("unsupported"));
	}

	#[test]
	fn decrypt_cookie_v10_roundtrip() {
		use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};

		let password = b"test_password";
		let plaintext = b"sk-ant-session-key-12345";

		let mut key = [0u8; 16];
		pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, b"saltysalt", 1003, &mut key);

		let iv = [0x20u8; 16];
		let mut buf = [0u8; 128];
		buf[..plaintext.len()].copy_from_slice(plaintext);

		let ciphertext = cbc::Encryptor::<aes::Aes128>::new(&key.into(), &iv.into())
			.encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
			.unwrap();

		let mut encrypted = b"v10".to_vec();
		encrypted.extend_from_slice(ciphertext);

		let result = decrypt_cookie(password, &encrypted).unwrap();
		assert_eq!(result, "sk-ant-session-key-12345");
	}

	#[test]
	fn parse_bundle_id_chrome() {
		let block = r#"
            LSHandlerRoleAll = "com.google.chrome";
            LSHandlerURLScheme = https;
        "#;
		assert_eq!(parse_bundle_id(block), Some(Browser::Chrome));
	}

	#[test]
	fn parse_bundle_id_brave() {
		let block = r#"
            LSHandlerRoleAll = "com.brave.Browser";
            LSHandlerURLScheme = https;
        "#;
		assert_eq!(parse_bundle_id(block), Some(Browser::Brave));
	}

	#[test]
	fn parse_bundle_id_firefox() {
		let block = r#"
            LSHandlerRoleAll = "org.mozilla.firefox";
            LSHandlerURLScheme = https;
        "#;
		assert_eq!(parse_bundle_id(block), Some(Browser::Firefox));
	}

	#[test]
	fn parse_bundle_id_unknown_returns_none() {
		let block = r#"
            LSHandlerRoleAll = "com.apple.Safari";
            LSHandlerURLScheme = https;
        "#;
		assert_eq!(parse_bundle_id(block), None);
	}

	fn write_ini(name: &str, content: &str) -> std::path::PathBuf {
		let path = std::env::temp_dir().join(name);
		std::fs::write(&path, content).unwrap();
		path
	}

	#[test]
	fn firefox_profile_install_section() {
		let path = write_ini("statusline_test_profiles.ini",
			"[Install12345]\nDefault=Profiles/abc.default-release\n\n[Profile0]\nName=default-release\nPath=Profiles/abc.default-release\nDefault=1\n");
		let result = default_firefox_profile(&path).unwrap();
		assert_eq!(result, "Profiles/abc.default-release");
	}

	#[test]
	fn firefox_profile_default_flag() {
		let path = write_ini("statusline_test_profiles2.ini",
			"[Profile0]\nName=default\nPath=Profiles/xyz.default\n\n[Profile1]\nName=default-release\nPath=Profiles/abc.default-release\nDefault=1\n");
		let result = default_firefox_profile(&path).unwrap();
		assert_eq!(result, "Profiles/abc.default-release");
	}

	#[test]
	fn firefox_profile_default_before_path() {
		let path = write_ini("statusline_test_profiles4.ini",
			"[Profile0]\nPath=Profiles/wrong.default\n\n[Profile1]\nDefault=1\nPath=Profiles/correct.default-release\n");
		let result = default_firefox_profile(&path).unwrap();
		assert_eq!(result, "Profiles/correct.default-release");
	}

	#[test]
	fn firefox_profile_fallback_first() {
		let path = write_ini("statusline_test_profiles3.ini",
			"[Profile0]\nName=default\nPath=Profiles/only.default\n");
		let result = default_firefox_profile(&path).unwrap();
		assert_eq!(result, "Profiles/only.default");
	}
}
