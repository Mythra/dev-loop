//! Represents the handlers for the "run" command, or the command that runs
//! a series of tasks in parallel so that way you can parralelize a series
//! of tasks at once.

use crate::{
	config::types::TopLevelConf,
	executors::ExecutorRepository,
	fetch::FetcherRepository,
	tasks::{
		execution::{
			execute_tasks_in_parallel,
			preparation::{build_concurrent_execution_list, fetch_helpers},
		},
		fs::ensure_dirs,
		TaskGraph,
	},
	terminal::Term,
};
use crossbeam_deque::Worker;
use std::path::PathBuf;
use tracing::{error, info};

/// Handle the "run" command provided by dev loop.
#[allow(clippy::cognitive_complexity)]
pub async fn handle_run_command(
	config: &TopLevelConf,
	fetcher: &FetcherRepository,
	terminal: &Term,
	args: &[String],
	root_dir: &PathBuf,
) -> i32 {
	// The order of run:
	//
	// 1. Validate we have a series of tasks to run.
	// 2. Create all the directories we need.
	// 3. Fetch all the executor definitions.
	// 4. Build the list of tasks to run.
	// 5. Fetch all the helper scripts.
	// 6. Execute.

	// You need to tell us what to execute.
	if args.is_empty() {
		error!(
			"Please specify a preset name to run! If you're unsure of the preset name use the: `list` command in order to see all the presets that can be run."
		);
		return 10;
	}

	// Find the list of tags to match on...
	let presets_opt = config.get_presets();
	if presets_opt.is_none() {
		error!("No presets have been specified! Impossible to run any preset!");
		return 11;
	}
	let presets = presets_opt.unwrap();
	let mut tags = Vec::new();
	for preset in presets {
		if preset.get_name() == args[0] {
			info!("Found Preset: {}", args[0]);
			tags = Vec::from(preset.get_tags());
		}
	}

	// Now we also need a valid TaskGraph...
	let task_repo_res = TaskGraph::new(config, fetcher).await;
	if let Err(task_err) = task_repo_res {
		error!("Failed to construct the task graph: [{:?}]", task_err);
		return 12;
	}
	let tasks = task_repo_res.unwrap().consume_and_get_tasks();

	// Before we start preparing a task for execution, let's ensure all the necessary dirs are
	// created.
	let ensure_res = ensure_dirs(config, root_dir);
	if ensure_res.is_err() {
		return 13;
	}

	// Let's fetch all the executors so we know how to assign them to tasks.
	let erepo_res = ExecutorRepository::new(config, fetcher, root_dir).await;
	if let Err(erepo_err) = erepo_res {
		error!(
			"Failed to enumerate all possible executors: [{:?}]",
			erepo_err,
		);
		return 14;
	}
	let mut erepo = erepo_res.unwrap();

	// Let's build a list of tasks to execute.
	let mut worker = Worker::new_fifo();
	let execution_lanes_res = build_concurrent_execution_list(
		&tasks,
		&tags,
		fetcher,
		&mut erepo,
		root_dir.clone(),
		&mut worker,
	)
	.await;
	if let Err(execution_lane_err) = execution_lanes_res {
		error!(
			"Failed to generate a list of tasks to execute: [{:?}]",
			execution_lane_err,
		);
		return 15;
	}
	let task_size = execution_lanes_res.unwrap();

	// Let's fetch all the helpers.
	let helpers_res = fetch_helpers(config, fetcher).await;
	if let Err(helper_fetch_err) = helpers_res {
		error!("Failed to fetch all the helpers: [{:?}]", helper_fetch_err);
		return 16;
	}
	let helpers = helpers_res.unwrap();
	let rc_res = execute_tasks_in_parallel(
		helpers,
		worker,
		task_size,
		terminal,
		num_cpus::get_physical(),
	)
	.await;

	match rc_res {
		Ok(rc) => {
			crate::executors::docker::DockerExecutor::clean().await;
			rc
		}
		Err(err_code) => {
			error!("Failed to execute tasks in parallel: [{:?}]", err_code);
			10
		}
	}
}
