//! Represents the handlers for the "exec" command, or the command that runs
//! a single task, and exits. This is the command we expect to be run one of the
//! most times, perhaps the only thing that comes close is list.

use crate::{
	config::types::TopLevelConf,
	executors::ExecutorRepository,
	fetch::FetcherRepository,
	tasks::{
		execution::{
			execute_tasks_in_parallel,
			preparation::{
				build_ordered_execution_list, fetch_helpers, new_pipeline_id, WorkQueue,
			},
		},
		fs::ensure_dirs,
		TaskGraph,
	},
	terminal::Term,
};
use crossbeam_deque::Worker;
use std::path::PathBuf;
use tracing::error;

/// Handle the "exec" command provided by dev loop.
///
/// `config`: The top level config of dev-loop.
/// `fetcher`: The thing that is capable of fetching locations.
/// `terminal`: The thing that helps output to the terminal.
/// `root_dir`: The root directory of the project.
/// `args`: The extra arguments provided to the exec command.
#[allow(clippy::cognitive_complexity)]
pub async fn handle_exec_command(
	config: &TopLevelConf,
	fetcher: &FetcherRepository,
	terminal: &Term,
	args: &[String],
	root_dir: &PathBuf,
) -> i32 {
	// The order of exec:
	//
	//  1. Validate we have a task to run:
	//     * Task name should be args[0]
	//     * The task must exist.
	//     * Task should not be marked internal.
	//  2. Create all the directories we need for execution.
	//  3. Fetch all the executors definitions.
	//  4. Build the list of tasks to run, and in what order.
	//  5. Fetch all the helper scripts.
	//  6. Execute the task(s).

	// We need something to execute...
	if args.is_empty() {
		error!(
			"Please specify a task name to exec! If you're unsure of the task name use the: `list` command in order to see all the tasks that can be run."
		);
		return 10;
	}
	// We also need a valid TaskGraph...
	let task_repo_res = TaskGraph::new(config, fetcher).await;
	if let Err(task_err) = task_repo_res {
		error!("Failed to construct the task graph: [{:?}]", task_err);
		return 11;
	}
	let tasks = task_repo_res.unwrap().consume_and_get_tasks();

	// Now let's make sure we can actually run the task we need to.
	let user_specified_task = &args[0];
	if !tasks.contains_key(user_specified_task) {
		error!(
			"Unknown Task: [{}], please use the: `list` command in order to see all the tasks that can be run.",
			user_specified_task
		);
		return 12;
	}
	let selected_task = &tasks[user_specified_task];
	if selected_task.is_internal() {
		error!(
			"Task: [{}] is marked as internal! Please use the `list` command in order to see all the tasks that can be run.",
			user_specified_task
		);
		return 13;
	}

	// Before we start preparing a task for execution, let's ensure all the necessary dirs are
	// created.
	let ensure_res = ensure_dirs(config, root_dir);
	if ensure_res.is_err() {
		return 14;
	}

	// Let's fetch all the executors so we know how to assign them to tasks.
	let erepo_res = ExecutorRepository::new(config, fetcher, root_dir).await;
	if let Err(erepo_err) = erepo_res {
		error!(
			"Failed to enumerate all possible executors: [{:?}]",
			erepo_err,
		);
		return 15;
	}
	let mut erepo = erepo_res.unwrap();

	// Generate the task execution order.
	let pid = new_pipeline_id();
	let mut worker = Worker::new_fifo();
	let task_size;
	{
		let mut worker_as_queue = WorkQueue::Queue(&mut worker);
		let execution_list_res = build_ordered_execution_list(
			&tasks,
			selected_task,
			fetcher,
			&mut erepo,
			root_dir.clone(),
			&args[1..],
			pid,
			&mut worker_as_queue,
		)
		.await;
		if let Err(ele) = execution_list_res {
			error!("Failed to generate a list of tasks to execute: [{:?}]", ele);
			return 16;
		}
		task_size = execution_list_res.unwrap();
	}

	// Finally fetch all the helpers...
	let helpers_res = fetch_helpers(config, fetcher).await;
	if let Err(helper_fetch_err) = helpers_res {
		error!("Failed to fetch all the helpers: [{:?}]", helper_fetch_err);
		return 17;
	}
	let helpers = helpers_res.unwrap();

	let mut parallelism = num_cpus::get_physical();
	if let Ok(env_var) = std::env::var("DL_WORKER_COUNT") {
		if let Ok(worker_count) = env_var.parse::<usize>() {
			parallelism = worker_count;
		}
	}

	let rc_res = execute_tasks_in_parallel(helpers, worker, task_size, terminal, parallelism).await;

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
