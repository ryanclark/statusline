use std::cmp::Ordering;
use owo_colors::{OwoColorize, Rgb};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Formatter;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub(crate) struct Cents(f64);

impl Cents {
	pub(crate) fn as_percentage_of(self, limit: Cents) -> Percentage {
		if limit.0 > 0.0 {
			Percentage((self.0 / limit.0) * 100.0)
		} else {
			Percentage(0.0)
		}
	}
}

impl From<f64> for Cents {
	fn from(value: f64) -> Self {
		Self(value)
	}
}

impl fmt::Display for Cents {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "${:.0}", (self.0 / 100.0).round())
	}
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct Percentage(f64);

impl Percentage {

	pub(crate) fn color(self) -> Rgb {
		let p = self.0.clamp(0.0, 100.0);

		if p < 50.0 {
			Rgb(
				lerp_channel(80, 240, p, 50.0),
				200,
				lerp_channel(120, 80, p, 50.0),
			)
		} else if p < 80.0 {
			let t = p - 50.0;
			Rgb(
				lerp_channel(240, 255, t, 30.0),
				lerp_channel(200, 140, t, 30.0),
				lerp_channel(80, 50, t, 30.0),
			)
		} else {
			let t = p - 80.0;
			Rgb(
				255,
				lerp_channel(140, 70, t, 20.0),
				lerp_channel(50, 70, t, 20.0),
			)
		}
	}
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn lerp_channel(from: u8, to: u8, t: f64, range: f64) -> u8 {
	// Safe: v is always in [min(from,to)..=max(from,to)] ⊆ [0, 255]
	let v = f64::from(from) + (f64::from(to) - f64::from(from)) * t / range;
	v.round() as u8
}

impl From<f64> for Percentage {
	fn from(value: f64) -> Self {
		Self(value)
	}
}

impl std::str::FromStr for Percentage {
	type Err = std::num::ParseFloatError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let s = s.strip_suffix('%').unwrap_or(s);
		s.parse::<f64>().map(Self)
	}
}

impl PartialOrd for Percentage {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl fmt::Display for Percentage {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}%", self.0)
	}
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ColoredPercentage(pub(crate) Percentage);

impl fmt::Display for ColoredPercentage {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}", format_args!("{}", self.0).color(self.0.color()).bold())
	}
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Tokens(u64);

impl From<u64> for Tokens {
	fn from(value: u64) -> Self {
		Self(value)
	}
}

impl std::ops::Add for Tokens {
	type Output = Tokens;

	fn add(self, rhs: Self) -> Tokens {
		Tokens(self.0 + rhs.0)
	}
}

impl fmt::Display for Tokens {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		if self.0 >= 1_000_000 {
			write!(f, "{:.1}M", self.0 as f64 / 1_000_000.0)
		} else if self.0 >= 1000 {
			write!(f, "{:.1}k", self.0 as f64 / 1000.0)
		} else {
			write!(f, "{}", self.0)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cents_rounds_to_whole_dollars() {
		assert_eq!(Cents::from(1050.0).to_string(), "$11");
		assert_eq!(Cents::from(999.0).to_string(), "$10");
		assert_eq!(Cents::from(0.0).to_string(), "$0");
		assert_eq!(Cents::from(100.0).to_string(), "$1");
		assert_eq!(Cents::from(149.0).to_string(), "$1");
		assert_eq!(Cents::from(150.0).to_string(), "$2");
	}

	#[test]
	fn tokens_below_thousand() {
		assert_eq!(Tokens::from(0).to_string(), "0");
		assert_eq!(Tokens::from(1).to_string(), "1");
		assert_eq!(Tokens::from(999).to_string(), "999");
	}

	#[test]
	fn tokens_thousands() {
		assert_eq!(Tokens::from(1000).to_string(), "1.0k");
		assert_eq!(Tokens::from(1500).to_string(), "1.5k");
		assert_eq!(Tokens::from(999_999).to_string(), "1000.0k");
	}

	#[test]
	fn tokens_millions() {
		assert_eq!(Tokens::from(1_000_000).to_string(), "1.0M");
		assert_eq!(Tokens::from(2_500_000).to_string(), "2.5M");
		assert_eq!(Tokens::from(10_000_000).to_string(), "10.0M");
	}

	#[test]
	fn percentage_to_color_low() {
		let c = Percentage::from(0.0).color();
		assert_eq!(c, Rgb(80, 200, 120));
	}

	#[test]
	fn percentage_to_color_mid() {
		let c = Percentage::from(50.0).color();
		assert_eq!(c, Rgb(240, 200, 80));
	}

	#[test]
	fn percentage_to_color_high() {
		let c = Percentage::from(80.0).color();
		assert_eq!(c, Rgb(255, 140, 50));
	}

	#[test]
	fn percentage_to_color_max() {
		let c = Percentage::from(100.0).color();
		assert_eq!(c, Rgb(255, 70, 70));
	}

	#[test]
	fn percentage_to_color_clamps_negative() {
		assert_eq!(
			Percentage::from(-10.0).color(),
			Percentage::from(0.0).color()
		);
	}

	#[test]
	fn percentage_to_color_clamps_over_100() {
		assert_eq!(
			Percentage::from(150.0).color(),
			Percentage::from(100.0).color()
		);
	}

	#[test]
	fn percentage_display_whole_number() {
		assert_eq!(Percentage::from(50.0).to_string(), "50%");
	}

	#[test]
	fn percentage_display_fractional() {
		assert_eq!(Percentage::from(99.5).to_string(), "99.5%");
	}

	#[test]
	fn percentage_display_zero() {
		assert_eq!(Percentage::from(0.0).to_string(), "0%");
	}

	#[test]
	fn percentage_from_str_bare_number() {
		assert_eq!("42.5".parse::<Percentage>().unwrap(), Percentage::from(42.5));
	}

	#[test]
	fn percentage_from_str_with_percent_suffix() {
		assert_eq!("42.5%".parse::<Percentage>().unwrap(), Percentage::from(42.5));
	}

	#[test]
	fn percentage_from_str_rejects_non_numeric() {
		assert!("abc".parse::<Percentage>().is_err());
	}

	#[test]
	fn percentage_from_str_rejects_empty() {
		assert!("".parse::<Percentage>().is_err());
	}

	#[test]
	fn percentage_fromstr_display_roundtrip() {
		let original = Percentage::from(42.5);
		let displayed = original.to_string();
		let parsed: Percentage = displayed.parse().unwrap();
		assert_eq!(parsed, original);
	}

	#[test]
	fn percentage_ordering() {
		use std::cmp::Ordering;
		let low = Percentage::from(25.0);
		let high = Percentage::from(75.0);
		assert!(low < high);
		assert!(high > low);
		assert_eq!(low.partial_cmp(&Percentage::from(25.0)), Some(Ordering::Equal));
	}

	#[test]
	fn tokens_addition() {
		assert_eq!(Tokens::from(100) + Tokens::from(200), Tokens::from(300));
	}

	#[test]
	fn tokens_addition_with_zero() {
		assert_eq!(Tokens::from(0) + Tokens::from(0), Tokens::from(0));
		assert_eq!(Tokens::from(42) + Tokens::from(0), Tokens::from(42));
	}

	#[test]
	fn tokens_ordering() {
		assert!(Tokens::from(100) < Tokens::from(200));
		assert!(Tokens::from(200) > Tokens::from(100));
		assert_eq!(Tokens::from(50), Tokens::from(50));
	}

	#[test]
	fn cents_as_percentage_of_normal() {
		assert_eq!(
			Cents::from(2500.0).as_percentage_of(Cents::from(10000.0)),
			Percentage::from(25.0),
		);
	}

	#[test]
	fn cents_as_percentage_of_full() {
		assert_eq!(
			Cents::from(10000.0).as_percentage_of(Cents::from(10000.0)),
			Percentage::from(100.0),
		);
	}
}
