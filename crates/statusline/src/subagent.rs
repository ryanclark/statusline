use crate::settings::Settings;
use statusline_core::constants::DIVIDER;
use statusline_core::subagent::{SubagentInput, default_subagent_segments, render_rows};
use std::io::{IsTerminal, Write};

pub(crate) fn run() {
	let stdin = std::io::stdin();
	if stdin.is_terminal() {
		eprintln!("statusline subagent reads Claude Code's subagentStatusLine JSON on stdin");
		return;
	}

	// Writing nothing keeps Claude Code's default rows, which beats a half-rendered panel.
	let Ok(input) = SubagentInput::from_reader(stdin.lock()) else {
		return;
	};

	let settings = Settings::load().ok();
	let segments = settings
		.as_ref()
		.and_then(|s| s.subagent_segments.clone())
		.unwrap_or_else(default_subagent_segments);
	let divider = settings
		.as_ref()
		.and_then(|s| s.divider.clone())
		.unwrap_or_else(|| DIVIDER.to_owned());
	let nerd_font = settings.as_ref().is_some_and(|s| s.nerd_font);

	let mut out = std::io::stdout().lock();
	for row in render_rows(&input, &segments, &divider, nerd_font) {
		if let Ok(line) = serde_json::to_string(&row) {
			let _ = writeln!(out, "{line}");
		}
	}
}
