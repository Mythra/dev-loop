//! Contains all the codes necessary for "Executors". Executors are what are
//! responsible for "abstract"'ing away the system level resources, and
//! managing the the lifecycle of anything it creates. For example if using
//! a docker executor the code that spins up/down the container will be here.

use crate::{
	config::types::{
		ExecutorConf, ExecutorConfFile, ExecutorType, LocationType, NeedsRequirement, TaskConf,
		TopLevelConf,
	},
	fetch::FetcherRepository,
	tasks::execution::preparation::ExecutableTask,
	yaml_err::contextualize_yaml_err,
};

use color_eyre::{
	eyre::{eyre, WrapErr},
	section::help::Help,
	Result,
};
use crossbeam_channel::Sender;
use std::{
	collections::{HashMap, HashSet},
	fmt::{Debug, Formatter},
	hash::{BuildHasher, Hasher},
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc, RwLock},
};
use tracing::{debug, error};
use twox_hash::{RandomXxHashBuilder64, XxHash64};

/// Describes the compatibility status of a particular Executor.
#[derive(Debug, PartialEq)]
pub enum CompatibilityStatus {
	/// Represents something that is compatible right now.
	Compatible,
	/// Represents something that could be compatible with the system under
	/// certain circumstances. For example they just need to install something.
	CouldBeCompatible(String),
	/// Represents something that could never be compatible with the current
	/// system. For example using a linux only virtulization technique.
	CannotBeCompatible(Option<String>),
}

/// Describes an "executor", or something that is capable of executing a task.
#[async_trait::async_trait]
pub trait Executor {
	/// Whether or not the executor meets the requirements needed by a particular
	/// task.
	///
	/// `reqs`: The requirements you need to fit.
	#[must_use]
	fn meets_requirements(&self, reqs: &[NeedsRequirement]) -> bool;

	/// Execute a task.
	///
	/// `log_channel`: The channel to send log updates over.
	/// `should_stop`: a helper that tells us when we need to forcibly stop.
	/// `helper_src_line`: the line that sources in all helper scripts.
	/// `task`: The actual task to execute.
	/// `worker_count`: The count of the worker we're placed on.
	#[must_use]
	async fn execute(
		&self,
		log_channel: Sender<(String, String, bool)>,
		should_stop: Arc<AtomicBool>,
		helper_src_line: &str,
		task: &ExecutableTask,
		worker_count: usize,
	) -> isize;
}

pub mod docker;
pub mod host;

/// Describes a "repository" of executors, or more accurately a set of all
/// the executors that could potentially run, or are running right now.
///
/// It is the sole thing that decides which executors should be running, when,
/// and how they should cleanup.
pub struct ExecutorRepository {
	/// Defines the executors that are currently running.
	active_executors: RwLock<HashSet<String>>,
	/// Defines the repository of executors that are compatible with this system.
	repo: RwLock<HashMap<String, Arc<dyn Executor + Sync + Send>>>,
	/// The root project directory.
	root_dir: PathBuf,
}

impl Debug for ExecutorRepository {
	fn fmt(&self, formatter: &mut Formatter) -> Result<(), std::fmt::Error> {
		let mut keys_str = String::new();
		{
			if let Ok(map) = self.repo.read() {
				for key in (*map).keys() {
					keys_str += key;
					keys_str += ",";
				}
			}
		}

		let mut active_str = String::new();
		{
			if let Ok(set) = self.active_executors.read() {
				for key in &*set {
					active_str += key;
					active_str += ",";
				}
			}
		}

		formatter.write_str(&format!(
			"ExecutorRepository executors: {}{}{} active: {}{}{} root_dir: {}{:?}{}",
			"{", keys_str, "}", "{", active_str, "}", "{", self.root_dir, "}",
		))
	}
}

impl ExecutorRepository {
	/// Create a new repository for holding executors.
	/// Ideally there should only ever be one of these in the program at a time.
	///
	/// `tlc`: The `TopLevelConfiguration`, or thing that outlines where to fetch
	///        executors from.
	/// `fr`: The `FetcherRepository`, or thing that is going to allow us to discover
	///       our executors.
	/// `rd`: The root directory for Dev-Loop
	///
	/// # Errors
	///
	/// - When there is an error fetching the executor yaml files from disk.
	/// - When the executor yaml files contain invalid yaml.
	#[allow(clippy::cognitive_complexity, clippy::map_entry)]
	pub async fn new(tlc: &TopLevelConf, fr: &FetcherRepository, rd: &PathBuf) -> Result<Self> {
		// Keep track of any executors we can construct outside of a custom_executor
		// for a task. Which will be constructed when the task is run.
		let mut executors = HashMap::new();
		// The hasher is used to assign global unique IDs.
		let hash_builder = RandomXxHashBuilder64::default();

		// First try to create the default executor.
		//
		// This isn't required to run, so even if it errors just continue, and log.
		// If someone tries to use it will fail, but if they don't it won't impact
		// their work if someone else breaks it somehow.
		if let Some(econf) = tlc.get_default_executor() {
			match Self::instantiate_executor(rd, econf)
				.await
				.wrap_err(
					"Error attempting to instantiate `default_executor` defined in `.dl/config.yml`",
				)
				.note("Will not choose this executor.")
			{
				Ok((_, executor)) => {
					// The default executor will never conflict because nothing else is in the map.
					// As such we don't need to check for colissions.
					executors.insert("default".to_owned(), executor);
					debug!("Inserted 'default' executor.");
				}
				Err(err) => error!("{:?}", err),
			}
		}

		if let Some(executor_locations) = tlc.get_executor_locations() {
			for (eloc_idx, exec_location) in executor_locations.iter().enumerate() {
				// Go fetch all the executors that we can.
				// If search in folders look for: `dl-executors.yml`.
				let resulting_fetched_executors = fr
					.fetch_filter(exec_location, Some("dl-executors.yml".to_owned()))
					.await
					.wrap_err(format!("Error while grabbing location specified at `.dl/config.yml:executor_locations:{}`", eloc_idx));

				// For HTTP errors we're going to try to continue, if your FS fails
				// well than something really bad is going on, and further FS
				// operations are most likely to fail, so just fail fast.
				if let Err(err) = resulting_fetched_executors {
					if exec_location.get_type() == &LocationType::HTTP {
						error!("{:?}", err);
						error!("Trying to continue, incase the failing remote endpoint doesn't matter for this run.");
						continue;
					} else {
						return Err(err.wrap_err(format!(
							"Failed to read the file: [{}] from the filesystem.",
							exec_location.get_at(),
						)));
					}
				}

				let fetched_executors = resulting_fetched_executors.unwrap();
				for exec_conf_file in fetched_executors {
					let exec_yaml_res =
						serde_yaml::from_slice::<ExecutorConfFile>(&exec_conf_file.get_contents());
					if let Err(exec_err) = exec_yaml_res {
						return contextualize_yaml_err(
							Err(exec_err),
							exec_conf_file.get_source(),
							&String::from_utf8_lossy(exec_conf_file.get_contents()).to_string()
						).wrap_err("Failed to parse executor file as yaml")
						 .note("Full types, and supported values are documented at: https://dev-loop.kungfury.io/docs/schemas/executor-conf-file");
					}
					let exec_yaml = exec_yaml_res.unwrap();

					for (idx, econf) in exec_yaml
						.consume_and_get_executors()
						.into_iter()
						.enumerate()
					{
						let exec_res = Self::instantiate_executor(rd, &econf).await;
						if let Err(exec_init_err) = exec_res {
							error!(
								"Failed to initialize executor #{} from: [{}] due to: {:?}. Will not be choosing.",
								idx + 1,
								exec_conf_file.get_source(),
								exec_init_err,
							);
							continue;
						}

						let (mut potential_id, executor) = exec_res.unwrap();
						if &potential_id == "host" {
							if !executors.contains_key(&potential_id) {
								debug!("Inserting host executor!");
								executors.insert(potential_id, executor);
							}
							continue;
						}
						while executors.contains_key(&potential_id) {
							potential_id =
								Self::hash_string(&potential_id, hash_builder.build_hasher());
						}
						debug!(
							"Executor #{} at location: [{}] has been assigned ID: [{}]",
							idx + 1,
							exec_conf_file.get_source(),
							potential_id
						);
						executors.insert(potential_id, executor);
					}
				}
			}
		}

		Ok(Self {
			active_executors: RwLock::new(HashSet::new()),
			repo: RwLock::new(executors),
			root_dir: rd.clone(),
		})
	}

	/// Perform selection of a particular executor for a task.
	///
	/// `task`: The actual task configuration.
	#[allow(clippy::cognitive_complexity)]
	pub async fn select_executor(
		&mut self,
		task: &TaskConf,
	) -> Option<Arc<dyn Executor + Sync + Send>> {
		// First we need to grab write locks on the repo + active executors.
		//
		// If a task specifies a custom executor we need to insert it into the repository.
		// Then we need to keep track of who's running for reuse selection first.
		let repo = self.repo.write();
		if let Err(repo_err) = repo {
			error!(
				"Internal Error, please report as an issue. Maintainer Info: [repo_write_mutex_failure: {:?}]",
				repo_err,
			);
			return None;
		}
		let mut repo = repo.unwrap();
		let active_executors = self.active_executors.write();
		if let Err(ae_err) = active_executors {
			error!(
				"Internal Error, please report as an issue. Maintainer Info: [active_executors_mutex_failure: {:?}]",
				ae_err,
			);
			return None;
		}
		let mut active_executors = active_executors.unwrap();

		// How dev-loop chooses an executor (precedence):
		//
		// 1. If a custom_executor is specified, use that.
		// 2. Next try to select an existing executor based off of the execution_needs
		//    field. This checks "active" executors first (even custom ones!), and then
		//    falls back to checking each in the repository.
		//    If none are matched error.
		// 3. Finally fallback to the default executor if one exists.

		// This is where we'll store the executor we select, and the hash builder
		// incase a custom executor is specified.
		let hash_builder = RandomXxHashBuilder64::default();

		// If a user has specified a custom executor.
		// This must be used.
		if let Some(custom_executor_config) = task.get_custom_executor() {
			debug!(
				"Task: [{}] has specified custom executor... using",
				task.get_name()
			);
			let resulting_executor =
				Self::instantiate_executor(&self.root_dir, custom_executor_config).await;

			if let Err(resulting_err) = resulting_executor {
				error!(
					"Failed to construct custom executor for task: [{}] defined in: [{}] due to: {:?}",
					task.get_name(),
					task.get_source_path(),
					resulting_err,
				);
				return None;
			}

			let (mut potential_id, executor) = resulting_executor.unwrap();
			if potential_id == "host" {
				if !repo.contains_key(&potential_id) {
					repo.insert(potential_id.clone(), executor);
				}
			} else {
				while repo.contains_key(&potential_id) {
					potential_id = Self::hash_string(&potential_id, hash_builder.build_hasher());
				}
				repo.insert(potential_id.clone(), executor);
			}
			debug!(
				"Custom Executor for task: [{}] generated id: [{}]",
				task.get_name(),
				potential_id
			);

			// Ensure the Host Executor doesn't get inserted multiple times.
			//
			// Everything else is guaranteed to be unique.
			if !active_executors.contains(&potential_id) {
				active_executors.insert(potential_id.clone());
			}
			return Some(repo.get(&potential_id).unwrap().clone());
		}

		if let Some(needs) = task.get_execution_needs() {
			debug!(
				"Task: [{}] has specified environment needs. Using those.",
				task.get_name(),
			);

			// Check active executors first
			for id in &*active_executors {
				let executor = repo.get(id).unwrap();
				if executor.meets_requirements(needs) {
					debug!(
						"Task: [{}] has it's requirements met by already active executor: [{}]",
						task.get_name(),
						id,
					);
					return Some(executor.clone());
				}
			}

			// Check all again, yes this means we will check some twice.
			for (id, exec) in &*repo {
				if exec.meets_requirements(needs) {
					debug!(
						"Task: [{}] has it's requirements met by executor: [{}]",
						task.get_name(),
						id,
					);
					return Some(exec.clone());
				}
			}

			// TODO(cynthia): report on execution needs specifically that don't match.
			// TODO(cynthia): check for typos in matching statements too.
			error!(
				"Cannot find a way to run: [{}] please check the `execution_needs` fields for the task in: [{}]",
				task.get_name(),
				task.get_source_path(),
			);
			return None;
		}

		// Finally fallback to the default executor.
		if repo.contains_key("default") {
			debug!("Selecting Default executor for task: [{}]", task.get_name());
			Some(repo.get("default").unwrap().clone())
		} else {
			error!(
				"Cannot find a way to run: [{}] defined in: [{}], did not specify a `custom_executor`/`execution_needs`, and a valid default executor has not been defined.",
				task.get_name(),
				task.get_source_path(),
			);
			None
		}
	}

	// Hash a particular string with an XxHash instance.
	fn hash_string(to_hash: &str, mut hasher: XxHash64) -> String {
		hasher.write(to_hash.as_bytes());
		format!("{}", hasher.finish())
	}

	/// "Create" a new executor.
	///
	/// This takes in a particular configuration for an executor, and tries to construct an actual executor
	/// for this. This is also what is responsible for generating the ID for a particular executor, however
	/// it does not log it since it can change. It is possible for the repository to modify the ID so it is
	/// globally unique.
	///
	/// We could just give executors an `id:` field and make a user fill it out, but it moves people away from
	/// what we want them to be doing which is just outlining their requirements, and letting us choose. Because
	/// this gives us the ability to reuse an executor that meets the requirements. Less Executors is better
	/// overall. If we gave them IDs, an image which may provide all the needed tools may fail to be selected
	/// because someone said: "I want the executor with this ID".
	///
	/// `rd`: The root directory for dev-loop.
	/// `conf`: The configuration for the executor to create.
	/// `hasher`: used to create unique ids.
	async fn instantiate_executor(
		rd: &PathBuf,
		conf: &ExecutorConf,
	) -> Result<(String, Arc<dyn Executor + Send + Sync>)> {
		// Help the type checker out.
		let ret_v: Result<(String, Arc<dyn Executor + Send + Sync>)> = match *conf.get_type() {
			ExecutorType::Host => {
				let compatibility = host::HostExecutor::is_compatible();
				match compatibility {
					CompatibilityStatus::Compatible => {}
					CompatibilityStatus::CouldBeCompatible(how_to_install) => {
						return Err(eyre!(
							"The host executor is not currently compatible with this system. To get it compatible you should: {}",
							how_to_install,
						));
					}
					CompatibilityStatus::CannotBeCompatible(potential_help) => {
						return Err(eyre!(
							"The host executor could never be compatible with this system. The help text is provided: {:?}",
							potential_help,
						));
					}
				}
				let he = host::HostExecutor::new(rd)?;
				Ok(("host".to_owned(), Arc::new(he)))
			}
			ExecutorType::Docker => {
				let compatibility = docker::DockerExecutor::is_compatible().await;
				match compatibility {
					CompatibilityStatus::Compatible => {}
					CompatibilityStatus::CouldBeCompatible(how_to_install) => {
						return Err(eyre!(
							"The docker executor is not currently compatible with this system. To get it compatible you should: {}",
							how_to_install,
						));
					}
					CompatibilityStatus::CannotBeCompatible(potential_help) => {
						return Err(eyre!(
							"The docker executor could never be compatible with this system. The help text is provided: {:?}",
							potential_help,
						));
					}
				}

				let params = conf.get_parameters();
				let provides = conf.get_provided();
				let de = docker::DockerExecutor::new(rd, &params, &provides, None)?;
				Ok((de.get_container_name().to_owned(), Arc::new(de)))
			}
		};

		ret_v
	}
}
