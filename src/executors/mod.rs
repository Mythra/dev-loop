//! Contains all the codes necessary for "Executors". Executors are what are
//! responsible for "abstract"'ing away the system level resources, and
//! managing the the lifecycle of anything it creates. For example if using
//! a docker executor the code that spins up/down the container will be here.

use crate::config::types::{
	ExecutorConf, ExecutorConfFile, NeedsRequirement, TaskConf, TopLevelConf,
};
use crate::fetch::{Fetcher, FetcherRepository};
use crate::tasks::execution::preparation::ExecutableTask;
use anyhow::{anyhow, Result};
use async_std::path::PathBuf;
use crossbeam_channel::Sender;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use tracing::{error, info};
use twox_hash::RandomXxHashBuilder64;
use twox_hash::XxHash64;

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
	/// `log_channel_err`: The channel to send log updates over for STDERR.
	/// `should_stop`: a helper that tells us when we need to forcibly stop.
	/// `helper_src_line`: the line that sources in all helper scripts.
	/// `task`: The actual task to execute.
	#[must_use]
	async fn execute(
		&self,
		log_channel: Sender<(String, String, bool)>,
		should_stop: Arc<AtomicBool>,
		helper_src_line: &str,
		task: &ExecutableTask,
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
	/// The ID of the default executor.
	default_executor_id: String,
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
			"ExecutorRepository default id: {}{}{} executors: {}{}{} active: {}{}{} root_dir: {}{:?}{}",
			"{", self.default_executor_id, "}", "{", keys_str, "}", "{", active_str, "}",
			"{", self.root_dir, "}",
		))
	}
}

impl ExecutorRepository {
	/// Create a new repository for holding executors.
	/// Ideally there should only ever be one of these in the program at a time.
	///
	/// `tlc`: The "TopLevelConfiguration", or thing that outlines where to fetch
	///        executors from.
	/// `fr`: The "FetcherRepository", or thing that is going to allow us to discover
	///       our executors.
	/// `rd`: The root directory for Dev-Loop
	#[allow(clippy::cognitive_complexity, clippy::map_entry)]
	#[must_use]
	#[tracing::instrument]
	pub async fn new(tlc: &TopLevelConf, fr: &FetcherRepository, rd: &PathBuf) -> Result<Self> {
		// Keep track of any executors we can construct outside of a custom_executor
		// for a task. Which will be constructed when the task is run.
		let mut executors = HashMap::new();
		let mut default_executor_id = String::new();
		// The hasher is used to assign global unique IDs.
		let hash_builder = RandomXxHashBuilder64::default();

		// First try to create the default executor.
		//
		// This isn't required to run, so even if it errors just continue, and log.
		// If someone tries to use it will fail, but if they don't it won't impact
		// their work if someone else breaks it somehow.
		if let Some(econf) = tlc.get_default_executor() {
			let exec_res = Self::instantiate_executor(rd, econf, hash_builder.build_hasher()).await;
			if let Err(err_exec) = exec_res {
				error!(
					"Failed to construct default executor due to: [{:?}]. Will not be choosing.",
					err_exec,
				);
			} else {
				let (id, exec) = exec_res.unwrap();
				// The default executor will never conflict because nothing else is in the map.
				//
				// As such do not check for "does this id collide with another?"
				info!(
					"Default Executor has been assigned ID: [{}] along with [default]",
					id
				);
				default_executor_id = id.clone();
				executors.insert(id, exec);
			}
		}

		if let Some(executor_locations) = tlc.get_executor_locations() {
			for exec_location in executor_locations {
				// Go fetch all the executors that we can.
				// If search in folders look for: `dl-executors.yml`.
				let resulting_fetched_executors = fr
					.fetch_filter(exec_location, Some("dl-executors.yml".to_owned()))
					.await;

				// For HTTP errors we're going to try to continue, if your FS fails
				// well than something really bad is going on that we don't want to handle.
				if let Err(err) = resulting_fetched_executors {
					if exec_location.get_type() == "http" {
						error!("Failed to fetch from an HTTP Endpoint. Trying to continue anyway, incase you're running a task that is fully local with no internet...");
						continue;
					} else {
						error!("Location type failed to fetch a filesystem component! Assuming this is a critical error.");
						return Err(anyhow!(
							"Failed to fetch executors from FS: [{:?}] Internal Err: [{:?}]",
							exec_location,
							err,
						));
					}
				}

				let fetched_executors = resulting_fetched_executors.unwrap();
				for exec_conf_file in fetched_executors {
					let exec_yaml_res =
						serde_yaml::from_slice::<ExecutorConfFile>(&exec_conf_file.get_contents());
					if let Err(exec_err) = exec_yaml_res {
						return Err(anyhow!(
							"Failed to parse executor file as yaml, path: [{:?}], err: [{:?}]",
							exec_conf_file.get_source(),
							exec_err,
						));
					}
					let exec_yaml = exec_yaml_res.unwrap();

					for (idx, econf) in exec_yaml
						.consume_and_get_executors()
						.into_iter()
						.enumerate()
					{
						let exec_res =
							Self::instantiate_executor(rd, &econf, hash_builder.build_hasher())
								.await;
						if let Err(exec_init_err) = exec_res {
							error!(
								"Failed to initialize executor #{} in location: [{}] due to: [{:?}] Will not be choosing.",
								idx + 1,
								exec_conf_file.get_source(),
								exec_init_err,
							);
							continue;
						}

						let (mut potential_id, executor) = exec_res.unwrap();
						if &potential_id == "host" {
							if !executors.contains_key(&potential_id) {
								info!("Inserting host executor!");
								executors.insert(potential_id, executor);
							}
							continue;
						}
						while executors.contains_key(&potential_id) {
							potential_id =
								Self::hash_string(&potential_id, hash_builder.build_hasher());
						}
						info!(
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
			default_executor_id,
			repo: RwLock::new(executors),
			root_dir: rd.clone(),
		})
	}

	/// Perform selection of a particular executor for a task.
	///
	/// `task`: The actual task configuration.
	#[allow(clippy::cognitive_complexity)]
	#[tracing::instrument]
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
				"Failed to acquire repo write lock in select_executor: [{:?}]",
				repo_err
			);
			return None;
		}
		let mut repo = repo.unwrap();
		let active_executors = self.active_executors.write();
		if let Err(ae_err) = active_executors {
			error!(
				"Failed to acquire active_executors write lock in select_executor: [{:?}]",
				ae_err
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
			info!(
				"Task: [{}] has specified custom executor... using",
				task.get_name()
			);
			let resulting_executor = Self::instantiate_executor(
				&self.root_dir,
				custom_executor_config,
				hash_builder.build_hasher(),
			)
			.await;

			if let Err(resulting_err) = resulting_executor {
				error!(
					"Failed to construct custom executor for task: [{}] due to: [{:?}]",
					task.get_name(),
					resulting_err
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
			info!(
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
			info!(
				"Task: [{}] has specified environment needs. Using those.",
				task.get_name()
			);

			// Check active executors first
			for id in &*active_executors {
				let executor = repo.get(id).unwrap();
				if executor.meets_requirements(needs) {
					info!(
						"Task: [{}] has it's requirements met by already active executor: [{}]",
						task.get_name(),
						id
					);
					return Some(executor.clone());
				}
			}

			// Check all again, yes this means we will check some twice.
			for (id, exec) in &*repo {
				if exec.meets_requirements(needs) {
					info!(
						"Task: [{}] has it's requirements met by executor: [{}]",
						task.get_name(),
						id
					);
					return Some(exec.clone());
				}
			}

			error!(
				"Failed to find an executor that provides what task: [{}] has speficied in it's needs",
				task.get_name()
			);
			return None;
		}

		// Finally fallback to the default executor.
		if self.default_executor_id.is_empty() {
			error!(
				"Task: [{}] did not specify needs requirement, or custom executor, and no default executor has been defined!",
				task.get_name()
			);
			None
		} else {
			info!("Selecting Default executor for task: [{}]", task.get_name());
			Some(repo.get(&self.default_executor_id).unwrap().clone())
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
	#[tracing::instrument]
	async fn instantiate_executor(
		rd: &PathBuf,
		conf: &ExecutorConf,
		hasher: XxHash64,
	) -> Result<(String, Arc<dyn Executor + Send + Sync>)> {
		// Help the type checker out.
		let ret_v: Result<(String, Arc<dyn Executor + Send + Sync>)> = match conf.get_type() {
			"host" => {
				let compatibility = host::HostExecutor::is_compatible().await;
				match compatibility {
					CompatibilityStatus::Compatible => {}
					CompatibilityStatus::CouldBeCompatible(how_to_install) => {
						return Err(anyhow!(
							"The host executor is not currently compatible with this system. To get it compatible you should: {}",
							how_to_install,
						));
					}
					CompatibilityStatus::CannotBeCompatible(potential_help) => {
						return Err(anyhow!(
							"The host executor could never be compatible with this system. The help text is provided: [{:?}]",
							potential_help,
						));
					}
				}
				let he = host::HostExecutor::new(rd.clone()).await?;
				Ok(("host".to_owned(), Arc::new(he)))
			}
			"docker" => {
				let compatibility = docker::DockerExecutor::is_compatible().await;
				match compatibility {
					CompatibilityStatus::Compatible => {}
					CompatibilityStatus::CouldBeCompatible(how_to_install) => {
						return Err(anyhow!(
							"The docker executor is not currently compatible with this system. To get it compatible you should: {}",
							how_to_install,
						));
					}
					CompatibilityStatus::CannotBeCompatible(potential_help) => {
						return Err(anyhow!(
							"The docker executor could never be compatible with this system. The help text is provided: [{:?}]",
							potential_help,
						));
					}
				}

				let params = conf.get_parameters();
				let provides = conf.get_provided();
				let de = docker::DockerExecutor::new(rd.clone(), &params, &provides, None).await?;
				Ok((de.get_container_name().to_owned(), Arc::new(de)))
			}
			_ => Err(anyhow!("Unknown type of executor!")),
		};

		ret_v
	}
}
