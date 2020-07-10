//! Represents the "list" command which is responsible for the main "help"
//! like page, as well as listing the possible subcommands a top level task
//! has.
//!
//! The list command is the one that makes use of the `terminal` module, and
//! is meant for user facing consumption only. We don't really expect something
//! without a TTY to use the list command.

use crate::{
	config::types::{OneofOption, TaskConf, TaskType, TopLevelConf},
	fetch::FetcherRepository,
	strsim::calculate_did_you_mean_possibilities,
	tasks::TaskGraph,
	terminal::TERM,
};
use color_eyre::Result;
use std::collections::HashMap;
use tracing::error;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

/// Fetch a series of preset lists so they can be be rendered with
/// `render_list_section`.
///
/// `conf`: The top level configuration which contains the presets.
fn get_presets_display(conf: &TopLevelConf) -> Vec<(String, String)> {
	let mut results = Vec::new();

	if let Some(presets) = conf.get_presets() {
		for preset in presets {
			let tags = preset.get_tags();
			let mut tag_str = String::new();
			for tag in tags {
				tag_str += "#";
				tag_str += tag;
				tag_str += " ";
			}
			if tag_str.is_empty() {
				tag_str += "no tags";
			}

			results.push((
				preset.get_name().to_owned(),
				format!(
					"{}: {}",
					preset
						.get_description()
						.unwrap_or("no description provided"),
					tag_str
				),
			));
		}
	}

	results
}

/// Get all the tasks in a "listable" state, ready to be shown
/// to a user.
///
/// By listable we simply mean a task that is not internal.
fn get_tasks_listable(tasks: &HashMap<String, TaskConf>) -> Vec<(String, String)> {
	let mut results = Vec::new();

	for (task_name, task_conf) in tasks {
		if !task_conf.is_internal() {
			results.push((
				task_name.to_owned(),
				task_conf
					.get_description()
					.unwrap_or("no description provided")
					.to_owned(),
			));
		}
	}

	results
}

/// Turn a potential set of options sinto something that can be listed.
fn turn_oneof_into_listable(options: Option<&Vec<OneofOption>>) -> Vec<(String, String)> {
	let mut results = Vec::new();

	if let Some(choices) = options {
		for choice in choices {
			results.push((
				choice.get_name().to_owned(),
				choice
					.get_description()
					.unwrap_or("no description provided")
					.to_owned(),
			));
		}
	}

	results
}

/// Check if an argument is a selectable top level argument aka:
///
///   1. Is a task.
///   2. Is a `Oneof` type.
///   3. Is not marked as internal.
fn is_selectable_top_level_arg<'a, 'b>(
	arg: &'b str,
	tasks: &'a HashMap<String, TaskConf>,
) -> Option<&'a TaskConf> {
	if !tasks.contains_key(arg) {
		error!(
			"Argument #1 ({}) is not a task that exists. Listing all possible tasks.",
			arg,
		);
		return None;
	}

	let selected_task = &tasks[arg];
	if selected_task.get_type() != &TaskType::Oneof {
		error!(
			"Argument #1 ({}) is not a task that can be listed. Listing all possible tasks.",
			arg,
		);
		return None;
	}

	if selected_task.is_internal() {
		error!(
			"Argument #1 ({}) is an internal task, and cannot be listed. Listing all the possible tasks.",
			arg,
		);
		return None;
	}

	Some(selected_task)
}

/// Check if a task has options that can be selected.
fn task_has_options(task: &TaskConf) -> bool {
	if let Some(options) = task.get_options() {
		!options.is_empty()
	} else {
		false
	}
}

/// Handle a raw list configuration.
///
/// `config`: The configuration object.
fn handle_raw_list(config: &TopLevelConf, tasks: &HashMap<String, TaskConf>) {
	let mut items: Vec<(String, String)> = Vec::new();
	items.push((
		"list".to_owned(),
		"for this page, and listing sub-tasks".to_owned(),
	));
	items.push(("exec".to_owned(), "to execute a single task".to_owned()));
	items.push((
		"run".to_owned(),
		"to run a preset, or a series of tasks based on their tags".to_owned(),
	));
	items.push((
		"clean".to_owned(),
		"to cleanup all dev-loop managed resources".to_owned(),
	));

	let presets = get_presets_display(config);
	let tasks = get_tasks_listable(tasks);

	println!(
		"{}\n\n{}{}{}",
		TERM.render_title_bar("Dev-Loop", &format!("[{}]", VERSION.unwrap_or("unknown"))),
		TERM.render_list_section("COMMANDS", &items),
		if tasks.is_empty() {
			String::new()
		} else {
			format!("\n{}", TERM.render_list_section("TASKS", &tasks))
		},
		if presets.is_empty() {
			String::new()
		} else {
			format!("\n{}", TERM.render_list_section("PRESETS", &presets))
		},
	);
}

fn handle_listing_arg<'a, 'b>(
	tasks: &'a HashMap<String, TaskConf>,
	args: &'b [String],
) -> Option<&'a TaskConf> {
	let mut last_selected_task: Option<&TaskConf> = None;

	for (arg_idx, arg) in args.iter().enumerate() {
		if let Some(prior_task) = last_selected_task {
			if !task_has_options(prior_task) {
				error!(
					"Argument #{} ({}) could not be found since the previous argument: #{} ({}) has no options. Performing top level list.",
					arg_idx + 1,
					arg,
					arg_idx,
					prior_task.get_name(),
				);

				return None;
			}

			let options = prior_task.get_options().unwrap();
			let potential_current_option = options.iter().find(|item| item.get_name() == arg);

			// Don't check for internal task here since a oneof could be built off
			// of internal options, and we want those to be selectable.
			if potential_current_option.is_none() {
				let did_you_mean_options = calculate_did_you_mean_possibilities(
					arg,
					&options
						.iter()
						.map(OneofOption::get_name)
						.collect::<Vec<&str>>(),
					3,
				);

				error!(
					"Argument #{} ({}) couldn't be found in the options provided by the previously selected task ({}).{}",
					arg_idx + 1,
					arg,
					prior_task.get_name(),
					if did_you_mean_options.is_empty() {
						" Listing all potential options.".to_owned()
					} else {
						let mut string = String::new();
						for option in did_you_mean_options {
							string += &format!("\nSuggestion: {}?", option);
						}
						string += "\nListing all potential options.";

						string
					}
				);
				break;
			}

			let current_opt = potential_current_option.unwrap();
			if !tasks.contains_key(current_opt.get_task_name()) {
				error!(
					"The option selected ({}) points to a task ({}) that does not exist. This should never happen, please report this issue.",
					arg,
					current_opt.get_task_name(),
				);
				break;
			}
			let selected_task = &tasks[current_opt.get_task_name()];
			if selected_task.get_type() != &TaskType::Oneof {
				error!(
					"You requested to list a specific option ({}) provided by ({}), but you can't list one specific option that isn't a oneof. Listing the options for {}.",
					current_opt.get_name(),
					prior_task.get_name(),
					prior_task.get_name(),
				);
				break;
			}

			last_selected_task = Some(selected_task);
		} else {
			let arg = is_selectable_top_level_arg(arg, tasks)?;
			last_selected_task = Some(arg);
		}
	}

	last_selected_task
}

/// Handle the actual `list command`.
///
/// `config` - the top level configuration object.
/// `fetcher` - the thing that goes and fetches for us.
/// `args` - the arguments for this list command.
///
/// # Errors
///
/// - When constructing the task graph.
pub async fn handle_list_command(
	config: &TopLevelConf,
	fetcher: &FetcherRepository,
	args: &[String],
) -> Result<()> {
	let span = tracing::info_span!("list");
	let _guard = span.enter();

	// The list command is the main command you get when running the binary.
	// It is not really ideal for CI environments, and we really only ever
	// expect humans to run it. This is why we specifically colour it, and
	// try to always output _something_.
	let tasks = TaskGraph::new(config, fetcher)
		.await?
		.consume_and_get_tasks();
	let last_selected_task = handle_listing_arg(&tasks, args);

	if last_selected_task.is_none() {
		handle_raw_list(config, &tasks);
		return Ok(());
	}

	let selected_task = last_selected_task.unwrap();
	let options = turn_oneof_into_listable(selected_task.get_options());

	// Show more info around the particular task they wanted to know.
	println!(
		"{}\n\n{}",
		TERM.render_title_bar("Dev-Loop", &format!("[{}]", VERSION.unwrap_or("unknown"))),
		TERM.render_list_section(
			&format!("Sub-Tasks for: [{}]", selected_task.get_name()),
			&options,
		)
	);

	Ok(())
}
