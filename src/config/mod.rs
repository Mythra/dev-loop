//! Types used to represent the actual configuration, and configuration parsing
//! for dev-loop. This is in essence a rust struct representation of the
//! config. It is not guaranteed to be valid, and not guaranteed to be workable.
//!
//! Those validations happen at different stages within the program.

use anyhow::{anyhow, Result};
use async_std::fs::{canonicalize, File};
use async_std::path::PathBuf;
use async_std::prelude::*;
use tracing::{error, trace};

pub mod types;

/// Get the root of the project repository.
///
/// This discovers the project directory automatically by looking at
/// `std::env::current_dir()`, and walking the path up.
#[allow(clippy::cognitive_complexity)]
pub async fn get_project_root() -> Option<PathBuf> {
	// Get the current directory (this is where we start looking...)
	//
	// We need the full "canonicalized" directory to ensure we can "pop"
	// all the way up.
	let current_dir = std::env::current_dir();
	if let Err(current_err) = current_dir {
		error!("Failed to get the current directory: [{:?}]", current_err);
		return None;
	}
	let current_dir = canonicalize(current_dir.unwrap()).await;
	if let Err(current_err) = current_dir {
		error!(
			"Failed to canonicalize the current directory: [{:?}]",
			current_err
		);
		return None;
	}
	let mut current_dir = current_dir.unwrap();

	// Go ahead, and look for the "dev-loop" directory that we should run
	// everything from. The "dev-loop" directory is one that has:
	//
	//   1. A: `.dl` folder.
	//   2. A: `config.yml` inside of that `.dl` folder.
	while current_dir.as_os_str() != "/" {
		trace!("Checking Path: [{:?}]", current_dir);
		let mut config_location = current_dir.clone();
		config_location.push(".dl/config.yml");

		if !config_location.is_file().await {
			trace!("Path does not have a .dl folder with a config.yml, continuing.");
			current_dir.pop();
			continue;
		}

		trace!("Path is viable!");
		return Some(current_dir);
	}

	None
}

/// Find and open a file handle the the project level configuration.
#[tracing::instrument]
async fn find_and_open_project_config() -> Option<File> {
	if let Some(mut project_root) = get_project_root().await {
		project_root.push(".dl/config.yml");
		trace!("Opening Config Path: [{:?}]", project_root);

		let file_res = File::open(project_root).await;
		if let Ok(handle) = file_res {
			Some(handle)
		} else {
			None
		}
	} else {
		None
	}
}

/// Attempt to fetch the top level project configuration for this project.
#[tracing::instrument]
pub async fn get_top_level_config() -> Result<types::TopLevelConf> {
	let config_fh = find_and_open_project_config().await;
	if config_fh.is_none() {
		return Err(anyhow!("Failed to find project configuration!"));
	}
	let mut config_fh = config_fh.unwrap();

	let mut contents = Vec::new();
	config_fh.read_to_end(&mut contents).await?;

	Ok(serde_yaml::from_slice::<types::TopLevelConf>(&contents)?)
}
