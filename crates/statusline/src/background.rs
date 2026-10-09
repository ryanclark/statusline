//! Coordination and deadlines for disposable background refresh processes.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

pub(crate) fn claim(path: &Path, interval: Duration) -> Option<File> {
	fs::create_dir_all(path.parent()?).ok()?;
	let stamp = path.with_extension("refresh");
	let lock = OpenOptions::new()
		.read(true)
		.write(true)
		.create(true)
		.truncate(false)
		.open(&stamp)
		.ok()?;
	lock.try_lock().ok()?;
	let metadata = lock.metadata().ok()?;
	// Back off failures too. A hung worker keeps the lock even after this timestamp expires.
	if metadata.len() > 0
		&& metadata
			.modified()
			.ok()?
			.elapsed()
			.is_ok_and(|age| age < interval)
	{
		return None;
	}
	lock.set_len(1).ok()?;
	lock.set_modified(SystemTime::now()).ok()?;
	Some(lock)
}

pub(crate) fn bounded_output(command: &mut Command, deadline: Instant) -> Option<Output> {
	if Instant::now() >= deadline {
		return None;
	}
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		command.process_group(0);
	}
	let mut child = command
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::null())
		.spawn()
		.ok()?;
	let mut stdout = child.stdout.take()?;
	let (send, receive) = std::sync::mpsc::channel();
	// Drain while Git runs so a large status cannot fill its pipe. A descendant holding it open is bounded by the
	// same deadline; a detached reader cannot hold this short-lived worker open.
	std::thread::spawn(move || {
		let mut bytes = Vec::new();
		let result = stdout
			.by_ref()
			.take(8 * 1024 * 1024)
			.read_to_end(&mut bytes)
			// Keep draining after the capture limit. Closing the pipe early kills large Git statuses with SIGPIPE.
			.and_then(|_| std::io::copy(&mut stdout, &mut std::io::sink()))
			.map(|_| bytes);
		let _ = send.send(result);
	});
	// Usually EOF means the process has exited, so wait on the pipe instead of polling every few milliseconds for
	// the entire command. A process that closes stdout early still has to exit before the same deadline.
	let Ok(Ok(stdout)) = receive.recv_timeout(deadline.saturating_duration_since(Instant::now()))
	else {
		stop(&mut child);
		return None;
	};
	loop {
		match child.try_wait() {
			Ok(Some(status)) => {
				return Some(Output {
					status,
					stdout,
					stderr: Vec::new(),
				});
			}
			Ok(None) if Instant::now() < deadline => {
				std::thread::sleep(Duration::from_millis(10));
			}
			_ => {
				stop(&mut child);
				return None;
			}
		}
	}
}

fn stop(child: &mut std::process::Child) {
	// Git can start helpers (fsmonitor and submodule status, for example). Stop the whole group so repeated refreshes
	// cannot accumulate helpers after Git itself has exited or timed out.
	#[cfg(unix)]
	let _ = rustix::process::kill_process_group(
		rustix::process::Pid::from_child(child),
		rustix::process::Signal::KILL,
	);
	let _ = child.kill();
	let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
	use super::*;

	#[test]
	fn large_output_is_drained_without_killing_the_command() {
		let out = bounded_output(
			Command::new("head").args(["-c", "9000000", "/dev/zero"]),
			Instant::now() + Duration::from_secs(3),
		)
		.unwrap();
		assert!(out.status.success(), "{}", out.status);
		assert_eq!(out.stdout.len(), 8 * 1024 * 1024);
	}

	#[test]
	fn timeout_stops_descendants_as_well_as_the_direct_child() {
		let marker =
			std::env::temp_dir().join(format!("statusline-killed-helper-{}", std::process::id()));
		let _ = fs::remove_file(&marker);
		assert!(
			bounded_output(
				Command::new("/bin/sh")
					.args(["-c", "(sleep 0.4; echo leaked > \"$1\") & wait", "sh"])
					.arg(&marker),
				Instant::now() + Duration::from_millis(100),
			)
			.is_none()
		);
		std::thread::sleep(Duration::from_millis(500));
		let leaked = marker.exists();
		let _ = fs::remove_file(marker);
		assert!(!leaked, "a descendant survived the process-group timeout");
	}
}
