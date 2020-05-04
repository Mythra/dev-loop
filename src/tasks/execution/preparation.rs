use crate::{
	config::types::{TaskConf, TopLevelConf},
	executors::{Executor, ExecutorRepository},
	fetch::{FetchedItem, Fetcher, FetcherRepository},
};
use anyhow::{anyhow, Result};
use crossbeam_deque::Worker;
use std::{
	collections::{HashMap, HashSet},
	fmt::{Debug, Formatter},
	future::Future,
	hash::BuildHasher,
	iter::FromIterator,
	path::PathBuf,
	pin::Pin,
	sync::Arc,
};
use tracing::{debug, info};
use uuid::Uuid;

/// Represents an `ExecutableTask`, or a task that contains all the necessary
/// bits of info needed for execution within an executor.
pub struct ExecutableTask {
	/// The arguments for this particular task.
	args: Vec<String>,
	/// The executor that was chosen to be used.
	chosen_executor: Arc<dyn Executor + Sync + Send>,
	/// The Pipeline ID represents a "namespace"
	/// that executors should use in order to "seperate" tasks
	/// to each other. For example in the docker executor the pipeline id
	/// corresponds to a docker network, so things in the same pipeline can
	/// communicate.
	pipeline_id: String,
	/// Get the contents of this particular task file.
	script_contents: FetchedItem,
	/// The name of the task.
	task_name: String,
}

impl Debug for ExecutableTask {
	fn fmt(&self, formatter: &mut Formatter) -> Result<(), std::fmt::Error> {
		formatter.write_str(&format!(
			"ExecutableTask args: {}{}{} pipeline_id: {}{}{}, task_name: {}{}{}",
			"{",
			self.args.join(" "),
			"}",
			"{",
			self.pipeline_id,
			"}",
			"{",
			self.task_name,
			"}",
		))
	}
}

impl ExecutableTask {
	/// Create a new represenation of an executable Task.
	#[must_use]
	pub fn new(
		args: Vec<String>,
		executor: Arc<dyn Executor + Sync + Send>,
		contents: FetchedItem,
		pipeline_id: String,
		task_name: String,
	) -> Self {
		Self {
			args,
			chosen_executor: executor,
			pipeline_id,
			script_contents: contents,
			task_name,
		}
	}

	#[must_use]
	pub fn get_arg_string(&self) -> String {
		self.args.join(" ")
	}

	/// Get the pipeline id for this task.
	#[must_use]
	pub fn get_pipeline_id(&self) -> &str {
		&self.pipeline_id
	}

	/// Get the name of this task.
	#[must_use]
	pub fn get_task_name(&self) -> &str {
		&self.task_name
	}

	/// Get the fetched contents of this task.
	#[must_use]
	pub fn get_contents(&self) -> &FetchedItem {
		&self.script_contents
	}

	/// Get the executor for this particular task.
	#[must_use]
	pub fn get_executor(&self) -> &Arc<dyn Executor + Sync + Send> {
		&self.chosen_executor
	}
}

/// Describes a particular workable unit, this ensures work can be stolen easily
/// from a single queue.
pub enum WorkUnit {
	/// A SingleTask that it's in the work queue.
	SingleTask(ExecutableTask),
	/// A Pipeline of tasks, that all need to be worked in a specific order.
	Pipeline(Vec<ExecutableTask>),
}

/// Describes a type of work queue. This helps easily build a pipeline of
/// tasks, and a work queue.
pub enum WorkQueue<'a> {
	/// An actual worker.
	Queue(&'a mut Worker<WorkUnit>),
	/// A vector of tasks.
	VecQueue(&'a mut Vec<ExecutableTask>),
}

/// Turns a command type task into an executable task.
async fn command_to_executable_task(
	pipeline_id: String,
	task: &TaskConf,
	fetcher: &FetcherRepository,
	executors: &mut ExecutorRepository,
	root_directory: PathBuf,
	args: Vec<String>,
) -> Result<ExecutableTask> {
	// First select the executor for this environment.
	let selected_executor = executors.select_executor(task).await;
	if selected_executor.is_none() {
		return Err(anyhow!(
			"Couldn't select executor for task: [{}]",
			task.get_name()
		));
	}
	let selected_executor = selected_executor.unwrap();

	// Get the location of the script for this task.
	let loc = task.get_location();
	if loc.is_none() {
		return Err(anyhow!(
			"Command type task does not have a location of a script: [{}]",
			task.get_name()
		));
	}
	let loc = loc.unwrap();
	// Now this location may be "relative" to the path of the task file.
	// It is relative when the task configuration is from the filesystem.
	//
	// A task file fetched from a remote endpoint specifying a FS endpoint
	// would fetch from the root of the project, since it doesn't have an
	// idea of what to be "relative" too.
	let tf_loc = task.get_source_path();
	let root_path = if let Some(task_path) = tf_loc {
		let mut tpb = PathBuf::from(task_path);
		tpb.pop();
		tpb
	} else {
		root_directory
	};
	let resulting_items = fetcher
		.fetch_with_root_and_filter(loc, &root_path, None)
		.await?;
	if resulting_items.len() != 1 {
		return Err(anyhow!(
			"Found more than one executable file for task: [{}]",
			task.get_name()
		));
	}
	let resulting_item = resulting_items.into_iter().next().unwrap();

	Ok(ExecutableTask::new(
		args,
		selected_executor,
		resulting_item,
		pipeline_id,
		task.get_name().to_owned(),
	))
}

/// Create a new pipeline id.
#[must_use]
pub fn new_pipeline_id() -> String {
	format!("{}", Uuid::new_v4())
}

/// Determine if a particular iter has all unique elements.
#[must_use]
pub fn has_unique_elements<T>(iter: T) -> bool
where
	T: IntoIterator,
	T::Item: Eq + std::hash::Hash,
{
	let mut uniq = HashSet::new();
	iter.into_iter().all(move |x| uniq.insert(x))
}

/// Taking the full (valid) map of tasks, and a `starting_task` to start with
/// build a full list of the tasks that need to be executed, in which order.
///
/// `tasks`: the consumed state of a once valid `TaskDag`.
/// `starting_task`: the task to actually start with.
/// `fetcher`: Used to fetch the actual task contents.
/// `executors`: The repository of executors.
/// `root_directory`: The root directory of the filesystem.
/// `arguments`: The arguments provided (from wherever) to know how to
///              properly traverse oneof's.
/// `pipeline_id`: the id of this pipeline.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
#[must_use]
pub fn build_ordered_execution_list<'a, H: BuildHasher>(
	tasks: &'a HashMap<String, TaskConf, H>,
	starting_task: &'a TaskConf,
	fetcher: &'a FetcherRepository,
	executors: &'a mut ExecutorRepository,
	root_directory: PathBuf,
	arguments: &'a [String],
	pipeline_id: String,
	work_queue: &'a mut WorkQueue,
) -> Pin<Box<dyn 'a + Future<Output = Result<usize>>>> {
	Box::pin(async move {
		let mut size = 0;

		match starting_task.get_type() {
			"command" => {
				match work_queue {
					WorkQueue::Queue(queue) => queue.push(WorkUnit::SingleTask(
						command_to_executable_task(
							pipeline_id,
							starting_task,
							fetcher,
							executors,
							root_directory,
							Vec::from(arguments),
						)
						.await?,
					)),
					WorkQueue::VecQueue(vec) => vec.push(
						command_to_executable_task(
							pipeline_id,
							starting_task,
							fetcher,
							executors,
							root_directory,
							Vec::from(arguments),
						)
						.await?,
					),
				}
				size += 1;
			}
			"oneof" => {
				// Parse a `oneof` type into a list of tasks.
				// This _will_ recurse if an option is selected that is not a command task.

				// First make sure someone has specified an options block for a oneof type.
				let options = starting_task.get_options();
				if options.is_none() {
					return Err(anyhow!(
						"Task type is marked oneof but has no options: [{}]",
						starting_task.get_name()
					));
				}
				let options = options.unwrap();

				// If someone specified an empty options array, assume it's intentional.
				if options.is_empty() {
					return Ok(size);
				}
				// If it's not an empty set of options we need to know how to choose one of the tasks.
				if arguments.is_empty() {
					return Err(anyhow!(
						"The Task: [{}] needs an argument to know what to invoke!",
						starting_task.get_name(),
					));
				}

				// Try to grab the option based on the first argument.
				// The other arguments are dropped on purpose.
				let potential_option = options
					.iter()
					.find(|option| option.get_name() == arguments[0]);
				if potential_option.is_none() {
					return Err(anyhow!(
						"The Task: [{}] does  not have a sub-task of: [{}]",
						starting_task.get_name(),
						arguments[0],
					));
				}
				let selected_option = potential_option.unwrap();

				// Try to turn that option into a relevant task.
				//
				// Remember we may have failed fetching from a remote endpoint.
				// so it may not be in the TaskGraph.
				let potential_option_as_task = tasks.get(selected_option.get_task_name());
				if potential_option_as_task.is_none() {
					return Err(anyhow!(
						"The Option: [{}] for Task: [{}] cannot find the task referenced: [{}], it is most likely a task fetched from a remote endpoint that failed.",
						selected_option.get_name(),
						starting_task.get_name(),
						selected_option.get_task_name(),
					));
				}
				let task = potential_option_as_task.unwrap();

				let final_args = if let Some(args_ref) = selected_option.get_args() {
					args_ref.clone()
				} else {
					Vec::new()
				};

				// Now let's add this task to the list of things to run.
				match task.get_type() {
					"command" => {
						match work_queue {
							WorkQueue::Queue(queue) => queue.push(WorkUnit::SingleTask(
								command_to_executable_task(
									pipeline_id,
									task,
									fetcher,
									executors,
									root_directory,
									final_args,
								)
								.await?,
							)),
							WorkQueue::VecQueue(vec) => vec.push(
								command_to_executable_task(
									pipeline_id,
									task,
									fetcher,
									executors,
									root_directory,
									final_args,
								)
								.await?,
							),
						}

						size += 1;
					}
					"oneof" | "pipeline" => {
						size += build_ordered_execution_list(
							tasks,
							task,
							fetcher,
							executors,
							root_directory,
							&final_args,
							pipeline_id,
							work_queue,
						)
						.await?;
					}
					_ => {
						return Err(anyhow!("Unknown task type for task: [{}]", task.get_name()));
					}
				}
			}
			"pipeline" => {
				let optional_steps = starting_task.get_steps();
				if optional_steps.is_none() {
					return Err(anyhow!(
						"Pipeline task: [{}] does not have any steps.",
						starting_task.get_name(),
					));
				}

				let steps = optional_steps.unwrap();
				let my_pid = new_pipeline_id();
				info!(
					"Pipeline task: [{}] has been given the pipeline-id: [{}]",
					starting_task.get_name(),
					my_pid,
				);

				let mut executable_steps = Vec::new();
				let mut executable_steps_as_queue = WorkQueue::VecQueue(&mut executable_steps);

				for step in steps {
					let potential_task = tasks.get(step.get_task_name());
					if potential_task.is_none() {
						return Err(anyhow!(
							"The Step: [{}] for Task: [{}] cannot find the task referenced: [{}], it is most likely a task fetched from a remote endpoint that failed.",
							step.get_name(),
							starting_task.get_name(),
							step.get_task_name(),
						));
					}
					let task = potential_task.unwrap();

					let final_args = if let Some(args_ref) = step.get_args() {
						args_ref.clone()
					} else {
						Vec::new()
					};

					match task.get_type() {
						"command" => {
							match executable_steps_as_queue {
								WorkQueue::Queue(_) => unreachable!(),
								WorkQueue::VecQueue(ref mut vec) => vec.push(
									command_to_executable_task(
										my_pid.clone(),
										task,
										fetcher,
										executors,
										root_directory.clone(),
										final_args,
									)
									.await?,
								),
							};
						}
						"oneof" | "pipeline" => {
							if let Some(args) = step.get_args() {
								build_ordered_execution_list(
									tasks,
									task,
									fetcher,
									executors,
									root_directory.clone(),
									&args,
									my_pid.clone(),
									&mut executable_steps_as_queue,
								)
								.await?;
							} else {
								let args = Vec::new();
								build_ordered_execution_list(
									tasks,
									task,
									fetcher,
									executors,
									root_directory.clone(),
									&args,
									my_pid.clone(),
									&mut executable_steps_as_queue,
								)
								.await?;
							}
						}
						_ => {
							return Err(anyhow!(
								"Unknown task type for task: [{}]",
								task.get_name()
							));
						}
					}
				}

				size += executable_steps.len();
				match work_queue {
					WorkQueue::Queue(queue) => queue.push(WorkUnit::Pipeline(executable_steps)),
					WorkQueue::VecQueue(vec) => vec.extend(executable_steps),
				}
			}
			_ => {
				return Err(anyhow!(
					"Unknown task type for task: [{}]",
					starting_task.get_name(),
				));
			}
		}

		Ok(size)
	})
}

/// Fetch all the helper scripts.
///
/// `tlc`: The top level config.
/// `fr`: The fetcher repository.
///
/// # Errors
///
/// If there was an error downloading the helpers.
pub async fn fetch_helpers(tlc: &TopLevelConf, fr: &FetcherRepository) -> Result<Vec<FetchedItem>> {
	if let Some(helper_locations) = tlc.get_helper_locations() {
		let mut fetched_items = Vec::new();

		for loc in helper_locations {
			fetched_items.extend(fr.fetch_filter(loc, Some(".sh".to_owned())).await?);
		}

		Ok(fetched_items)
	} else {
		Ok(Vec::new())
	}
}

/// Build a concurrent execution list to use for the run command.
///
/// `tasks`: the list of tasks to potentially run.
/// `fetcher`: used for fetching particular files/executors/etc.
/// `executors`: the list of executors.
/// `root_directory`: the root directory of the project.
#[allow(clippy::cast_possible_wrap, clippy::type_complexity)]
#[must_use]
pub fn build_concurrent_execution_list<'a, H: BuildHasher>(
	tasks: &'a HashMap<String, TaskConf, H>,
	tags: &'a [String],
	fetcher: &'a FetcherRepository,
	executors: &'a mut ExecutorRepository,
	root_directory: PathBuf,
	work_queue: &'a mut Worker<WorkUnit>,
) -> Pin<Box<dyn 'a + Future<Output = Result<usize>>>> {
	Box::pin(async move {
		let unique_tags: HashSet<&String> = HashSet::from_iter(tags.iter());
		let mut as_queue = WorkQueue::Queue(work_queue);
		let mut size = 0;

		for (task_name, task) in tasks {
			if task.is_internal() {
				debug!("Skipping Task: {} because it is internal", task_name);
				continue;
			}

			if let Some(tags_on_task) = task.get_tags() {
				let uniq_tags_on_task: HashSet<&String> = HashSet::from_iter(tags_on_task.iter());
				// We had an intersection of some tags.
				if !has_unique_elements(unique_tags.iter().chain(uniq_tags_on_task.iter())) {
					// We found a task to run.
					size += build_ordered_execution_list(
						tasks,
						task,
						fetcher,
						executors,
						root_directory.clone(),
						&Vec::new(),
						new_pipeline_id(),
						&mut as_queue,
					)
					.await?;
				}
			}
		}

		Ok(size)
	})
}
