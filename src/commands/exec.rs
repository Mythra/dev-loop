//! Represents the handlers for the "exec" command, or the command that runs
//! a single task, and exits. This is the command we expect to be run one of the
//! most times, perhaps the only thing that comes close is list.

use crate::{
	config::types::{TaskConf, TaskType, TopLevelConf},
	executors::ExecutorRepository,
	fetch::FetcherRepository,
	strsim::add_did_you_mean_text,
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
};

use color_eyre::{eyre::eyre, section::help::Help, Report, Result};
use crossbeam_deque::Worker;
use std::{collections::HashMap, path::PathBuf};

/// Attempt to find simple replacement for an internal task.
fn report_potential_internal_task_names<T>(
	mut result: Result<T, Report>,
	tasks: &HashMap<String, TaskConf>,
	internal_task: &str,
) -> Result<T, Report> {
	let mut simple_tasks = Vec::new();

	for (_, task_conf) in tasks {
		if task_conf.is_internal() {
			continue;
		}

		match *task_conf.get_type() {
			TaskType::Oneof => {
				if let Some(options) = task_conf.get_options() {
					for option in options {
						if option.get_task_name() == internal_task {
							simple_tasks.push(task_conf);
						}
					}
				}
			}
			TaskType::Pipeline | TaskType::ParallelPipeline => {
				if let Some(steps) = task_conf.get_steps() {
					for step in steps {
						if step.get_task_name() == internal_task {
							simple_tasks.push(task_conf);
						}
					}
				}
			}
			_ => {}
		}

		// We don't want to print so many tasks it overwhelms the user.
		if simple_tasks.len() > 2 {
			simple_tasks.clear();
			break;
		}
	}

	if simple_tasks.is_empty() {
		result = result.note("You can use the list subcommand to describe the tasks you can run, and find the one that you want.");
	} else {
		for simple_task in simple_tasks {
			match *simple_task.get_type() {
				TaskType::Oneof => {
					// This unwrap is guaranteed to be safe .
					for option in simple_task.get_options().unwrap() {
						if option.get_task_name() == internal_task {
							result = result.suggestion(format!(
								"You can run the subcommand: `exec {} {}`",
								simple_task.get_name(),
								option.get_name()
							));
						}
					}
				}
				TaskType::Pipeline | TaskType::ParallelPipeline => {
					result = result.suggestion(format!(
						"You can run the subcommand: `exec {}`",
						simple_task.get_name()
					));
				}
				_ => {}
			}
		}
	}

	result
}

/// Handle the "exec" command provided by dev loop.
///
/// # Errors
///
/// - Can Error when no argument was provided.
/// - Error constructing the `TaskGraph`.
/// - Error finding the task the user wants to run/running an internal task.
/// - Error creating directories that need to be ensured.
/// - Error creating an executor/choosing an executor for tasks.
/// - Error writing the helper scripts.
/// - Error running the task.
#[allow(clippy::cognitive_complexity)]
pub async fn handle_exec_command(
	config: &TopLevelConf,
	fetcher: &FetcherRepository,
	args: &[String],
	root_dir: &PathBuf,
) -> Result<()> {
	let span = tracing::info_span!("exec");
	let _guard = span.enter();

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
		return Err(eyre!("Please specify a task name to execute!",))
			.suggestion("You can use the list subcommand to get a list of tasks you can execute.");
	}
	// We also need a valid TaskGraph...
	let tasks = TaskGraph::new(config, fetcher)
		.await?
		.consume_and_get_tasks();

	// Now let's make sure we can actually run the task we need to.
	let user_specified_task = &args[0];
	if !tasks.contains_key(user_specified_task) {
		return add_did_you_mean_text(
			Err(eyre!("There is no task named: [{}]", user_specified_task,)),
			user_specified_task,
			&tasks.keys().map(String::as_str).collect::<Vec<&str>>(),
			3,
			Some("You can use the list subcommand to get a list of tasks you can execute"),
		);
	}
	let selected_task = &tasks[user_specified_task];
	if selected_task.is_internal() {
		return report_potential_internal_task_names(
			Err(eyre!(
				"Task: [{}] is marked as internal! Please use the `list` command in order to see all the tasks that can be run.",
				user_specified_task,
			)),
			&tasks,
			user_specified_task,
		);
	}

	// Before we start preparing a task for execution, let's ensure all the necessary dirs are
	// created.
	ensure_dirs(config, root_dir)?;

	// Let's fetch all the executors so we know how to assign them to tasks.
	let mut erepo = ExecutorRepository::new(config, fetcher, root_dir).await?;

	// Generate the task execution order.
	let pid = new_pipeline_id();
	let mut worker = Worker::new_fifo();
	let task_size;
	{
		let mut worker_as_queue = WorkQueue::Queue(&mut worker);
		task_size = build_ordered_execution_list(
			&tasks,
			selected_task,
			fetcher,
			&mut erepo,
			root_dir.clone(),
			&args[1..],
			pid,
			&mut worker_as_queue,
		)
		.await?;
	}

	// Finally fetch all the helpers...
	let helpers = fetch_helpers(config, fetcher).await?;

	let mut parallelism = num_cpus::get_physical();
	if let Ok(env_var) = std::env::var("DL_WORKER_COUNT") {
		if let Ok(worker_count) = env_var.parse::<usize>() {
			parallelism = worker_count;
		}
	}

	let res = execute_tasks_in_parallel(helpers, worker, task_size, parallelism).await;
	// Don't clean if we encouter an error, aid in debugging.
	match res {
		Ok(exit_code) => {
			if exit_code == 0 {
				// Don't cause an error for cleaning if the task succeeded, the user can always clean manually.
				let _ = crate::executors::docker::DockerExecutor::clean().await;
				Ok(())
			} else {
				Err(eyre!(
					"One of the tasks being run failed. You can use the logs above from your tasks to debug.",
				)).note(format!("Failing exit code: {}", exit_code))
			}
		}
		Err(err_code) => Err(err_code),
	}
}
