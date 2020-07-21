use crate::config::types::TopLevelConf;

use color_eyre::{eyre::eyre, Report, Section};
use tracing::warn;

pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod dirs;
pub(crate) mod executors;
pub(crate) mod fetch;
pub(crate) mod future_helper;
pub(crate) mod log;
pub(crate) mod sigint;
pub(crate) mod strsim;
pub(crate) mod tasks;
pub(crate) mod terminal;
pub(crate) mod yaml_err;

/// The entrypoint to the application.
///
/// Gets called at the beginning, and performs setup.
fn main() -> Result<(), Report> {
	log::initialize_crate_logging()?;
	sigint::setup_global_ctrlc_handler()?;

	let span = tracing::info_span!("dev-loop");
	let _span_guard = span.enter();

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

	let tlc_res = config::get_top_level();
	let errord_on_tlc = tlc_res.is_err();
	let tlc = if let Err(tlc_err) = tlc_res {
		// NOTE(cynthia): if you change this print statement, make sure it looks
		// correct on below commands!
		//
		// Because list/clean still wants to be run even with no configuration
		// we print it out here, and exec/run assume it's printed here.
		let formatted_err = tlc_err.wrap_err(
			"Invalid YAML Configuration, you will need a valid one if you want to run dev-loop",
		);
		warn!("{:?}\n", formatted_err,);
		config::types::TopLevelConf::create_empty_config()
	} else {
		tlc_res
			.unwrap()
			.unwrap_or_else(TopLevelConf::create_empty_config)
	};

	let root_dir_opt = config::get_project_root();
	let root_dir = if let Some(dir) = root_dir_opt {
		dir
	} else if let Ok(dir) = std::env::current_dir() {
		dir
	} else {
		return Err(eyre!(
			"Failed to find [.dl/config.yml] in current directory, or parent directories. The current working directory could also not be determined."
		)).suggestion("This is an internal error, please file an issue on the dev-loop repo.");
	};

	let fetcher_res = fetch::FetcherRepository::new(root_dir.clone());
	if let Err(fetch_err) = fetcher_res {
		panic!("Unknown fetcher error: [{:?}]", fetch_err);
	}
	let fetcher = fetcher_res.unwrap();

	Ok(match action.as_str() {
		"list" => async_std::task::block_on(async {
			commands::list::handle_list_command(&tlc, &fetcher, &arguments).await
		}),
		"exec" => {
			if errord_on_tlc {
				std::process::exit(10);
			}

			async_std::task::block_on(async {
				commands::exec::handle_exec_command(&tlc, &fetcher, &arguments, &root_dir).await
			})
		}
		"run" => {
			if errord_on_tlc {
				std::process::exit(10);
			}

			async_std::task::block_on(async {
				commands::run::handle_run_command(&tlc, &fetcher, &arguments, &root_dir).await
			})
		}
		"clean" => {
			async_std::task::block_on(async { commands::clean::handle_clean_command().await })
		}
		&_ => {
			let err = Err(eyre!(
				"The sub-command: [{}] is not known to dev-loop.",
				action,
			));

			strsim::add_did_you_mean_text(
				err,
				&action,
				&["clean", "list", "exec", "run"],
				2,
				Some("You can use the `list` sub-command to get a list of commands to run."),
			)
		}
	}?)
}
