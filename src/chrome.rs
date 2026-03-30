use crate::util::home_dir;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use eyre::{Context, Result};

pub(crate) fn load_session_key() -> Result<String> {
	let home_dir = home_dir()?;

	let db_path = home_dir.join("Library/Application Support/Google/Chrome/Default/Cookies");
	let conn = rusqlite::Connection::open_with_flags(
		&db_path,
		rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
	)
	.context("opening Chrome cookies database")?;

	let (value, encrypted_value): (String, Vec<u8>) = conn.prepare(
		"SELECT value, encrypted_value FROM cookies WHERE host_key IN ('.claude.ai', 'claude.ai') AND name = 'sessionKey' LIMIT 1",
	)?.query_row([], |row| Ok((row.get(0)?, row.get(1)?)))
		.context("sessionKey cookie not found")?;

	if !value.is_empty() {
		return Ok(value);
	}

	let password = keychain_password()?;

	decrypt_cookie(&password, &encrypted_value)
}

fn password_from_keychain() -> Result<Vec<u8>> {
	security_framework::passwords::get_generic_password("Chrome Safe Storage", "Chrome")
		.context("reading Chrome Safe Storage from Keychain")
}

#[cfg(feature = "codesigned")]
fn keychain_password() -> Result<Vec<u8>> {
	password_from_keychain()
}

#[cfg(not(feature = "codesigned"))]
fn keychain_password() -> Result<Vec<u8>> {
	use crate::util::app_data_dir;

	let cache = app_data_dir()?.join("chrome_key");

	match std::fs::read(&cache) {
		Ok(cached) => return Ok(cached),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
		Err(e) => return Err(e).context("reading cached Chrome key"),
	}

	let password = password_from_keychain()?;

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
		// Encrypt a known plaintext with the same PBKDF2 + AES-CBC pipeline
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
}
