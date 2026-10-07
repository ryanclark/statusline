//! Headless Chrome, which lays out and captures each page.
//!
//! `CHROME_ARGS` adds flags, such as `--no-sandbox` where Chrome's sandbox cannot nest. Chrome never exits in some of
//! those environments, so neither call waits for it to, and both kill it once they have what they need.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use eyre::{Result, WrapErr, bail, eyre};

const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const TIMEOUT: Duration = Duration::from_secs(30);

/// The page's laid out size in CSS pixels, which its script leaves in `data-size` on the body.
pub fn measure(page: &Path, work: &Path) -> Result<(u32, u32)> {
	let mut child = launch(
		work,
		&["--virtual-time-budget=3000", "--dump-dom", &file_url(page)],
		Stdio::piped(),
	)?;
	let stdout = child.stdout.take().expect("stdout is piped");
	let (tx, rx) = mpsc::channel();
	thread::spawn(move || {
		let mut dom = String::new();
		for line in BufReader::new(stdout)
			.lines()
			.map_while(std::result::Result::ok)
		{
			dom.push_str(&line);
			dom.push('\n');
			if line.contains("</html>") {
				break;
			}
		}
		// The receiver is gone only after a timeout, when the DOM is no longer wanted.
		let _ = tx.send(dom);
	});
	let dom = rx.recv_timeout(TIMEOUT);
	stop(&mut child);
	let dom = dom.map_err(|_| eyre!("chrome did not dump {} in time", page.display()))?;
	parse_size(&dom).ok_or_else(|| eyre!("could not measure {}", page.display()))
}

/// Saves `page` as a PNG at `out`, at twice its CSS size.
pub fn capture(page: &Path, out: &Path, (width, height): (u32, u32), work: &Path) -> Result<()> {
	let mut child = launch(
		work,
		&[
			"--force-device-scale-factor=2",
			"--default-background-color=00000000",
			&format!("--window-size={width},{height}"),
			&format!("--screenshot={}", out.display()),
			&file_url(page),
		],
		Stdio::null(),
	)?;
	let started = Instant::now();
	while started.elapsed() < TIMEOUT {
		if matches!(child.try_wait(), Ok(Some(_))) || out.exists() {
			break;
		}
		thread::sleep(Duration::from_millis(100));
	}
	// The file can appear before Chrome has finished writing it.
	thread::sleep(Duration::from_millis(500));
	stop(&mut child);
	if !out.exists() {
		bail!("chrome did not write {}", out.display());
	}
	Ok(())
}

fn launch(work: &Path, args: &[&str], stdout: Stdio) -> Result<Child> {
	let extra = std::env::var("CHROME_ARGS").unwrap_or_default();
	Command::new(CHROME)
		.arg("--headless=new")
		.arg(format!("--user-data-dir={}", work.join("chrome").display()))
		.args(["--hide-scrollbars", "--allow-file-access-from-files"])
		.args(extra.split_whitespace())
		.args(args)
		.stdout(stdout)
		.stderr(Stdio::null())
		.spawn()
		.wrap_err("launching chrome")
}

fn stop(child: &mut Child) {
	// Either call fails only when Chrome has already exited and been reaped, which is the state wanted anyway.
	let _ = child.kill();
	let _ = child.wait();
}

/// A `file://` URL for `path`, percent-encoding every byte that is not unreserved, so spaces and `#` in a temp dir
/// or HOME survive.
pub fn file_url(path: &Path) -> String {
	let mut url = "file://".to_owned();
	for &byte in path.as_os_str().as_encoded_bytes() {
		if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
			url.push(char::from(byte));
		} else {
			url.push_str(&format!("%{byte:02X}"));
		}
	}
	url
}

fn parse_size(dom: &str) -> Option<(u32, u32)> {
	let start = dom.find("data-size=\"")? + "data-size=\"".len();
	let value = &dom[start..start + dom[start..].find('"')?];
	let (width, height) = value.split_once('x')?;
	Some((width.parse().ok()?, height.parse().ok()?))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn size_is_read_from_the_body_attribute() {
		let dom = r#"<html><body data-size="812x290"><div></div></body></html>"#;
		assert_eq!(parse_size(dom), Some((812, 290)));
		assert_eq!(parse_size("<body>"), None);
	}

	#[test]
	fn file_urls_percent_encode_unsafe_bytes() {
		assert_eq!(
			file_url(Path::new("/tmp/a b#c/é.html")),
			"file:///tmp/a%20b%23c/%C3%A9.html"
		);
	}
}
