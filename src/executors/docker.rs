use crate::{
	config::types::{NeedsRequirement, ProvideConf},
	dirs::{get_tmp_dir, rewrite_tmp_dir},
	executors::{
		docker_engine::{
			delete_container, delete_network, docker_version_check, ensure_docker_container,
			ensure_network_attached, ensure_network_exists, execute_command_in_container_async,
			get_command_exit_code, has_command_finished, list_devloop_containers,
			list_devloop_networks, DockerContainerInfo, SOCKET_PATH,
		},
		shared::{create_entrypoint, create_executor_shared_dir, create_log_proxy_files},
		CompatibilityStatus, Executor as ExecutorTrait,
	},
	tasks::execution::preparation::ExecutableTask,
};

use color_eyre::{
	eyre::{eyre, WrapErr},
	Report, Result, Section,
};
use crossbeam_channel::Sender;
use isahc::{
	config::{Dialer, VersionNegotiation}, prelude::*, Error as HttpError, HttpClient, HttpClientBuilder,
};
use semver::{Version, VersionReq};
use std::{
	collections::HashMap,
	fs::File,
	io::{prelude::*, BufReader},
	path::PathBuf,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tracing::{debug, error, info, warn};

/// Represents the actual `Executor` for docker, responsible for maintaining
/// the lifecycle of a single docker container.
#[derive(Debug)]
pub struct Executor {
	/// The HTTPClient used to talking to the docker socket.
	client: HttpClient,
	/// The root of the project on the host filesystem represented as a string.
	project_root: String,
	/// The list of things this provides.
	provides: HashMap<String, Option<Version>>,
	/// A random string used for various unique identifiers.
	random_str: String,
	/// Represents the docker container api.
	container: DockerContainerInfo,
	/// The temporary directory.
	tmp_dir: String,
}

impl Executor {
	/// Create a new Docker Executor.
	///
	/// `project_root`: the root of the project directory.
	/// `executor_args`: the arguments for this particular executor.
	/// `provided_conf`: the list of services this docker executor providers.
	/// `override_sock_path`: override the socket path, this should only ever
	///                       be used by tests, or where a user has explicitly
	///                       requested a specific socket. We automatically
	///                       want to use the socket path as much as possible.
	///
	/// # Errors
	///
	/// The docker executor must have all of it's required arguments passed in
	/// otherwise it will error on construction.
	pub fn new(
		project_root: &PathBuf,
		executor_args: &HashMap<String, String>,
		provided_conf: &[ProvideConf],
		override_sock_path: Option<String>,
	) -> Result<Self> {
		let pr_as_string = project_root.to_str();
		if pr_as_string.is_none() {
			return Err(eyre!(
				"Failed to turn the project directory: [{:?}] into a utf8-string.",
				project_root,
			))
			.suggestion(
				"Please move the project directory to somewhere that is a UTF-8 only file path.",
			);
		}
		let pr_as_string = pr_as_string.unwrap();

		// Next Generate the random name for the container to use that won't clash.
		let random_str = format!("{}", uuid::Uuid::new_v4());

		let mut provides = HashMap::new();
		for provided in provided_conf {
			let version_opt = if provided.get_version().is_empty() {
				None
			} else if let Ok(vs) = Version::parse(provided.get_version()) {
				Some(vs)
			} else {
				None
			};

			provides.insert(provided.get_name().to_owned(), version_opt);
		}

		let client = if cfg!(target_os = "windows") {
			// TODO(xxx): set windows named pipe/url
			HttpClientBuilder::new()
				.version_negotiation(VersionNegotiation::http11())
				.build()
		} else {
			HttpClientBuilder::new()
				.dial(
					override_sock_path.unwrap_or_else(|| SOCKET_PATH.to_owned()).parse::<Dialer>()?
				)
				.version_negotiation(VersionNegotiation::http11())
				.build()
		}?;

		let container = DockerContainerInfo::new(executor_args, pr_as_string, &random_str)?;

		Ok(Self {
			client,
			project_root: pr_as_string.to_owned(),
			provides,
			random_str,
			container,
			tmp_dir: get_tmp_dir().to_string_lossy().to_string(),
		})
	}

	/// Attempt to clean up all resources left behind by the docker executor.
	///
	/// # Errors
	///
	/// - when there is an issue talking to the docker api for containers.
	pub async fn clean() -> Result<()> {
		// Cleanup all things left behind by the docker executor.
		if Self::is_compatible().await != CompatibilityStatus::Compatible {
			info!("Docker is not listening on this host, won't clean!");
			return Ok(());
		}

		let client = if cfg!(target_os = "windows") {
			HttpClientBuilder::new()
				.version_negotiation(VersionNegotiation::http11())
				.build()
		} else {
			HttpClientBuilder::new()
				.dial(SOCKET_PATH.parse::<Dialer>()?)
				.version_negotiation(VersionNegotiation::http11())
				.build()
		}
		.wrap_err("Failed to construct HTTP-Client to talk to Docker")?;

		for container in list_devloop_containers(&client).await.wrap_err("Failed to list containers").note("Will not clean up docker containers due to this error.").suggestion("To manually clean up containers use `docker ps -a` to list containers, and `docker kill ${container name that starts with `dl-`}`")? {
			debug!("Found dev-loop container: [{}]", container);
			delete_container(&client, &container).await;
		}

		for network in list_devloop_networks(&client)
			.await
			.wrap_err("Failed to list networks")
			.note("Will not delete docker networks due to this error.")?
		{
			debug!("Found dev-loop network: [{}]", network);
			delete_network(&client, &network).await;
		}

		// Done! \o/
		Ok(())
	}

	/// Determines if this `Executor` is compatible with the system.
	pub async fn is_compatible() -> CompatibilityStatus {
		let client = if cfg!(target_os = "windows") {
			HttpClientBuilder::new()
				.version_negotiation(VersionNegotiation::http11())
				.build()
		} else {
			let as_dialer = SOCKET_PATH.parse::<Dialer>();
			if as_dialer.is_err() {
				return CompatibilityStatus::CannotBeCompatible(Some(format!(
					"{:?}",
					Err::<(), isahc::config::DialerParseError>(as_dialer.unwrap_err())
						.wrap_err("Internal Exception: Failed to construct HTTP Client")
						.suggestion("This is an internal error, please file an issue.")
						.unwrap_err(),
				)));
			}

			HttpClientBuilder::new()
				.dial(as_dialer.unwrap())
				.version_negotiation(VersionNegotiation::http11())
				.build()
		};
		if let Err(client_err) = client {
			return CompatibilityStatus::CannotBeCompatible(Some(format!(
				"{:?}",
				Err::<(), HttpError>(client_err)
					.wrap_err("Internal Exception: Failed to construct HTTP Client")
					.suggestion("This is an internal error, please file an issue.")
					.unwrap_err(),
			)));
		}
		let client = client.unwrap();

		match docker_version_check(&client).await {
			Ok(data) => {
				if data.get("Version").is_some() {
					CompatibilityStatus::Compatible
				} else {
					debug!("Failed to get key: `Version` from docker executor api!");
					CompatibilityStatus::CouldBeCompatible("install docker".to_owned())
				}
			}
			Err(http_err) => {
				let formatted_err = Err::<(), Report>(http_err)
					.note("Failed to reach out to docker socket.")
					.unwrap_err();
				debug!("{:?}", formatted_err,);
				CompatibilityStatus::CouldBeCompatible("install docker".to_owned())
			}
		}
	}

	pub fn get_container_name(&self) -> &str {
		self.container.get_container_name()
	}

	fn read_buf_until_end(
		reader: &mut BufReader<File>,
		channel_name: &str,
		flush_channel_clone: &Sender<(String, String, bool)>,
		is_stderr: bool,
	) {
		let mut line = String::new();
		while let Ok(read) = reader.read_line(&mut line) {
			if read == 0 {
				break;
			}

			let _ = flush_channel_clone.send((channel_name.to_owned(), line, is_stderr));
			line = String::new();
		}
	}
}

#[async_trait::async_trait]
impl ExecutorTrait for Executor {
	#[must_use]
	fn meets_requirements(&self, reqs: &[NeedsRequirement]) -> bool {
		let mut met = true;

		for req in reqs {
			if !self.provides.contains_key(req.get_name()) {
				met = false;
				break;
			}

			if let Some(matcher) = req.get_version_matcher() {
				if let Ok(version_req) = VersionReq::parse(matcher) {
					if let Some(version) = self.provides.get(req.get_name()).unwrap() {
						if !version_req.matches(version) {
							met = false;
							break;
						}
					} else {
						met = false;
					}
				} else {
					continue;
				}
			}
		}

		met
	}

	#[must_use]
	async fn execute(
		&self,
		log_channel: Sender<(String, String, bool)>,
		should_stop: Arc<AtomicBool>,
		helper_src_line: &str,
		task: &ExecutableTask,
		worker_count: usize,
	) -> Result<i32> {
		ensure_network_exists(&self.client, task.get_pipeline_id()).await?;
		ensure_docker_container(
			&self.client,
			&self.project_root,
			&self.tmp_dir,
			&self.container,
		)
		.await?;
		ensure_network_attached(
			&self.client,
			self.container.get_container_name(),
			self.container.get_hostname(),
			task.get_pipeline_id(),
		)
		.await?;

		let shared_dir = create_executor_shared_dir(task.get_pipeline_id())?;

		let (stdout_host_log_path, stderr_host_log_path) =
			create_log_proxy_files(&shared_dir, task)?;
		let stdout_path_in_docker = rewrite_tmp_dir(&self.tmp_dir, &stdout_host_log_path);
		let stderr_path_in_docker = rewrite_tmp_dir(&self.tmp_dir, &stderr_host_log_path);

		let entrypoint = create_entrypoint(
			"/mnt/dl-root",
			&self.tmp_dir,
			shared_dir,
			helper_src_line,
			task,
			true,
			Some(stdout_path_in_docker),
			Some(stderr_path_in_docker),
		)?;
		let entrypoint_as_str = entrypoint.to_string_lossy().to_string();
		let exec_id = execute_command_in_container_async(
			&self.client,
			self.container.get_container_name(),
			&[entrypoint_as_str],
			self.container.get_environment_to_export(),
			self.container.get_base_user(),
			true,
			self.container.get_cloned_proxy_user_id(),
			self.container.get_cloned_proxy_group_id(),
		)
		.await
		.wrap_err("Failed to execute script inside docker container.")?;

		let has_finished = Arc::new(AtomicBool::new(false));
		let flush_channel_clone = log_channel.clone();
		let flush_task_name = task.get_task_name().to_owned();
		let flush_is_finished_clone = has_finished.clone();

		let flush_task = async_std::task::spawn(async move {
			let file = File::open(stdout_host_log_path)
				.expect("Failed to open log file even though we created it!");
			let err_file = File::open(stderr_host_log_path)
				.expect("Failed to open stderr log file even though we created it!");
			let mut reader = BufReader::new(file);
			let mut stderr_reader = BufReader::new(err_file);
			let channel_name = format!("{}-{}", worker_count, flush_task_name);

			while !flush_is_finished_clone.load(Ordering::Relaxed) {
				Self::read_buf_until_end(&mut reader, &channel_name, &flush_channel_clone, false);
				Self::read_buf_until_end(
					&mut stderr_reader,
					&channel_name,
					&flush_channel_clone,
					true,
				);

				async_std::task::sleep(Duration::from_millis(10)).await;
			}
		});

		let rc: i32;

		loop {
			if has_command_finished(&self.client, &exec_id).await {
				let rc_res = get_command_exit_code(&self.client, &exec_id)
					.await
					.wrap_err("Failed to check if your task has finished.");
				if let Err(rc_err) = rc_res {
					error!("{:?}", rc_err);
					rc = 10;
					break;
				}
				rc = rc_res.unwrap();
				break;
			}

			// Have we been requested to stop?
			if should_stop.load(Ordering::Acquire) {
				if task.ctrlc_is_failure() {
					error!("Docker Executor was told to terminate as failure!");
					rc = 10;
				} else {
					warn!("Docker Executor was told to terminate! Stopping!");
					rc = 0;
				}
				break;
			}

			async_std::task::sleep(Duration::from_millis(10)).await;
		}

		has_finished.store(true, Ordering::Release);
		flush_task.await;

		Ok(rc)
	}
}

#[cfg(test)]
mod unit_tests {
	use super::*;

	#[test]
	fn creation_errors() {
		{
			let args = HashMap::new();
			let provided_conf = Vec::new();
			let pb = PathBuf::from("/tmp/non-existant");

			assert!(
				Executor::new(&pb, &args, &provided_conf, None,).is_err(),
				"Docker Executor without a name_prefix should error.",
			);
		}

		{
			let mut args = HashMap::new();
			args.insert("name_prefix".to_owned(), "asdf-".to_owned());
			let provided_conf = Vec::new();
			let pb = PathBuf::from("/tmp/non-existant");

			assert!(
				Executor::new(&pb, &args, &provided_conf, None,).is_err(),
				"Docker executor without an image should error.",
			);
		}

		{
			let mut args = HashMap::new();
			args.insert("name_prefix".to_owned(), "asdf-".to_owned());
			args.insert("image".to_owned(), "localhost:5000/blah:latest".to_owned());
			let provided_conf = Vec::new();
			let pb = PathBuf::from("/tmp/non-existant");

			assert!(
				Executor::new(&pb, &args, &provided_conf, None,).is_ok(),
				"Docker executor with an image/name prefix should succeed!",
			);
		}
	}

	#[test]
	fn meets_requirements() {
		let mut args = HashMap::new();
		args.insert("name_prefix".to_owned(), "name-prefix-".to_owned());
		args.insert("image".to_owned(), "localhost:5000/blah:latest".to_owned());
		let mut provided_conf = Vec::new();
		provided_conf.push(crate::config::types::ProvideConf::new(
			"a-really-random-service".to_owned(),
			Some("1.0.0".to_owned()),
		));
		let pb = PathBuf::from("/tmp/non-existant");

		let de = Executor::new(&pb, &args, &provided_conf, None)
			.expect("Docker Executor in meets_requirements should be able to be constructed");

		assert!(
			de.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"a-really-random-service".to_owned(),
				None
			)])
		);
		assert!(
			!de.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"blah".to_owned(),
				None
			)])
		);
		assert!(
			!de.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"a-really-random-service".to_owned(),
				Some("> 1.0.0".to_owned())
			)])
		);
		assert!(
			de.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"a-really-random-service".to_owned(),
				Some(">= 1.0.0".to_owned())
			)])
		);
	}

	// TODO(xxx): mock the rest of the calls.
}
