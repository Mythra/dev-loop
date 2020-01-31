use std::time::{Duration, Instant};

/// A "throttle" struct to ensure we don't output _too_ fast to the terminal
/// for this task indicator. This is really for two reasons:
///
/// 1. We don't want to output something that would "flash" or come in,
///    and leave right away.
/// 2. We don't want to force the terminal to redraw a whole bunch, eating
///    up valuable CPU time.
///
/// This module is actually a recreation of:
///   - <https://github.com/rust-lang/cargo/blob/74383b4fdb4f1d29152f64a552c93b7a241f265b/src/cargo/util/progress.rs#L19>
///
/// Which does the exact same thing.
pub struct Throttle {
	/// Is this our first time rendering? If so let's wait at least 500ms.
	first: bool,
	/// The last time we drew to the screen.
	last_update: Instant,
}

/// The default implementation for `Throttle`.
impl Default for Throttle {
	#[must_use]
	fn default() -> Self {
		Self::new()
	}
}

impl Throttle {
	/// Create a new instance of a throttler.
	#[must_use]
	pub fn new() -> Self {
		Self {
			first: true,
			last_update: Instant::now(),
		}
	}

	/// Should you be allowed to print?
	pub fn allowed(&mut self) -> bool {
		if self.first {
			let delay = Duration::from_millis(500);
			if self.last_update.elapsed() < delay {
				return false;
			}
		} else {
			let interval = Duration::from_millis(100);
			if self.last_update.elapsed() < interval {
				return false;
			}
		}
		self.update();
		true
	}

	/// Update the last time we printed.
	fn update(&mut self) {
		self.first = false;
		self.last_update = Instant::now();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Simply test the most basic "allowed" cases.
	/// This doesn't test any negation cases.
	#[test]
	fn test_allowed() {
		let mut throttler = Throttle::new();
		std::thread::sleep(std::time::Duration::from_millis(500));
		assert!(throttler.allowed());
		std::thread::sleep(std::time::Duration::from_millis(100));
		assert!(throttler.allowed());
	}

	/// Test that the first render is still blocked after 100ms.
	///
	/// And then after first render you can render faster.
	#[test]
	fn test_longer_block_on_start() {
		let mut throttler = Throttle::new();
		std::thread::sleep(std::time::Duration::from_millis(300));
		// It's been ~300ms total, first render is blocked at 500ms
		assert!(!throttler.allowed());

		std::thread::sleep(std::time::Duration::from_millis(300));
		// It's been ~600ms total, first render can be allowed it's been past 500ms
		assert!(throttler.allowed());

		std::thread::sleep(std::time::Duration::from_millis(300));
		// It's been ~300ms since the last render, but since it's not the first
		// render it should be allowed.
		assert!(throttler.allowed());
	}

	#[test]
	fn can_block_non_first_render() {
		let mut throttler = Throttle::new();
		std::thread::sleep(std::time::Duration::from_millis(600));
		assert!(throttler.allowed());
		assert!(!throttler.allowed());
		assert!(!throttler.allowed());
		std::thread::sleep(std::time::Duration::from_millis(50));
		assert!(!throttler.allowed());
		std::thread::sleep(std::time::Duration::from_millis(50));
		assert!(throttler.allowed());
	}
}
