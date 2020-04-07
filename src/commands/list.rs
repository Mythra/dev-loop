//! Represents the "list" command which is responsible for the main "help"
//! like page, as well as listing the possible subcommands a top level task
//! has.
//!
//! The list command is the one that makes use of the `terminal` module, and
//! is meant for user facing consumption only. We don't really expect something
//! without a TTY to use the list command.

use crate::{
	config::types::{OneofOption, TaskConf, TopLevelConf},
	fetch::FetcherRepository,
	tasks::TaskGraph,
	terminal::Term,
};
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

/// Handle a raw list configuration.
///
/// `config`: The configuration object.
fn handle_raw_list(config: &TopLevelConf, terminal: &Term, tasks: &HashMap<String, TaskConf>) {
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
		"{}\n\n{}\n{}\n{}",
		terminal.render_title_bar("Dev-Loop", &format!("[{}]", VERSION.unwrap_or("unknown"))),
		terminal.render_list_section("COMMANDS", &items),
		terminal.render_list_section("TASKS", &tasks),
		terminal.render_list_section("PRESETS", &presets),
	);
}

/// Handle the actual `list command`.
///
/// `config` - the top level configuration object.
/// `fetcher` - the thing that goes and fetches for us.
/// `terminal` - the thing that helps output to the terminal.
/// `args` - the arguments for this list command.
#[allow(clippy::cognitive_complexity)]
pub async fn handle_list_command(
	config: &TopLevelConf,
	fetcher: &FetcherRepository,
	terminal: &Term,
	args: &[String],
) -> i32 {
	// The list command is the main command you get when running the binary.
	// It is not really ideal for CI environments, and we really only ever
	// expect humans to run it. This is why we specifically colour it, and
	// try to always output _something_. It doesn't matter if the repo is half
	// destroyed, we should still output what we can.

	// First construct the task graph. If one can't be constructed, that's fine.
	// Just show what we can, and log that you have a task graph error.
	let task_repo_res = TaskGraph::new(config, fetcher).await;
	let tasks = if let Ok(task_repo) = task_repo_res {
		task_repo.consume_and_get_tasks()
	} else if let Err(task_err) = task_repo_res {
		error!("Failed constructing Task DAG: [{:?}]", task_err);
		HashMap::new()
	} else {
		unreachable!();
	};

	let mut last_selected_task: Option<&TaskConf> = None;
	for arg in args {
		if let Some(prior_task) = last_selected_task {
			if prior_task.get_options().is_none() {
				error!(
					"Task has no options can't dig in: [{}] Listing...",
					prior_task.get_name()
				);
				break;
			}
			let potential_current_option = prior_task
				.get_options()
				.unwrap()
				.iter()
				.find(|item| item.get_name() == arg);
			if potential_current_option.is_none() {
				error!(
					"Can't find option: [{}] for oneof: [{}] Listing...",
					arg,
					prior_task.get_name(),
				);
				break;
			}
			let current_opt = potential_current_option.unwrap();
			if !tasks.contains_key(current_opt.get_task_name()) {
				error!(
					"Can't find sub-task with name: [{}], that option: [{}] points too. Listing...",
					current_opt.get_task_name(),
					current_opt.get_name(),
				);
				break;
			}
			let selected_task = &tasks[current_opt.get_task_name()];
			if selected_task.get_type() != "oneof" {
				error!(
					"Task: [{}] is not a oneof type! Listing...",
					selected_task.get_name()
				);
				break;
			}

			last_selected_task = Some(selected_task);
		} else {
			if !tasks.contains_key(arg) {
				error!("Unknown task: [{}] doing top level list", arg);
				handle_raw_list(config, terminal, &tasks);
				return 0;
			}

			let selected_task = &tasks[arg];
			if selected_task.get_type() != "oneof" {
				error!("Task: [{}] is not a oneof type! Doing top-level list", arg);
				handle_raw_list(config, terminal, &tasks);
				return 0;
			}

			last_selected_task = Some(selected_task);
		}
	}

	if last_selected_task.is_none() {
		handle_raw_list(config, terminal, &tasks);
		return 0;
	}

	let selected_task = last_selected_task.unwrap();
	let options = turn_oneof_into_listable(selected_task.get_options());

	// Show more info around the particular task they wanted to know.
	println!(
		"{}\n\n{}",
		terminal.render_title_bar("Dev-Loop", &format!("[{}]", VERSION.unwrap_or("unknown"))),
		terminal.render_list_section(
			&format!("Sub-Tasks for: [{}]", selected_task.get_name()),
			&options,
		)
	);

	0
}
