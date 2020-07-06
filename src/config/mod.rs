//! Types used to represent the actual configuration, and configuration parsing
//! for dev-loop. This is in essence a rust struct representation of the
//! config. It is not guaranteed to be valid, and not guaranteed to be workable.
//!
//! Those validations happen at different stages within the program.

use crate::yaml_err::contextualize_yaml_err;
use color_eyre::{eyre::WrapErr, Result, Section};
use std::{
	fs::{canonicalize, File},
	io::{Error as IoError, Read},
	path::PathBuf,
};
use tracing::{error, trace};

pub mod types;

/// Get the root of the project repository.
///
/// This discovers the project directory automatically by looking at
/// `std::env::current_dir()`, and walking the path up.
#[allow(clippy::cognitive_complexity)]
#[must_use]
pub fn get_project_root() -> Option<PathBuf> {
	// Get the current directory (this is where we start looking...)
	//
	// We need the full "canonicalized" directory to ensure we can "pop"
	// all the way up.
	let current_dir_res = std::env::current_dir().and_then(canonicalize);
	if let Err(finding_dir) = current_dir_res {
		error!(
			"{:?}",
			Err::<(), IoError>(finding_dir).wrap_err("Failed to find the current directory.").suggestion("Please file an issue for support, if the underlying cause is not immediately clear.").unwrap_err(),
		);
		return None;
	}
	let mut current_dir = current_dir_res.unwrap();

	// Go ahead, and look for the "dev-loop" directory that we should run
	// everything from. The "dev-loop" directory is one that has:
	//
	//   1. A: `.dl` folder.
	//   2. A: `config.yml` inside of that `.dl` folder.
	while current_dir.as_os_str() != "/" {
		trace!("Checking Path: [{:?}]", current_dir);
		let mut config_location = current_dir.clone();
		config_location.push(".dl/config.yml");

		if !config_location.is_file() {
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
fn find_and_open_project_config() -> Option<(File, PathBuf)> {
	get_project_root().and_then(|mut project_root| {
		project_root.push(".dl/config.yml");
		trace!("Opening Config Path: [{:?}]", project_root);

		let file = File::open(project_root.clone());
		if let Ok(fh) = file {
			Some((fh, project_root))
		} else {
			None
		}
	})
}

/// Attempt to fetch the top level project configuration for this project.
///
/// # Errors
///
/// - When there is error doing a file read on a found configuration file.
pub fn get_top_level_config() -> Result<Option<types::TopLevelConf>> {
	let config_fh_opt = find_and_open_project_config();
	if config_fh_opt.is_none() {
		error!("Could not find project configuration [.dl/config.yml] looking in current directory, and parent directories.");
		return Ok(None);
	}
	let (mut config_fh, config_path) = config_fh_opt.unwrap();
	let config_path_as_str = config_path.to_str().unwrap_or_default();

	let mut contents = String::new();
	config_fh.read_to_string(&mut contents)?;

	Ok(Some(
		contextualize_yaml_err(
			serde_yaml::from_str::<types::TopLevelConf>(&contents),
			".dl/config.yml",
			&contents,
		)
		.note(format!(
			"Full path to project configuration is: {}",
			config_path_as_str
		))?,
	))
}
