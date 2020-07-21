//! Represents all of the raw configuration struct types.
//!
//! These are essentially just the actual config objects in a typed structure
//! so they can be deserialized with Serde.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Describes the configuration for a specific provided version
/// of a tool.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProvideConf {
	/// The name of the item provided.
	name: String,
	/// The version of the tool this provides.
	version: Option<String>,
}

/// All of the possible types of executors that dev-loop supports executing.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub enum ExecutorType {
	/// Represents an executor type that utilizes docker containers.
	#[serde(rename = "docker")]
	Docker,
	/// Represents an executor that just uses the raw host.
	#[serde(rename = "host")]
	Host,
}

/// Describes the configuration for an executor.
///
/// This may not be valid executor, this is just the configuration for it.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct ExecutorConf {
	/// The type this executor is.
	///
	/// For example "docker", or "host".
	#[serde(rename = "type")]
	typ: ExecutorType,
	/// The parameters to this particular executor.
	params: Option<HashMap<String, String>>,
	/// The list of provided installed utilities.
	provides: Option<Vec<ProvideConf>>,
}

impl ProvideConf {
	/// Create a new implementation of `ProvideConf`
	#[cfg(test)]
	#[must_use]
	pub fn new(name: String, version: Option<String>) -> Self {
		Self { name, version }
	}

	/// Get the name of the thing provided.
	#[must_use]
	pub fn get_name(&self) -> &str {
		&self.name
	}

	/// Get the version of the thing provided.
	#[must_use]
	pub fn get_version(&self) -> &str {
		if self.version.is_none() {
			""
		} else {
			self.version.as_ref().unwrap()
		}
	}
}

impl ExecutorConf {
	/// Get the type of this executor.
	#[must_use]
	pub fn get_type(&self) -> &ExecutorType {
		&self.typ
	}

	/// Get all the parameters.
	#[must_use]
	pub fn get_parameters(&self) -> HashMap<String, String> {
		self.params.as_ref().cloned().unwrap_or_else(HashMap::new)
	}

	/// Get a provided tool by it's name.
	///
	/// `name` - The name of the tool provided.
	#[must_use]
	pub fn get_provided(&self) -> Vec<ProvideConf> {
		self.provides.as_ref().cloned().unwrap_or_else(Vec::new)
	}
}

/// All of the possible types of locations that dev-loop supports fetching from.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum LocationType {
	/// Fetch from a path on the filesystem.
	#[serde(rename = "path")]
	Path,
	/// Fetch from an HTTP(S) endpoint.
	#[serde(rename = "http")]
	HTTP,
}

impl std::fmt::Display for LocationType {
	fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
		match *self {
			LocationType::Path => formatter.write_str("path"),
			LocationType::HTTP => formatter.write_str("http"),
		}
	}
}

/// Describes a particular location that is _somewhere_ to read data from.
/// For example a path on the filesystem, or a remote location over say HTTP.
///
/// This may not be a valid location (and location type), but is just the
/// configuration.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct LocationConf {
	/// The type of this location.
	///
	/// Currently the two supported values are: `http`, and `path`.
	/// However, these could be expanded in the future.
	#[serde(rename = "type")]
	typ: LocationType,
	/// The actual "place" of this location.
	///
	/// For a `path` type this is the place on the filesystem.
	/// For a `http` type this is the URL in which to fetch.
	at: String,
	/// Whether or not to recursively look at this location.
	///
	/// Only valid for `path` currently, ignored otherwise.
	recurse: Option<bool>,
}

impl LocationConf {
	/// Return what type of location this is.
	///
	/// This gives you an idea of how to handle the "at" value.
	#[must_use]
	pub fn get_type(&self) -> &LocationType {
		&self.typ
	}

	/// Return where this location is.
	#[must_use]
	pub fn get_at(&self) -> &str {
		&self.at
	}

	/// Return where this location needs to recurse.
	#[must_use]
	pub fn get_recurse(&self) -> bool {
		self.recurse.unwrap_or(false)
	}
}

/// Describes a preset, or a predefined "tag group" to run.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct PresetConf {
	/// The name of this particular preset.
	name: String,
	/// The description of this particular preset.
	description: Option<String>,
	/// The list of tags that are included in this preset.
	tags: Vec<String>,
}

impl PresetConf {
	/// Get the name of this preset, which is also a global identifier.
	#[must_use]
	pub fn get_name(&self) -> &str {
		&self.name
	}

	/// Get the description of this preset.
	#[must_use]
	pub fn get_description(&self) -> Option<&str> {
		// &String vs &str :'(
		if let Some(desc) = &self.description {
			Some(desc)
		} else {
			None
		}
	}

	/// Get the tags that are part of this preset.
	#[must_use]
	pub fn get_tags(&self) -> &[String] {
		&self.tags
	}
}

/// The `TopLevelConf` for dev-loop, also known as what's in
/// `.dl/config.yml`.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct TopLevelConf {
	/// The default executor to use if no other executor has been specified,
	/// or if no requirements have been specified.
	default_executor: Option<ExecutorConf>,
	/// The list of directories to ensure exist before running a task.
	ensure_directories: Option<Vec<String>>,
	/// Defines a place for executors.
	executor_locations: Option<Vec<LocationConf>>,
	/// The list of locations to fetch helpers from.
	helper_locations: Option<Vec<LocationConf>>,
	/// The list of presets, or presets which can be run by default.
	presets: Option<Vec<PresetConf>>,
	/// The list of locations for task files to be found.
	task_locations: Option<Vec<LocationConf>>,
}

impl TopLevelConf {
	/// Create an empty top level configuration.
	#[must_use]
	pub fn create_empty_config() -> Self {
		Self {
			default_executor: None,
			ensure_directories: None,
			executor_locations: None,
			helper_locations: None,
			presets: None,
			task_locations: None,
		}
	}

	/// Get the default executor if one has been defined.
	#[must_use]
	pub fn get_default_executor(&self) -> Option<&ExecutorConf> {
		self.default_executor.as_ref()
	}

	/// Get the list of locations where helpers are located.
	#[must_use]
	pub fn get_helper_locations(&self) -> Option<&Vec<LocationConf>> {
		self.helper_locations.as_ref()
	}

	/// Get the list of locations where executor definitions are located.
	#[must_use]
	pub fn get_executor_locations(&self) -> Option<&Vec<LocationConf>> {
		self.executor_locations.as_ref()
	}

	/// Get the list of locations where tasks are located.
	#[must_use]
	pub fn get_task_locations(&self) -> Option<&Vec<LocationConf>> {
		self.task_locations.as_ref()
	}

	/// Get the list of directories to ensure exist.
	#[must_use]
	pub fn get_dirs_to_ensure(&self) -> Option<&Vec<String>> {
		self.ensure_directories.as_ref()
	}

	/// Get the list of presets for dev-loop.
	#[must_use]
	pub fn get_presets(&self) -> Option<&Vec<PresetConf>> {
		self.presets.as_ref()
	}
}

/// Describes a requirement that's needed for a particular task.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct NeedsRequirement {
	/// The name of this requirement.
	name: String,
	/// The version matched requirement specified.
	///
	/// Should be a semver matching string.
	version_matcher: Option<String>,
}

impl NeedsRequirement {
	/// Create a new `NeedsRequirements`.
	#[cfg(test)]
	#[must_use]
	pub fn new(name: String, version_matcher: Option<String>) -> Self {
		Self {
			name,
			version_matcher,
		}
	}
	/// Get the name of this particular requirement.
	#[must_use]
	pub fn get_name(&self) -> &str {
		&self.name
	}

	/// Get the version matched requirement specified.
	///
	/// Should be a semver matching string.
	#[must_use]
	pub fn get_version_matcher(&self) -> Option<&str> {
		// &String vs &str :'(
		if let Some(vm) = &self.version_matcher {
			Some(vm)
		} else {
			None
		}
	}
}

/// Describes a particular step in a pipeline.
///
/// These only ever get used for a task type of pipeline, but if it makes
/// you feel better you can put them in any task to be fair.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct PipelineStep {
	/// The name of this pipeline step.
	///
	/// Not globally unique, but should be unique within the pipeline.
	name: String,
	/// The description of this pipeline step.
	///
	/// NOTE: we won't ever display this right now because frankly we haven't
	/// seen a nice way of displaying it yet. However, we want to allow people
	/// to configure it now, so it's not breaking whenever we do add it in.
	description: Option<String>,
	/// The name of the actual task to run.
	task: String,
	/// The list of arguments to pass into the task as if it were being run
	/// directly.
	args: Option<Vec<String>>,
}

impl PipelineStep {
	/// Get the name of this `PipelineStep`
	#[must_use]
	pub fn get_name(&self) -> &str {
		&self.name
	}

	/// Get the description of this `PipelineStep`
	#[allow(unused)]
	#[must_use]
	pub fn get_description(&self) -> Option<&str> {
		if let Some(desc) = &self.description {
			Some(desc)
		} else {
			None
		}
	}

	/// Get the name of the task to run for this `PipelineStep`
	#[must_use]
	pub fn get_task_name(&self) -> &str {
		&self.task
	}

	/// Get the arguments for this step of the pipeline
	#[must_use]
	pub fn get_args(&self) -> Option<&Vec<String>> {
		self.args.as_ref()
	}
}

/// Describe a particular option inside a oneof task.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct OneofOption {
	/// The name of this option. Will be used to match with the argument.
	///
	/// As such it should be unique within the task options.
	name: String,
	/// The list of arguments to pass into the task as if it were being run
	/// directly.
	args: Option<Vec<String>>,
	/// The description of this option. Will be shown when called: `list <oneofname>`.
	description: Option<String>,
	/// The name of the task to run when this particular oneof option is selected.
	task: String,
	/// The list of tags that apply to this particular option.
	tags: Option<Vec<String>>,
}

impl OneofOption {
	/// Get the name of this particular option.
	#[must_use]
	pub fn get_name(&self) -> &str {
		&self.name
	}

	/// Get the arguments for this step of the pipeline
	#[must_use]
	pub fn get_args(&self) -> Option<&Vec<String>> {
		self.args.as_ref()
	}

	/// Get the description of this particular option.
	#[must_use]
	pub fn get_description(&self) -> Option<&str> {
		if let Some(desc) = &self.description {
			Some(desc)
		} else {
			None
		}
	}

	/// Get the name of the task to run for this option.
	#[must_use]
	pub fn get_task_name(&self) -> &str {
		&self.task
	}

	/// Get the list of tags that apply to this option.
	#[must_use]
	pub fn get_tags(&self) -> Option<&Vec<String>> {
		self.tags.as_ref()
	}
}

/// All of the possible types of tasks that dev-loop supports executing.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub enum TaskType {
	/// Represents a "command", or a task that actually executes a script.
	#[serde(rename = "command")]
	Command,
	/// Represents a task that can be chosen.
	#[serde(rename = "oneof")]
	Oneof,
	/// Represents a task that executes a series of steps in a specific order.
	#[serde(rename = "pipeline")]
	Pipeline,
	/// Represents a task that executes multiple tasks at once.
	#[serde(rename = "parallel-pipeline")]
	ParallelPipeline,
}

impl std::fmt::Display for TaskType {
	fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
		match *self {
			TaskType::Command => formatter.write_str("command"),
			TaskType::Oneof => formatter.write_str("oneof"),
			TaskType::Pipeline => formatter.write_str("pipeline"),
			TaskType::ParallelPipeline => formatter.write_str("parallel-pipeline"),
		}
	}
}

/// Represents the configuration for a singular task.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct TaskConf {
	/// The name of this task, this should be globally unique.
	name: String,
	/// The type of this task, currently these are: "command", "pipeline",
	/// and "oneof".
	#[serde(rename = "type")]
	typ: Option<TaskType>,
	/// The description of this task.
	description: Option<String>,
	/// The location of this task, this will only be used on a command type
	/// of task. (or the type when one isn't specified).
	location: Option<LocationConf>,
	/// A list of things needed by the executor, executing this task.
	execution_needs: Option<Vec<NeedsRequirement>>,
	/// Represents an optional custom executor for this particular task.
	custom_executor: Option<ExecutorConf>,
	/// The list of steps to run when it is a pipeline.
	steps: Option<Vec<PipelineStep>>,
	/// The list of options to choose from when the task is a "oneof" type.
	options: Option<Vec<OneofOption>>,
	/// Get the list of tags that apply to this task
	tags: Option<Vec<String>>,
	/// If this task is "internal", e.g. should not be shown
	/// in the "list" command.
	internal: Option<bool>,
	/// If this task should keep running until a user hits CtrlC. E.g.
	/// Ctrl-C should not be marked as a failure.
	ctrlc_is_failure: Option<bool>,
	/// Represents the source path this task configuration file is at.
	/// This is always overriden by dev-loop itself, and will never
	/// use a user provided value.
	#[serde(rename = "completely_useless")]
	source_path: Option<String>,
}

impl TaskConf {
	/// Get the name of this particular task.
	#[must_use]
	pub fn get_name(&self) -> &str {
		&self.name
	}

	/// Set the source path for this `TaskConfiguration`.
	pub fn set_path(&mut self, source_path: String) {
		self.source_path = Some(source_path);
	}

	/// Get the type of this particular task.
	#[must_use]
	pub fn get_type(&self) -> &TaskType {
		if let Some(the_type) = &self.typ {
			the_type
		} else {
			&TaskType::Command
		}
	}

	/// Get the description of this particular task.
	#[must_use]
	pub fn get_description(&self) -> Option<&str> {
		if let Some(desc) = &self.description {
			Some(desc)
		} else {
			None
		}
	}

	/// Get the location of this particular task, note this is only required
	/// for a type of command, and needs to be checked itself.
	#[must_use]
	pub fn get_location(&self) -> Option<&LocationConf> {
		self.location.as_ref()
	}

	/// Get the list of needed things for an executor.
	#[must_use]
	pub fn get_execution_needs(&self) -> Option<&Vec<NeedsRequirement>> {
		self.execution_needs.as_ref()
	}

	/// Get the custom executor this task has specified to run in.
	#[must_use]
	pub fn get_custom_executor(&self) -> Option<&ExecutorConf> {
		self.custom_executor.as_ref()
	}

	/// Get the list of steps to run when this task is a pipeline.
	#[must_use]
	pub fn get_steps(&self) -> Option<&Vec<PipelineStep>> {
		self.steps.as_ref()
	}

	/// Get the list of options to choose from when this is a 'oneof' type.
	#[must_use]
	pub fn get_options(&self) -> Option<&Vec<OneofOption>> {
		self.options.as_ref()
	}

	/// Get the list of tags for this particular task.
	#[must_use]
	pub fn get_tags(&self) -> Option<&Vec<String>> {
		self.tags.as_ref()
	}

	/// Determine if this task is an "internal" one.
	#[must_use]
	pub fn is_internal(&self) -> bool {
		self.internal.unwrap_or(false)
	}

	/// Determine if this task should treat Ctrl-C as failure.
	#[must_use]
	pub fn ctrlc_is_failure(&self) -> bool {
		self.ctrlc_is_failure.unwrap_or(true)
	}

	/// Get the original path of this particular task.
	#[must_use]
	pub fn get_source_path(&self) -> &str {
		if let Some(sp) = &self.source_path {
			sp
		} else {
			""
		}
	}
}

/// Represents the config that lives inside of a tasks configuration file.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct TaskConfFile {
	/// The list of tasks to add to the global list of tasks.
	tasks: Vec<TaskConf>,
}

impl TaskConfFile {
	/// Set the task location for all the configuration items
	/// in this file.
	pub fn set_task_location(&mut self, loc: &str) {
		for task in &mut self.tasks {
			task.set_path(loc.to_owned());
		}
	}

	/// Get the tasks from this file consuming the `TaskConfFile`.
	#[must_use]
	pub fn consume_tasks(self) -> Vec<TaskConf> {
		self.tasks
	}
}

/// Represents the config that lives inside of a executor configuration file.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct ExecutorConfFile {
	/// The list of executors to add to the global list of executors.
	executors: Vec<ExecutorConf>,
}

impl ExecutorConfFile {
	/// Consume the representation of this `ExecutorConfFile`, and receive the executors.
	#[must_use]
	pub fn consume_and_get_executors(self) -> Vec<ExecutorConf> {
		self.executors
	}
}
