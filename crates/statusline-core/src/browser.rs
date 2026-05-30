use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "clap", clap(rename_all = "lowercase"))]
pub enum Browser {
	Chrome,
	Brave,
	Firefox,
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
