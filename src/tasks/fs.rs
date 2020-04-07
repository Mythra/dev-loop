//! Any filesystem related code for tasks...

use crate::config::types::TopLevelConf;
use anyhow::{anyhow, Result};
use std::{fs::create_dir_all, path::PathBuf};
use tracing::error;

/// Ensure all the directories exist for a task that need to exist.
///
/// `config`: The top level configuration.
/// `root_dir`: The root directory.
///
/// # Errors
///
/// If a directory fails to get created for any reason.
pub fn ensure_dirs(config: &TopLevelConf, root_dir: &PathBuf) -> Result<()> {
	if let Some(edirs) = config.get_dirs_to_ensure() {
		for ensure_dir in edirs {
			let mut path = root_dir.clone();
			path.push(ensure_dir);
			if let Err(err) = create_dir_all(&path) {
				error!(
					"Failed to create directory: [{:?}] reason: [{:?}]",
					path, err
				);
				return Err(anyhow!("{:?}", err));
			}
		}
	}

	Ok(())
}
