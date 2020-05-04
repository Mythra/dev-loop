#![allow(
	clippy::module_name_repetitions,
	clippy::result_map_unwrap_or_else,
	clippy::wildcard_imports
)]

use lazy_static::*;
use std::{
	path::PathBuf,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};
use tracing::error;

lazy_static! {
	pub static ref RUNNING: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
}

pub mod commands;
pub mod config;
pub mod dirs;
pub mod executors;
pub mod fetch;
pub mod log;
pub mod tasks;
pub mod terminal;

/// Determines if Ctrl-C has been hit.
#[must_use]
pub fn has_ctrlc_been_hit() -> bool {
	!RUNNING.clone().load(Ordering::SeqCst)
}

/// Get the temporary directory for this host.
#[must_use]
pub fn get_tmp_dir() -> PathBuf {
	// Mac OS X actually uses "TMPDIR" for a user specified temporary directory
	// as opposed to `/tmp`. There are subtle differences between the two, and
	// without getting into details the key thing is we should use it if it
	// is set.
	//
	// We've seen numerous problems trying to use `/tmp` on OSX.
	if let Ok(tmpdir_env) = std::env::var("TMPDIR") {
		let pbte = PathBuf::from(tmpdir_env);
		if pbte.is_dir() {
			pbte
		} else {
			PathBuf::from("/tmp")
		}
	} else {
		PathBuf::from("/tmp")
	}
}

/// The entrypoint to the application.
///
/// Gets called at the beginning, and performs setup.
#[tracing::instrument]
fn main() {
	if let Err(log_err) = log::initialize_crate_logging() {
		panic!("Failed to initialize logger due to: [{:?}]", log_err);
	}
	let r = RUNNING.clone();
	if let Err(ctrlc_err) = ctrlc::set_handler(move || {
		r.store(false, Ordering::SeqCst);
	}) {
		panic!(
			"Failed to initialize CTRLC handler due to: [{:?}]",
			ctrlc_err
		);
	}

	let mut program_name = String::new();
	let mut action = String::new();
	let mut arguments: Vec<String> = Vec::new();

	for arg in std::env::args() {
		if program_name.is_empty() {
			program_name = arg.to_owned();
		} else if action.is_empty() {
			action = arg.to_owned();
		} else {
			arguments.push(arg.to_owned());
		}
	}

	if action.is_empty() {
		// List is the "help" page or the default command.
		action = "list".to_owned();
	}

	// Use if let over unwrap_or since unwrap_or executes optimistically.
	let tlc_res = config::get_top_level_config();
	let tlc = if let Err(tlc_err) = tlc_res {
		error!("Valid YAML Configuration not found, you will need to create one before using dev-loop. Error: [{:?}]", tlc_err);
		config::types::TopLevelConf::create_empty_config()
	} else {
		tlc_res.unwrap()
	};

	let root_dir_opt = config::get_project_root();
	let root_dir = if let Some(dir) = root_dir_opt {
		dir
	} else if let Ok(dir) = std::env::current_dir() {
		dir
	} else {
		panic!("No project root, and couldn't fetch current directory!");
	};

	let fetcher_res = fetch::FetcherRepository::new(root_dir.clone());
	if let Err(fetch_err) = fetcher_res {
		panic!("Unknown fetcher error: [{:?}]", fetch_err);
	}
	let fetcher = fetcher_res.unwrap();
	let term = terminal::Term::new();

	let exit_code = match action.as_str() {
		"list" => async_std::task::block_on(async {
			commands::list::handle_list_command(&tlc, &fetcher, &term, &arguments).await
		}),
		"exec" => async_std::task::block_on(async {
			commands::exec::handle_exec_command(&tlc, &fetcher, &term, &arguments, &root_dir).await
		}),
		"run" => async_std::task::block_on(async {
			commands::run::handle_run_command(&tlc, &fetcher, &term, &arguments, &root_dir).await
		}),
		"clean" => {
			async_std::task::block_on(async { commands::clean::handle_clean_command().await })
		}
		&_ => {
			error!("The sub-command: [{}] is not known to dev-loop.\n\n\
							note: Valid commands are: `list` for listing tasks/presets, `exec` to execute a task, `run` to run a preset, and `clean` to cleanup.", action);
			10
		}
	};

	std::process::exit(exit_code)
}
