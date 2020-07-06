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
};
use color_eyre::{eyre::eyre, Result, Section};
use crossbeam_deque::Worker;
use std::path::PathBuf;

/// Handle the "run" command provided by dev loop.
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
pub async fn handle_run_command(
	config: &TopLevelConf,
	fetcher: &FetcherRepository,
	args: &[String],
	root_dir: &PathBuf,
) -> Result<()> {
	let span = tracing::info_span!("run");
	let _guard = span.enter();

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
		return Err(eyre!(
			"Please specify a preset name to run! If you're unsure of the preset name use the: `list` command in order to see all the presets that can be run."
		));
	}

	// Find the list of tags to match on...
	let presets_opt = config.get_presets();
	if presets_opt.is_none() {
		return Err(eyre!(
			"You have configured no presets, so we cannot run a preset."
		)).note("You can define presets in `.dl/config.yml`, the format is specified here: https://dev-loop.kungfury.io/docs/schemas/preset-conf");
	}
	let presets = presets_opt.unwrap();
	let mut tags = Vec::new();
	for preset in presets {
		if preset.get_name() == args[0] {
			tags = Vec::from(preset.get_tags());
		}
	}

	// Now we also need a valid TaskGraph...
	let tasks = TaskGraph::new(config, fetcher)
		.await?
		.consume_and_get_tasks();

	// Before we start preparing a task for execution, let's ensure all the necessary dirs are
	// created.
	ensure_dirs(config, root_dir)?;

	// Let's fetch all the executors so we know how to assign them to tasks.
	let mut erepo = ExecutorRepository::new(config, fetcher, root_dir).await?;

	// Let's build a list of tasks to execute.
	let mut worker = Worker::new_fifo();
	let task_size = build_concurrent_execution_list(
		&tasks,
		&tags,
		fetcher,
		&mut erepo,
		root_dir.clone(),
		&mut worker,
	)
	.await?;

	// Let's fetch all the helpers.
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
					"One of the inner tasks returned a non-zero exit code: [{}], please use the logs to debug what went wrong.",
					exit_code,
				))
			}
		}
		Err(err_code) => Err(err_code),
	}
}
