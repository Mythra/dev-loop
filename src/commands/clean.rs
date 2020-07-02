//! Implements the `clean` command, or the command that clears all resources
//! created by a dev-loop executor. This should only need to be used during
//! circumstances where we like powered off while running. However, it should
//! always be safe to run.

use crate::executors::{docker::DockerExecutor, host::HostExecutor};

use color_eyre::Result;
use tracing::info;

/// Execute the clean command.
///
/// # Errors
///
/// - When one of the underlying executor cleanups fail.
pub async fn handle_clean_command() -> Result<()> {
	let span = tracing::info_span!("clean");
	let _guard = span.enter();

	info!("Cleaning resources ...");

	HostExecutor::clean().await;
	DockerExecutor::clean().await?;

	info!("Cleaned.");
	Ok(())
}
