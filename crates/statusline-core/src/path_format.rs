//! Purely textual directory display rules: no filesystem lookups or Git commands.
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PathFormat {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub trim_prefixes: Vec<String>,
	/// Maximum display columns. Missing or zero leaves the length unrestricted.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub max_width: Option<usize>,
}

impl PathFormat {
	#[must_use]
	pub fn is_default(&self) -> bool {
		self.trim_prefixes.is_empty() && self.max_width.is_none()
	}

	#[must_use]
	pub fn format(&self, path: &str, home: Option<&str>) -> String {
		let matched = self
			.trim_prefixes
			.iter()
			.filter_map(|prefix| {
				let expanded = if prefix == "~" {
					home?.to_owned()
				} else if let Some(rest) = prefix.strip_prefix("~/") {
					format!("{}/{rest}", home?.trim_end_matches('/'))
				} else {
					prefix.clone()
				};
				let prefix = if expanded == "/" {
					"/"
				} else {
					expanded.trim_end_matches('/')
				};
				strip_component_prefix(path, prefix).map(|rest| (prefix.len(), rest))
			})
			.max_by_key(|(len, _)| *len);

		let text = if let Some((_, rest)) = matched {
			join_prefix("…", rest)
		} else if let Some(rest) =
			home.and_then(|home| strip_component_prefix(path, home.trim_end_matches('/')))
		{
			join_prefix("~", rest)
		} else {
			path.to_owned()
		};
		match self.max_width.filter(|&width| width > 0) {
			Some(width) if text.width() > width => shorten(&text, width),
			_ => text,
		}
	}
}

fn strip_component_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
	if prefix.is_empty() {
		return None;
	}
	if prefix == "/" {
		return path.strip_prefix('/');
	}
	let rest = path.strip_prefix(prefix)?;
	if rest.is_empty() {
		Some(rest)
	} else {
		rest.strip_prefix('/')
	}
}

fn join_prefix(prefix: &str, rest: &str) -> String {
	if rest.is_empty() {
		prefix.to_owned()
	} else {
		format!("{prefix}/{rest}")
	}
}

fn shorten(path: &str, width: usize) -> String {
	let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
	let Some(leaf) = parts.last() else {
		return middle(path, width);
	};
	let prefix = if path.starts_with("~/") {
		"~/…/"
	} else if path.starts_with('/') {
		"/…/"
	} else {
		"…/"
	};
	// Remove the fewest leading directory components possible, keeping the longest readable suffix.
	for start in 1..parts.len() {
		let candidate = format!("{prefix}{}", parts[start..].join("/"));
		if candidate.width() <= width {
			return candidate;
		}
	}
	let candidate = format!("…/{leaf}");
	if candidate.width() <= width {
		return candidate;
	}
	// When even the final directory is too wide, keep its beginning and end (often a project name and suffix).
	middle(leaf, width)
}

fn middle(text: &str, width: usize) -> String {
	if text.width() <= width {
		return text.to_owned();
	}
	if width == 0 {
		return String::new();
	}
	let budget = width - 1;
	let mut left = String::new();
	let mut used = 0;
	for part in text.graphemes(true) {
		let w = part.width();
		if used + w > budget.div_ceil(2) {
			break;
		}
		left.push_str(part);
		used += w;
	}
	let mut right = Vec::new();
	for part in text[left.len()..].graphemes(true).rev() {
		let w = part.width();
		if used + w > budget {
			break;
		}
		right.push(part);
		used += w;
	}
	format!("{left}…{}", right.into_iter().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn longest_prefix_wins_on_whole_directory_boundaries() {
		let f = PathFormat {
			trim_prefixes: vec![
				"~/go/".into(),
				"~/go/src/remote/ryanclark".into(),
				"/work".into(),
			],
			max_width: None,
		};
		assert_eq!(
			f.format(
				"/Users/ryan/go/src/remote/ryanclark/statusline",
				Some("/Users/ryan")
			),
			"…/statusline"
		);
		assert_eq!(
			f.format("/workspace/repo", Some("/Users/ryan")),
			"/workspace/repo"
		);
		assert_eq!(f.format("/work/repo", None), "…/repo");
		assert_eq!(f.format("/work", None), "…");
		assert_eq!(
			f.format("/Users/ryan2/repo", Some("/Users/ryan")),
			"/Users/ryan2/repo"
		);
	}

	#[test]
	fn defaults_keep_home_abbreviation_and_empty_prefixes_do_not_match() {
		let f = PathFormat::default();
		assert_eq!(f.format("/Users/ryan/repo", Some("/Users/ryan")), "~/repo");
		assert_eq!(f.format("/Users/ryan", Some("/Users/ryan")), "~");
		assert_eq!(f.format("/tmp/repo", None), "/tmp/repo");
		assert_eq!(f.format("/", None), "/");
		let f = PathFormat {
			trim_prefixes: vec![String::new()],
			max_width: Some(0),
		};
		assert_eq!(f.format("/tmp/repo", None), "/tmp/repo");
	}

	#[test]
	fn width_keeps_the_longest_suffix_and_shortens_a_long_leaf_last() {
		let f = PathFormat {
			max_width: Some(22),
			..PathFormat::default()
		};
		assert_eq!(
			f.format("/Users/ryan/go/src/project/apps/mac", Some("/Users/ryan")),
			"~/…/project/apps/mac"
		);
		let f = PathFormat {
			max_width: Some(8),
			..PathFormat::default()
		};
		assert_eq!(
			f.format("/many/directories/extraordinary", None),
			"extr…ary"
		);
		assert_eq!(f.format("/many/directories/leaf", None), "/…/leaf");
	}

	#[test]
	fn prefix_trimming_and_width_limit_compose_without_repeated_ellipses() {
		let f = PathFormat {
			trim_prefixes: vec!["/work/long-prefix".into()],
			max_width: Some(15),
		};
		assert_eq!(
			f.format("/work/long-prefix/project/apps/mac", None),
			"…/apps/mac"
		);
	}

	#[test]
	fn unicode_names_and_tiny_limits_stay_within_display_columns() {
		for width in 1..20 {
			let f = PathFormat {
				max_width: Some(width),
				..PathFormat::default()
			};
			for path in ["/工作/项目/目录", "/tmp/café-project", "/a/🚀-launch-demo"] {
				let shown = f.format(path, None);
				assert!(shown.width() <= width, "{width}: {shown}");
			}
		}
	}

	#[test]
	fn shortening_keeps_accents_and_joined_emoji_together() {
		let mut f = PathFormat {
			max_width: Some(2),
			..PathFormat::default()
		};
		assert_eq!(f.format("/tmp/abcde\u{301}", None), "a…");
		f.max_width = Some(6);
		assert_eq!(f.format("/tmp/👩‍💻abcdef", None), "👩‍💻a…ef");
	}
}
