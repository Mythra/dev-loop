//! Represents all core types related to tasks inside of dev-loop.
//!
//! Everything from the "DAG" of tasks, to running a specific task, etc.

use crate::{
	config::types::{TaskConf, TaskConfFile, TopLevelConf},
	fetch::{Fetcher, FetcherRepository},
};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use tracing::error;

pub mod execution;
pub mod fs;

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
	/// Create a new TaskGraph.
	///
	/// NOTE: this will completely parse all the task files (remote or otherwise),
	/// and can generally be considered to be one of the longer tasks within dev-loop.
	///
	/// `tlc`: The parsed top level config to start fetching tasks from.
	/// `fetcher`: The repository of fetchers.
	#[allow(clippy::cognitive_complexity)]
	#[must_use]
	#[tracing::instrument]
	pub async fn new(tlc: &TopLevelConf, fetcher: &FetcherRepository) -> Result<Self> {
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

			let mut flatenned_tasks = HashMap::new();

			for task_location in tasks {
				// Go, and fetch all the task locations, if we're searching folders
				// search for "dl-tasks.yml" files.
				let resulting_fetched_tasks = fetcher
					.fetch_filter(task_location, Some("dl-tasks.yml".to_owned()))
					.await;

				// For HTTP errors we're going to try to continue, if your FS fails
				// well than something really bad is going on that we don't want to handle.
				if let Err(err) = resulting_fetched_tasks {
					if task_location.get_type() == "http" {
						error!("Failed to fetch from an HTTP Endpoint. Trying to continue anyway, incase you're running a task that is fully local with no internet...");
						allowing_dag_errors = true;
					} else {
						error!("Location type failed to fetch a filesystem component! Assuming this is a critical error.");
						return Err(anyhow!(
							"Failed to fetch tasks from FS: [{:?}] Internal Err: [{:?}]",
							task_location,
							err
						));
					}
					continue;
				}

				let fetched_tasks = resulting_fetched_tasks.unwrap();
				for task_conf_file in fetched_tasks {
					let task_yaml_res =
						serde_yaml::from_slice::<TaskConfFile>(&task_conf_file.get_contents());
					if let Err(tye) = task_yaml_res {
						if task_location.get_type() == "http" {
							error!("Failed to fetch from an HTTP Endpoint. Trying to continue anyway, incase you're running a task that is fully local with no internet...");
							allowing_dag_errors = true;
							continue;
						}
						return Err(anyhow!(
							"Failed to parse task configuration file yaml. Source: [{}] Err: [{:?}]",
							task_conf_file.get_source(),
							tye,
						));
					}
					let mut task_yaml = task_yaml_res.unwrap();

					if task_conf_file.get_fetched_by() == "path" {
						task_yaml.set_task_location(task_conf_file.get_source());
					}

					// This is the "core" loop, where we've now parsed a task config
					// file, and need to enter it's contents into the DAG. We have to
					// be careful though because there's no order guarantee of the files
					// we're reading. So we have to allow for cases where a task _may_ not
					// be parsed yet.
					for task_conf in task_yaml.consume_tasks() {
						let task_name = task_conf.get_name();

						// If we've already seen this task... it's an error.
						// Task names need to be globally unique.
						if flatenned_tasks.contains_key(task_name) {
							return Err(anyhow!(
								"Found duplicate task: [{}] defined in: [{:?}]",
								task_name,
								task_location,
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
							"oneof" => {
								if let Some(options) = task_conf.get_options() {
									for option in options {
										internal_task_names.remove(option.get_task_name());
										if !flatenned_tasks.contains_key(option.get_task_name()) {
											unsatisfied_task_names
												.insert(option.get_task_name().to_owned());
										}
									}
								}
							}
							"parallel-pipeline" | "pipeline" => {
								if let Some(steps) = task_conf.get_steps() {
									for step in steps {
										internal_task_names.remove(step.get_task_name());
										if !flatenned_tasks.contains_key(step.get_task_name()) {
											unsatisfied_task_names
												.insert(step.get_task_name().to_owned());
										}
									}
								}
							}
							_ => {}
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
					}
				}
			}

			if !allowing_dag_errors {
				// If we had any tasks that we're in a
				// 'oneof'/'parallel-pipeline'/'pipeline', but we never
				// saw... go ahead and error.
				if !unsatisfied_task_names.is_empty() {
					return Err(anyhow!(
						"Found tasks referenced that do not exist: {:?}",
						unsatisfied_task_names
					));
				}

				// If we had any tasks that we're marked internal, but never referenced...
				// go ahead and error.
				if !internal_task_names.is_empty() {
					return Err(anyhow!(
						"Found tasks that are marked internal, but are never referenced: {:?}",
						internal_task_names
					));
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

	/// Get a full list of the tasks that are available.
	#[must_use]
	pub fn get_all_tasks(&self) -> &HashMap<String, TaskConf> {
		&self.flattened_tasks
	}

	/// Consume the overlying tasks type, and get all the tasks.
	#[must_use]
	pub fn consume_and_get_tasks(self) -> HashMap<String, TaskConf> {
		self.flattened_tasks
	}
}
