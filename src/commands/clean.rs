//! Implements the `clean` command, or the command that clears all resources
//! created by a dev-loop executor. This should only need to be used during
//! circumstances where we like powered off while running. However, it should
//! always be safe to run.

use crate::executors::docker::DockerExecutor;
use crate::executors::host::HostExecutor;

pub async fn handle_clean_command() -> i32 {
	print!("Cleaning all Resources ...");
	HostExecutor::clean().await;
	DockerExecutor::clean().await;
	println!(" Done.");

	0
}
