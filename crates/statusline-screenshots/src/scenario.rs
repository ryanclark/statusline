use std::path::Path;

use eyre::{Result, WrapErr, bail, eyre};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One screenshot, as described by a file in `screenshots/scenarios`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
	/// Written to `settings.json` as is, since entries are segment names or segment objects.
	pub segments: Value,
	pub subagent_segments: Option<Value>,
	#[serde(default)]
	pub settings: Map<String, Value>,
	#[serde(default)]
	pub git: GitSpec,
	/// The project directory, relative to the scratch HOME.
	#[serde(default = "default_cwd")]
	pub cwd: String,
	#[serde(default)]
	pub transcript: Vec<String>,
	#[serde(default)]
	pub prompt: String,
	/// Claude Code's mode label. Null or empty draws none.
	#[serde(default = "default_mode")]
	pub mode: Option<String>,
	/// The plugin draws on the mode row, a native `statusLine` on a row of its own.
	#[serde(default = "default_plugin")]
	pub plugin: bool,
	#[serde(default = "default_plan")]
	pub plan: String,
	/// More than one renders an animated PNG with a frame a second.
	#[serde(default = "default_frames")]
	pub frames: u32,
	pub input: Map<String, Value>,
	/// Absent draws no subagent panel.
	pub tasks: Option<Value>,
}

/// The state of the fixture repo the git segments read.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GitSpec {
	pub branch: String,
	pub ahead: u32,
	pub stash: u32,
	pub dirty: bool,
}

impl Default for GitSpec {
	fn default() -> Self {
		Self {
			branch: "main".to_owned(),
			ahead: 0,
			stash: 0,
			dirty: false,
		}
	}
}

fn default_cwd() -> String {
	"code/statusline".to_owned()
}

fn default_mode() -> Option<String> {
	Some("auto mode on".to_owned())
}

fn default_plugin() -> bool {
	true
}

fn default_plan() -> String {
	"Claude Max".to_owned()
}

fn default_frames() -> u32 {
	1
}

impl Scenario {
	pub fn load(path: &Path) -> Result<Self> {
		let text = std::fs::read_to_string(path)
			.wrap_err_with(|| format!("reading {}", path.display()))?;
		let scenario: Self =
			serde_json::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))?;
		if scenario.frames == 0 {
			bail!("{}: frames must be at least 1", path.display());
		}
		Ok(scenario)
	}
}

/// The instant a frame is drawn at, which relative times in a scenario resolve against.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
	pub secs: i64,
	pub millis: i64,
}

/// Replaces strings like `"@+3240"` with epoch seconds and `"@ms-12000"` with epoch milliseconds, relative to `now`.
pub fn resolve_times(value: &mut Value, now: Clock) -> Result<()> {
	match value {
		Value::Object(map) => map.values_mut().try_for_each(|v| resolve_times(v, now)),
		Value::Array(items) => items.iter_mut().try_for_each(|v| resolve_times(v, now)),
		Value::String(s) => {
			let resolved = if let Some(offset) = s.strip_prefix("@ms") {
				now.millis + parse_offset(offset, s)?
			} else if let Some(offset) = s.strip_prefix('@') {
				now.secs + parse_offset(offset, s)?
			} else {
				return Ok(());
			};
			*value = resolved.into();
			Ok(())
		}
		_ => Ok(()),
	}
}

fn parse_offset(offset: &str, whole: &str) -> Result<i64> {
	offset
		.parse()
		.map_err(|_| eyre!("{whole:?} is not a relative time like \"@+60\" or \"@ms-1000\""))
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn relative_times_resolve_in_nested_values() {
		let mut value = json!({"a": ["@+60", "@ms-1500"], "b": {"c": "@-5"}, "d": "plain", "e": 3});
		let now = Clock {
			secs: 1_000,
			millis: 1_000_250,
		};
		resolve_times(&mut value, now).unwrap();
		assert_eq!(
			value,
			json!({"a": [1060, 998_750], "b": {"c": 995}, "d": "plain", "e": 3})
		);
	}

	#[test]
	fn a_malformed_relative_time_is_an_error() {
		let now = Clock { secs: 0, millis: 0 };
		assert!(resolve_times(&mut json!("@soon"), now).is_err());
	}

	#[test]
	fn every_scenario_parses() {
		let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../screenshots/scenarios");
		for entry in std::fs::read_dir(dir).unwrap() {
			Scenario::load(&entry.unwrap().path()).unwrap();
		}
	}
}
