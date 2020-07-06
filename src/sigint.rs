use color_eyre::{eyre::WrapErr, Result, Section};
use lazy_static::*;

lazy_static! {
	pub static ref RUNNING: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
}

use std::sync::{
	atomic::{AtomicBool, Ordering},
	Arc,
};

/// Determines if Ctrl-C has been hit.
#[must_use]
pub fn has_ctrlc_been_hit() -> bool {
	!RUNNING.clone().load(Ordering::Acquire)
}

/// Setup the CTRL-C Handler.
///
/// Watches for Ctrl-C, and properly handles shutdown for an application so
/// we don't leave junk everywhere.
///
/// # Errors
///
/// - Bubbled up error from `ctrlc` crate.
pub fn setup_global_ctrlc_handler() -> Result<()> {
	let r = RUNNING.clone();
	ctrlc::set_handler(move || {
		r.store(false, Ordering::Release);
	})
	.wrap_err("Failed to setup Ctrl-C handler.")
	.note("If the error isn't immediately clear, there's probably something really wrong going on, it'd be best to file an issue.")?;

	Ok(())
}
