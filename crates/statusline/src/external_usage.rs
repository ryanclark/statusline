//! External programs run only in a disposable worker; the rendering process reads a scoped cache.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use statusline_core::external_usage::{
	ExternalUsageError, MAX_RESPONSE_BYTES, UsageRequest, UsageSnapshot, UsageSource,
	UsageSourceConfig,
};
use statusline_core::input::InputData;
use statusline_core::settings::Settings;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

pub(crate) fn resolve_source(
	settings: &Settings,
	lookup: impl Fn(&str) -> Option<String>,
) -> Result<Option<UsageSource>, ExternalUsageError> {
	let source = if let Some(raw) = lookup("STATUSLINE_USAGE_SOURCE") {
		Some(
			serde_json::from_str::<UsageSourceConfig>(&raw)
				.map_err(|_| ExternalUsageError::Configuration)?,
		)
	} else {
		settings.usage_source.clone()
	};
	match source {
		None => Ok(None),
		Some(UsageSourceConfig::Invalid(_)) => Err(ExternalUsageError::Configuration),
		Some(UsageSourceConfig::Valid(source)) => {
			source.validate()?;
			Ok(Some(source))
		}
	}
}

pub(crate) fn request(
	source: &UsageSource,
	input: &InputData,
	lookup: impl Fn(&str) -> Option<String>,
) -> Result<UsageRequest, ExternalUsageError> {
	let key = match &source.key_env {
		Some(name) => lookup(name).ok_or(ExternalUsageError::Identity)?,
		None => input.session_id.clone(),
	};
	let request = UsageRequest {
		version: 1,
		key,
		session_id: input.session_id.clone(),
		model_id: (!input.model.id.is_empty()).then(|| input.model.id.clone()),
	};
	validate_request(&request)?;
	Ok(request)
}
fn validate_request(request: &UsageRequest) -> Result<(), ExternalUsageError> {
	if request.version != 1
		|| request.key.trim().is_empty()
		|| request.key.len() > 4096
		|| request.key.contains('\0')
		|| serde_json::to_vec(request)
			.map_err(|_| ExternalUsageError::Schema)?
			.len() > 64 * 1024
	{
		return Err(ExternalUsageError::Identity);
	}
	Ok(())
}

#[derive(Serialize, Deserialize)]
struct Envelope {
	fingerprint: String,
	key: String,
	snapshot: UsageSnapshot,
}
#[derive(Serialize, Deserialize)]
struct Attempt {
	at: DateTime<Utc>,
	error: Option<String>,
}
pub(crate) struct CachedSnapshot {
	pub snapshot: UsageSnapshot,
	pub last_attempt: Option<DateTime<Utc>>,
}

fn location(source: &UsageSource, request: &UsageRequest) -> Option<(PathBuf, String)> {
	let bytes = serde_json::to_vec(&(1, source, &request.key)).ok()?;
	let fingerprint = format!("{:x}", Sha256::digest(bytes));
	Some((
		crate::util::app_data_dir()
			.ok()?
			.join("external-usage")
			.join(format!("{fingerprint}.json")),
		fingerprint,
	))
}
fn bounded_read(path: &Path, limit: u64) -> Option<Vec<u8>> {
	let mut bytes = Vec::new();
	fs::File::open(path)
		.ok()?
		.take(limit + 1)
		.read_to_end(&mut bytes)
		.ok()?;
	(u64::try_from(bytes.len()).ok()? <= limit).then_some(bytes)
}

pub(crate) fn read(source: &UsageSource, request: &UsageRequest) -> Option<CachedSnapshot> {
	let (path, fingerprint) = location(source, request)?;
	let envelope: Envelope = serde_json::from_slice(&bounded_read(&path, 2 * 1024 * 1024)?).ok()?;
	if envelope.fingerprint != fingerprint || envelope.key != request.key {
		return None;
	}
	let snapshot = UsageSnapshot::parse(
		&serde_json::to_vec(&envelope.snapshot).ok()?,
		&request.key,
		Utc::now(),
	)
	.ok()?;
	let attempt: Option<Attempt> = bounded_read(&path.with_extension("attempt.json"), 4096)
		.and_then(|bytes| serde_json::from_slice(&bytes).ok());
	Some(CachedSnapshot {
		snapshot,
		last_attempt: attempt.map(|a| a.at),
	})
}

pub(crate) fn maybe_spawn_refresh(source: &UsageSource, request: &UsageRequest) {
	let Some((path, _)) = location(source, request) else {
		return;
	};
	if fs::metadata(path.with_extension("refresh"))
		.and_then(|m| m.modified())
		.ok()
		.and_then(|at| at.elapsed().ok())
		.is_some_and(|age| age < source.refresh_interval)
	{
		return;
	}
	let Ok(exe) = std::env::current_exe() else {
		return;
	};
	let (Ok(source), Ok(request)) = (
		serde_json::to_string(source),
		serde_json::to_string(request),
	) else {
		return;
	};
	let mut command = Command::new(exe);
	command
		.args([
			"external-usage-refresh",
			"--source",
			&source,
			"--request",
			&request,
		])
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		command.process_group(0);
	}
	let _ = command.spawn();
}

pub(crate) fn run_refresh(
	source: &UsageSource,
	request: &UsageRequest,
) -> Result<(), ExternalUsageError> {
	source.validate()?;
	validate_request(request)?;
	let (path, fingerprint) = location(source, request).ok_or(ExternalUsageError::Cache)?;
	private_directory(path.parent().ok_or(ExternalUsageError::Cache)?)
		.map_err(|_| ExternalUsageError::Cache)?;
	let Some(_lock) = crate::background::claim(&path, source.refresh_interval) else {
		return Ok(());
	};
	let at = Utc::now();
	let result = fetch(source, request);
	atomic(
		&path.with_extension("attempt.json"),
		&Attempt {
			at,
			error: result.as_ref().err().map(ToString::to_string),
		},
	)?;
	let snapshot = result?;
	atomic(
		&path,
		&Envelope {
			fingerprint,
			key: request.key.clone(),
			snapshot,
		},
	)
}
fn fetch(
	source: &UsageSource,
	request: &UsageRequest,
) -> Result<UsageSnapshot, ExternalUsageError> {
	let mut command = Command::new(&source.command[0]);
	command.args(&source.command[1..]);
	let input = serde_json::to_vec(request).map_err(|_| ExternalUsageError::Schema)?;
	let deadline = Instant::now()
		.checked_add(source.timeout)
		.ok_or(ExternalUsageError::Configuration)?;
	let output = crate::background::bounded_output_with_input(
		&mut command,
		deadline,
		&input,
		u64::try_from(MAX_RESPONSE_BYTES).unwrap_or(1024 * 1024) + 1,
	)
	.map_err(|error| match error {
		crate::background::OutputError::Timeout => ExternalUsageError::Timeout,
		crate::background::OutputError::Process => ExternalUsageError::ProcessExit,
	})?;
	if !output.status.success() {
		return Err(ExternalUsageError::ProcessExit);
	}
	UsageSnapshot::parse(&output.stdout, &request.key, Utc::now())
}
fn private_directory(path: &Path) -> std::io::Result<()> {
	let mut builder = fs::DirBuilder::new();
	builder.recursive(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;
		builder.mode(0o700);
	}
	builder.create(path)
}
fn atomic(path: &Path, value: &impl Serialize) -> Result<(), ExternalUsageError> {
	let bytes = serde_json::to_vec(value).map_err(|_| ExternalUsageError::Schema)?;
	let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
	let result = (|| -> std::io::Result<()> {
		let mut options = OpenOptions::new();
		options.write(true).create_new(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options.mode(0o600);
		}
		let mut file = options.open(&temporary)?;
		file.write_all(&bytes)?;
		file.sync_all()?;
		fs::rename(&temporary, path)?;
		Ok(())
	})();
	if result.is_err() {
		let _ = fs::remove_file(&temporary);
	}
	result.map_err(|_| ExternalUsageError::Cache)
}

/// The session report follows the same source/key boundary as rendered usage.
pub(crate) fn report(
	source: &UsageSource,
	input: &InputData,
) -> Result<serde_json::Value, ExternalUsageError> {
	use statusline_core::external_usage::SnapshotState;
	let request = request(source, input, |name| std::env::var(name).ok())?;
	let cached = read(source, &request);
	maybe_spawn_refresh(source, &request);
	let Some(mut cached) = cached else {
		return Ok(serde_json::json!({"key":request.key,"state":"pending"}));
	};
	let now = Utc::now();
	let state = cached.snapshot.state(now, source.max_age);
	cached
		.snapshot
		.limits
		.retain(|limit| limit.resets_at.is_none_or(|at| at > now));
	if state == SnapshotState::Unusable {
		cached.snapshot.limits.clear();
		cached.snapshot.spend = None;
	}
	let mut value =
		serde_json::to_value(&cached.snapshot).map_err(|_| ExternalUsageError::Schema)?;
	value["state"] = serde_json::json!(match state {
		SnapshotState::Fresh => "fresh",
		SnapshotState::Stale => "stale",
		SnapshotState::Unusable => "unusable",
	});
	value["last_attempt"] = serde_json::json!(cached.last_attempt);
	Ok(value)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	#[test]
	fn source_override_is_authoritative_even_when_invalid_and_key_is_never_global() {
		let settings = Settings {
			usage_source: Some(
				serde_json::from_value(json!({"command":["provider"],"key_env":"ACCOUNT"}))
					.unwrap(),
			),
			..Settings::default()
		};
		assert!(resolve_source(&settings, |_| Some("broken".into())).is_err());
		let source = resolve_source(&settings, |_| None).unwrap().unwrap();
		let input = InputData::default();
		assert!(request(&source, &input, |_| None).is_err());
		let a = request(&source, &input, |_| Some("a".into())).unwrap();
		let b = request(&source, &input, |_| Some("b".into())).unwrap();
		assert_ne!(location(&source, &a), location(&source, &b));
	}
}
