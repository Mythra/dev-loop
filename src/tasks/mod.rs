//! Represents all core types related to tasks inside of dev-loop.
//!
//! Everything from the "DAG" of tasks, to running a specific task, etc.

use crate::{
	config::types::{LocationType, TaskConf, TaskConfFile, TaskType, TopLevelConf},
	fetch::FetcherRepository,
	strsim::add_did_you_mean_text,
	yaml_err::contextualize,
};

use color_eyre::{
	eyre::{eyre, WrapErr},
	Result, Section,
};
use std::collections::{HashMap, HashSet};
use tracing::warn;

pub(crate) mod execution;
pub(crate) mod fs;

/// Describes the full "graph" of tasks.
///
/// This is how we "navigate" the configuration, and actually ensure that the
/// configuration of at least the task files are valid.
#[derive(Debug, PartialEq)]
pub struct TaskGraph {
	/// The list of tasks that have been "flatenned", or are
	/// not in any special type of graph structure.
	flattened_tasks: HashMap<String, TaskConf>,
}

impl TaskGraph {
	fn parse_task(
		task_conf_file_src: &str,
		task_conf: TaskConf,
		internal_task_names: &mut HashSet<String>,
		unsatisfied_task_names: &mut HashSet<String>,
		flatenned_tasks: &mut HashMap<String, TaskConf>,
	) -> Result<()> {
		let task_name = task_conf.get_name();

		// If we've already seen this task... it's an error.
		// Task names need to be globally unique.
		if let Some(other_task_conf) = flatenned_tasks.get(task_name) {
			return Err(eyre!(
				"Found duplicate task named: [{}]. Originally defined in config at: [{}], found again in config at: [{}]",
				task_name,
				other_task_conf.get_source_path(),
				task_conf_file_src,
			));
		}

		// If it's a 'oneof', 'parallel-pipeline', or 'pipeline' type, we
		// need to parse it's children so we can ensure everything is valid.
		//
		// We call `internal_task_names.remove()` always (it'll be a no-op if it
		// doesn't contain the key). The `internal_task_names` are tasks that are marked
		// `internal: true`, but don't yet have a reference. By being in a
		// oneof/parallel-pipeline/pipeline they themselves have a reference.
		//
		// Next we check if the option "exists", if not. we add it to `unsatisfied_task_names`
		// so it can be checked later.
		let ttype = task_conf.get_type();
		match ttype {
			TaskType::Oneof => {
				if let Some(options) = task_conf.get_options() {
					for option in options {
						internal_task_names.remove(option.get_task_name());
						if !flatenned_tasks.contains_key(option.get_task_name()) {
							unsatisfied_task_names.insert(option.get_task_name().to_owned());
						}
					}
				}
			}
			TaskType::Pipeline | TaskType::ParallelPipeline => {
				if let Some(steps) = task_conf.get_steps() {
					for step in steps {
						internal_task_names.remove(step.get_task_name());
						if !flatenned_tasks.contains_key(step.get_task_name()) {
							unsatisfied_task_names.insert(step.get_task_name().to_owned());
						}
					}
				}
			}
			TaskType::Command => {}
		}

		// If we're an internal task, and someone hasn't referenced us already
		// go ahead and add ourselves to the list of "waiting for a ref" set.
		if task_conf.is_internal() && !unsatisfied_task_names.contains(task_name) {
			internal_task_names.insert(task_name.to_owned());
		}
		// NO-OP if we're not there, otherwise let people know we exist.
		unsatisfied_task_names.remove(task_name);

		// Add ourselves to the final map.
		flatenned_tasks.insert(task_name.to_owned(), task_conf);

		Ok(())
	}

	/// Create a new `TaskGraph`.
	///
	/// NOTE: this will completely parse all the task files (remote or otherwise),
	/// and can generally be considered to be one of the longer tasks within dev-loop.
	///
	/// `tlc`: The parsed top level config to start fetching tasks from.
	/// `fetcher`: The repository of fetchers.
	///
	/// # Errors
	///
	/// - When there is an error fetching the tasks yaml files.
	/// - When the task yaml files are invalid yaml.
	/// - When the task yaml file has some sort of invariant error.
	pub async fn new(tlc: &TopLevelConf, fetcher: &FetcherRepository) -> Result<Self> {
		let span = tracing::info_span!("finding_tasks");
		let _guard = span.enter();

		// If we have tasks, we have some fetching to do...
		if let Some(tasks) = tlc.get_task_locations() {
			// These are hashsets to track for a "valid" DAG. A Valid DAG:
			//   1. Does not have any "internal: true" nodes that are never referenced
			//      (and thus can never be reached).
			//   2. Does not have a task referenced that does not exist.
			//
			// There is one case where we allow for an invalid DAG. This is when we fail to
			// fetch an HTTP endpoint. This is because you might be on say a plane, and want to
			// run a task that's entirely local. So for HTTP failures we will purposefully not
			// validate the DAG, to try and let the program run. Obviously if someone tries to run
			// a task from that HTTP endpoint on a plane, there's nothing we can do.
			let mut internal_task_names = HashSet::new();
			let mut unsatisfied_task_names = HashSet::new();
			let mut allowing_dag_errors = false;

			let mut flatenned_tasks: HashMap<String, TaskConf> = HashMap::new();

			for (tl_idx, task_location) in tasks.iter().enumerate() {
				// Go, and fetch all the task locations, if we're searching folders
				// search for "dl-tasks.yml" files.
				let resulting_fetched_tasks = fetcher
					.fetch_filter(task_location, Some("dl-tasks.yml".to_owned()))
					.await
					.wrap_err(format!(
						"Failed fetching tasks specified at `.dl/config.yml:task_locations:{}`",
						tl_idx,
					));

				// For HTTP errors we're going to try to continue, if your FS fails
				// well than something really bad is going on that we don't want to handle.
				if let Err(err) = resulting_fetched_tasks {
					if task_location.get_type() == &LocationType::HTTP {
						warn!("{:?}", err);
						warn!("Trying to continue, incase the failing remote endpoint doesn't matter for this run.");
						allowing_dag_errors = true;
						continue;
					}

					warn!("Failed to fetch a file from the filesystem! Assuming this is a critical error.");
					return Err(err.wrap_err(format!(
						"Failed to read the file: [{}] from the filesystem",
						task_location.get_at()
					)));
				}

				for task_conf_file in resulting_fetched_tasks.unwrap() {
					let task_yaml_res =
						serde_yaml::from_slice::<TaskConfFile>(&task_conf_file.get_contents());
					if let Err(tye) = task_yaml_res {
						if task_location.get_type() == &LocationType::HTTP {
							warn!("{:?}", tye,);
							warn!("Trying to continue, incase the failing remote endpoint doesn't matter for this run.");
							allowing_dag_errors = true;
							continue;
						}

						return contextualize(
							Err(tye),
							task_conf_file.get_source(),
							&String::from_utf8_lossy(task_conf_file.get_contents()).to_string()
						).note("Full types, and supported values are documented at: https://dev-loop.kungfury.io/docs/schemas/task-conf-file");
					}
					let mut task_yaml = task_yaml_res.unwrap();
					task_yaml.set_task_location(task_conf_file.get_source());

					// This is the "core" loop, where we've now parsed a task config
					// file, and need to enter it's contents into the DAG. We have to
					// be careful though because there's no order guarantee of the files
					// we're reading. So we have to allow for cases where a task _may_ not
					// be parsed yet.
					for task_conf in task_yaml.consume_tasks() {
						Self::parse_task(
							task_conf_file.get_source(),
							task_conf,
							&mut internal_task_names,
							&mut unsatisfied_task_names,
							&mut flatenned_tasks,
						)?;
					}
				}
			}

			if !allowing_dag_errors {
				// If we had any tasks that we're in a
				// 'oneof'/'parallel-pipeline'/'pipeline', but we never
				// saw... go ahead and error.

				if !unsatisfied_task_names.is_empty() {
					let mut err = Err(eyre!(
						"Tasks referenced that do not exist: {:?}",
						unsatisfied_task_names
					));
					for unknown_task in unsatisfied_task_names {
						err = add_did_you_mean_text(
							err,
							&unknown_task,
							&flatenned_tasks
								.keys()
								.map(String::as_str)
								.collect::<Vec<&str>>(),
							3,
							None,
						);
					}

					return err;
				}

				// If we had any tasks that we're marked internal, but never referenced...
				// go ahead and error.
				if !internal_task_names.is_empty() {
					return Err(eyre!(
						"Found tasks that are marked internal, but are never referenced: {:?}",
						internal_task_names
					))
					.suggestion("If an internal task is no longer needed it should be deleted.");
				}
			}

			Ok(Self {
				flattened_tasks: flatenned_tasks,
			})
		} else {
			Ok(Self {
				flattened_tasks: HashMap::new(),
			})
		}
	}

	/// Consume the overlying tasks type, and get all the tasks.
	#[must_use]
	pub fn consume_and_get_tasks(self) -> HashMap<String, TaskConf> {
		self.flattened_tasks
	}
}
