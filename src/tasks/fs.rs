//! Any filesystem related code for tasks...

use crate::config::types::TopLevelConf;
use color_eyre::{eyre::WrapErr, section::help::Help, Result};
use std::{fs::create_dir_all, path::PathBuf};

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

			create_dir_all(&path)
				.wrap_err("Cannot ensure directory specified in `.dl/config.yml` in `ensure_directories`.")
				.note(format!("Tried to create directory: [{:?}]", path))?;
		}
	}

	Ok(())
}
