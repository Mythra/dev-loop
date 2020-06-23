//! Contains the code that represents the Docker Executor, or the executor
//! that runs your code inside of a Docker Container. Dev-Loop manages the
//! state of this container, it's network, and the like in it's entirety.
//!
//! The docker executor has many arguments to it, that control it's behaviour,
//! but they're hopefully all sensible.
//!
//! The options are configured through `params` (a map of strings passed to the
//! executor itself). The following attributes you can set are:
//!
//! `user`: the user to launch commands as in the container, defaults to root.
//!
//! `name_prefix`: the prefix of the container to use. this is required, and
//!                used to help derive the container name which follows a
//!                format like: `dl-${name_prefix}${data}`. As such your name
//!                prefix should end with: `-`.
//!
//! `image`: the docker image to use for this container. This is required. This
//!          should be a full pullable image. For example: `ubuntu:18.04`, or
//!          `gcr.io/....:latest`
//!
//! `extra_mounts`: a list of extra directories to mount for the docker executor.
//!                 it should be noted the root project directory, and $TMPDIR
//!                 will always be mounted.
//!
//! `hostname`: the hostname to use for the docker container. If you don't
//!             provide one, it will be derived automatically for you. This is
//!             almost always preferred since dev-loop will ensure there are no
//!             possible conflicts.
//!
//! `export_env`: a comma seperated list of environment variables to allow
//!               to be passed into the container.
//!
//! `tcp_ports_to_expose`: a comma seperated list of ports to export to the
//!                        host machine. you won't need to set these if you're
//!                        using two tasks in a pipeline, as each pipeline
//!                        gets it's own docker network that allows services
//!                        to natively communicate.
//!
//! `udp_ports_to_expose`: the same as `tcp_ports_to_export` just for udp instead
//!
//! If you ever find yourself with a docker container/network that's running
//! when it's not supposed to be you can use the `clean` command. The clean
//! command will automatically remove all resources associated with `dev-loop`.

use crate::{
	config::types::{NeedsRequirement, ProvideConf},
	executors::{CompatibilityStatus, Executor},
	get_tmp_dir,
	tasks::execution::preparation::ExecutableTask,
};
use anyhow::{anyhow, Result};
use async_std::future;
use crossbeam_channel::Sender;
use isahc::{config::VersionNegotiation, prelude::*, HttpClient, HttpClientBuilder};
use once_cell::sync::Lazy;
use semver::{Version, VersionReq};
use serde_json::Value as JsonValue;
use std::{
	collections::HashMap,
	io::{prelude::*, BufReader},
	path::PathBuf,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};
use tracing::{debug, error, info, warn};

/// This is the api version we use for talking to the docker socket.
///
/// The docker socket allows us to choose a versioned api like this, which is
/// why we use it as opposed to using a terminal command (not to mention we
/// don't have to worry about escaping correctly).
///
/// `v1.30` is chosen because as of the time of writing this
/// `v1.30` is the version for Docker Engine 17.06, which at
/// the time of writing this (January 7th, 2020) is the lowest supported
/// version according to docker:
///
/// <https://success.docker.com/article/compatibility-matrix>
///
/// We can bump this in the future when we know it won't run into anyone.
const DOCKER_API_VERSION: &str = "/v1.30";

cfg_if::cfg_if! {
  if #[cfg(unix)] {
		const SOCKET_PATH: &str = "/var/run/docker.sock";
  } else if #[cfg(win)] {
		// TODO(cynthia): named pipes? url?
		const SOCKET_PATH: &str = "UNIMPLEMENTED";
  }
}

// A global lock for the unix socket since it can't have multiple things communicating
// at the same time.
//
// You can techincally have multiple writers on windows but only up to a particular buff
// size, and it's just much easier to have just a global lock, and take the extra bit. Really
// the only time this is truly slow is when we're downloading a docker image.
static DOCK_SOCK_LOCK: Lazy<async_std::sync::Mutex<()>> =
	Lazy::new(|| async_std::sync::Mutex::new(()));

/// Represents the actual `DockerExecutor`, responsible for maintaining
/// the lifecycle of a single docker container.
#[derive(Debug)]
pub struct DockerExecutor {
	/// The HTTPClient used to talking to the docker socket.
	client: HttpClient,
	/// The name of the underlying container.
	container_name: String,
	/// The environment variables to export.
	environment_to_export: Vec<String>,
	/// The list of extra directories to mount.
	extra_mounts: Vec<String>,
	/// The image to use for this docker executor.
	image: String,
	/// The hostname for this container.
	hostname: String,
	/// The root of the project on the host filesystem represented as a string.
	project_root: String,
	/// The list of things this provides.
	provides: HashMap<String, Option<Version>>,
	/// A random string used for various unique identifiers.
	random_str: String,
	/// The group id to run as.
	run_as_group_id: u32,
	/// The user id to run as.
	run_as_user_id: u32,
	/// The list of ports to export (for tcp).
	tcp_ports_to_expose: Vec<u32>,
	/// The temporary directory of the host represented as a string.
	tmp_dir: String,
	/// The list of udp ports to export.
	udp_ports_to_expose: Vec<u32>,
	/// The user to launch the container as.
	user: String,
}

impl DockerExecutor {
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
	#[allow(unused_assignments, clippy::too_many_lines)]
	pub fn new(
		project_root: &PathBuf,
		executor_args: &HashMap<String, String>,
		provided_conf: &[ProvideConf],
		override_sock_path: Option<String>,
	) -> Result<Self> {
		// First get the project_root directory, and tmp_dir as strings.
		//
		// We need them as strings more than paths.

		let tmp_dir = get_tmp_dir();
		let tmp_dir_as_string = tmp_dir.to_str();
		if tmp_dir_as_string.is_none() {
			return Err(anyhow!(
				"Failed to turn temporary directory into a utf-8 string!"
			));
		}
		let pr_as_string = project_root.to_str();
		if pr_as_string.is_none() {
			return Err(anyhow!("Failed to turn project root into a utf-8 string!"));
		}
		let pr_as_string = pr_as_string.unwrap();

		// Next Generate the random name for the container to use that won't clash.
		let random_str = format!("{}", uuid::Uuid::new_v4());

		// Finally parse out all the executor arguments, including the required ones.

		let mut container_name = "dl-".to_owned();
		if let Some(user_specified_prefix) = executor_args.get("name_prefix") {
			container_name += user_specified_prefix;
		} else {
			return Err(anyhow!("Docker Executor requires a `name_prefix` field!"));
		}
		container_name += &random_str;

		let mut image = String::new();
		if let Some(image_identifier) = executor_args.get("image") {
			image = image_identifier.to_owned();
		} else {
			return Err(anyhow!(
				"Docker Executor requires an `image` to know which docker image to use."
			));
		}

		let mut group_id = 0;
		let mut user_id = 0;
		if let Some(permission_helper_active) = executor_args.get("experimental_permission_helper")
		{
			if permission_helper_active == "true" {
				user_id = users::get_effective_uid();
				group_id = users::get_effective_gid();
			}
		}

		let mut env_vars = Vec::new();
		if let Some(envs_to_export) = executor_args.get("export_env") {
			env_vars = envs_to_export
				.split(',')
				.map(|the_str| {
					std::env::var(the_str)
						.map(|val| format!("{}={}", the_str, val))
						.unwrap_or_else(|_| the_str.to_owned())
				})
				.collect::<Vec<String>>();
		}

		let mut tcp_ports_to_expose = Vec::new();
		if let Some(ports_to_expose) = executor_args.get("tcp_ports_to_expose") {
			tcp_ports_to_expose = ports_to_expose
				.split(',')
				.filter_map(|item| item.parse().ok())
				.collect::<Vec<u32>>();
		}

		let mut udp_ports_to_expose = Vec::new();
		if let Some(ports_to_expose) = executor_args.get("udp_ports_to_expose") {
			udp_ports_to_expose = ports_to_expose
				.split(',')
				.filter_map(|item| item.parse().ok())
				.collect::<Vec<u32>>();
		}

		let user = executor_args
			.get("user")
			.map_or_else(|| "root".to_owned(), String::from);

		let hostname = if let Some(hostname_ref) = executor_args.get("hostname") {
			hostname_ref.to_owned()
		} else {
			let mut string = executor_args.get("name_prefix").unwrap().to_owned();
			string.pop();
			string
		};

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

		// For mounts remember they:
		//
		//   1. Need to be relative to home (signified by starting with: `~` as a
		//      source location), or be relative to the project root.
		//   2. Each mount is in the form: `${src}:${dest}`.
		//   3. Docker will error if a mount doesn't exist, so we need to skip it if
		//      it's not on the FS.
		let mut extra_mounts = Vec::new();
		if let Some(mount_str_ref) = executor_args.get("extra_mounts") {
			extra_mounts = mount_str_ref
				.split(',')
				.filter_map(|item| {
					let mounts = item.split(':').collect::<Vec<&str>>();
					if mounts.len() != 2 {
						error!("Invalid Mount String: [{}] skipping...", item);
						return None;
					}

					let src = mounts[0];
					let dest = mounts[1];

					let src =
						if src.starts_with('~') {
							let potential_home_dir = crate::dirs::home_dir();
							if potential_home_dir.is_none() {
								error!("Failed to find home directory! Skipping Mount!");
								return None;
							}
							let home_dir = potential_home_dir.unwrap();
							let home_dir = home_dir.to_str();
							if home_dir.is_none() {
								error!("Failed to turn home directory into a utf8 string! Skipping Mount!");
								return None;
							}
							let home_dir = home_dir.unwrap();

							src.replace("~", home_dir)
						} else if src.starts_with('/') {
							src.to_owned()
						} else {
							pr_as_string.to_owned() + "/" + src
						};

					let src_as_pb = std::path::PathBuf::from(&src);
					if !src_as_pb.exists() {
						error!("Mount src: [{}] does not exist! Skipping Mount!", src);
						return None;
					}

					Some(format!("{}:{}", src, dest))
				})
				.collect::<Vec<String>>();
		}

		let client = if cfg!(target_os = "windows") {
			// TODO(xxx): set windows named pipe/url
			HttpClientBuilder::new()
				.version_negotiation(VersionNegotiation::http11())
				.build()
		} else {
			HttpClientBuilder::new()
				.unix_socket(override_sock_path.unwrap_or_else(|| SOCKET_PATH.to_owned()))
				.version_negotiation(VersionNegotiation::http11())
				.build()
		}?;

		Ok(Self {
			client,
			container_name,
			environment_to_export: env_vars,
			extra_mounts,
			image,
			hostname,
			project_root: pr_as_string.to_owned(),
			provides,
			random_str,
			run_as_group_id: group_id,
			run_as_user_id: user_id,
			tcp_ports_to_expose,
			tmp_dir: tmp_dir_as_string.unwrap().to_owned(),
			udp_ports_to_expose,
			user,
		})
	}

	/// Call the docker engine api using the GET http method.
	///
	/// `client`: the http client to use.
	/// `path`: the path to call (along with Query Args).
	/// `timeout`: The optional timeout. Defaults to 30 seconds.
	/// `is_json`: whether or not to parse the response as json.
	async fn docker_api_get(
		client: &HttpClient,
		path: &str,
		timeout: Option<std::time::Duration>,
		is_json: bool,
	) -> Result<JsonValue> {
		let _guard = DOCK_SOCK_LOCK.lock().await;

		let timeout_frd = timeout.unwrap_or_else(|| std::time::Duration::from_millis(30000));
		let url = format!("http://localhost{}{}", DOCKER_API_VERSION, path);
		debug!("URL for get will be: {}", url);
		let req = Request::get(url)
			.header("Accept", "application/json; charset=UTF-8")
			.header("Content-Type", "application/json; charset=UTF-8")
			.body(())?;
		let resp_fut_res = future::timeout(timeout_frd, client.send_async(req)).await?;
		if let Err(resp_fut_err) = resp_fut_res {
			return Err(anyhow!(
				"Failed to send docker API Request due to: [{:?}]",
				resp_fut_err,
			));
		}
		let mut resp = resp_fut_res.unwrap();

		let status = resp.status().as_u16();
		if status < 200 || status > 299 {
			return Err(anyhow!(
				"Docker responded with an invalid status code: [{}] to path: [{}]",
				resp.status().as_u16(),
				path
			));
		}

		if is_json {
			match resp.json() {
				Ok(json_value) => Ok(json_value),
				Err(json_err) => Err(anyhow!(
					"Failed to parse response from docker api as json: [{:?}]",
					json_err
				)),
			}
		} else {
			// Ensure the response body is read in it's entirerty. Otherwise
			// the body could still be writing, but we think we're done with the
			// request, and all of a sudden we're writing to a socket while
			// a response body is all being written and it's all bad.
			let _ = resp.text();
			Ok(serde_json::Value::default())
		}
	}

	/// Call the docker engine api using the POST http method.
	///
	/// `client`: the http client to use.
	/// `path`: the path to call (along with Query Args).
	/// `body`: The body to send to the remote endpoint.
	/// `timeout`: the optional timeout. Defaults to 30 seconds.
	/// `is_json`: whether to attempt to read the response body as json.
	async fn docker_api_post(
		client: &HttpClient,
		path: &str,
		body: Option<serde_json::Value>,
		timeout: Option<std::time::Duration>,
		is_json: bool,
	) -> Result<JsonValue> {
		let _guard = DOCK_SOCK_LOCK.lock().await;

		let timeout_frd = timeout.unwrap_or_else(|| std::time::Duration::from_millis(30000));
		let url = format!("http://localhost{}{}", DOCKER_API_VERSION, path);
		debug!("URL for get will be: {}", url);
		let req_part = Request::post(url)
			.header("Accept", "application/json; charset=UTF-8")
			.header("Content-Type", "application/json; charset=UTF-8")
			.header("Expect", "");
		let req = if let Some(body_data) = body {
			req_part.body(serde_json::to_vec(&body_data)?).unwrap()
		} else {
			req_part.body(Vec::new()).unwrap()
		};
		let resp_fut_res = future::timeout(timeout_frd, client.send_async(req)).await?;
		if let Err(resp_fut_err) = resp_fut_res {
			return Err(anyhow!(
				"Failed to send docker API Request due to: [{:?}]",
				resp_fut_err,
			));
		}
		let mut resp = resp_fut_res.unwrap();

		let status = resp.status().as_u16();
		if status < 200 || status > 299 {
			return Err(anyhow!(
				"Docker responded with an invalid status code: [{}] to path: [{}]",
				resp.status().as_u16(),
				path
			));
		}

		if is_json {
			let serialized_resp = resp.json();
			match serialized_resp {
				Ok(json_value) => Ok(json_value),
				Err(json_err) => Err(anyhow!(
					"Failed to parse response from docker api as json: [{:?}]",
					json_err
				)),
			}
		} else {
			// Ensure the response body is read in it's entirerty. Otherwise
			// the body could still be writing, but we think we're done with the
			// request, and all of a sudden we're writing to a socket while
			// a response body is all being written and it's all bad.
			let _ = resp.text();
			Ok(serde_json::Value::default())
		}
	}

	/// Call the docker engine api using the POST http method.
	///
	/// `client`: the http client to use.
	/// `path`: the path to call (along with Query Args).
	/// `body`: The body to send to the remote endpoint.
	/// `timeout`: the timeout for this requests, defaults to 30 seconds.
	/// `is_json`: whether to actually try to read the response body as json.
	async fn docker_api_delete(
		client: &HttpClient,
		path: &str,
		body: Option<serde_json::Value>,
		timeout: Option<std::time::Duration>,
		is_json: bool,
	) -> Result<JsonValue> {
		let _guard = DOCK_SOCK_LOCK.lock().await;

		let timeout_frd = timeout.unwrap_or_else(|| std::time::Duration::from_millis(30000));
		let url = format!("http://localhost{}{}", DOCKER_API_VERSION, path);
		debug!("URL for get will be: {}", url);

		let req_part = Request::delete(url)
			.header("Accept", "application/json; charset=UTF-8")
			.header("Content-Type", "application/json; charset=UTF-8")
			.header("Expect", "");
		let req = if let Some(body_data) = body {
			req_part.body(serde_json::to_vec(&body_data)?).unwrap()
		} else {
			req_part.body(Vec::new()).unwrap()
		};
		let resp_fut_res = future::timeout(timeout_frd, client.send_async(req)).await?;
		if let Err(resp_fut_err) = resp_fut_res {
			return Err(anyhow!(
				"Failed to send docker API Request due to: [{:?}]",
				resp_fut_err,
			));
		}
		let mut resp = resp_fut_res.unwrap();

		let status = resp.status().as_u16();
		if status < 200 || status > 299 {
			return Err(anyhow!(
				"Docker responded with an invalid status code: [{}] to path: [{}]",
				resp.status().as_u16(),
				path
			));
		}

		if is_json {
			let serialized_resp = resp.json();
			match serialized_resp {
				Ok(json_value) => Ok(json_value),
				Err(json_err) => Err(anyhow!(
					"Failed to parse response from docker api as json: [{:?}]",
					json_err
				)),
			}
		} else {
			// Ensure the response body is read in it's entirerty. Otherwise
			// the body could still be writing, but we think we're done with the
			// request, and all of a sudden we're writing to a socket while
			// a response body is all being written and it's all bad.
			let _ = resp.text();
			Ok(serde_json::Value::default())
		}
	}

	/// Attempt to clean up all resources left behind by the docker executor.
	#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
	pub async fn clean() {
		// Cleanup all things left behind by the docker executor.
		info!("Performing Cleanup for Docker Executor");
		if Self::is_compatible().await != CompatibilityStatus::Compatible {
			info!("Docker is not listening on this host!");
			return;
		}

		let client = if cfg!(target_os = "windows") {
			HttpClientBuilder::new()
				.version_negotiation(VersionNegotiation::http11())
				.build()
		} else {
			HttpClientBuilder::new()
				.unix_socket(SOCKET_PATH.to_owned())
				.version_negotiation(VersionNegotiation::http11())
				.build()
		};
		if let Err(client_err) = client {
			error!("Failed to construct HTTP CLIENT: [{:?}]", client_err);
			return;
		}
		let client = client.unwrap();

		// First cleanup containers.
		let containers_json_res =
			Self::docker_api_get(&client, "/containers/json?all=true", None, true).await;
		match containers_json_res {
			Ok(res) => {
				if let Some(containers) = res.as_array() {
					for container in containers {
						let names_opt = container.get("Names");
						if names_opt.is_none() {
							continue;
						}
						let names_untyped = names_opt.unwrap();
						let names_typed_opt = names_untyped.as_array();
						if names_typed_opt.is_none() {
							continue;
						}
						let names = names_typed_opt.unwrap();

						let mut dl_name = String::new();
						for name in names {
							if let Some(name_str) = name.as_str() {
								if name_str.starts_with("/dl-") {
									dl_name = name_str.to_owned();
								}
							}
						}

						if dl_name.is_empty() {
							continue;
						}

						info!("Found dev-loop container: [{}]", dl_name);
						// The container may already be stopped so ignore kill.
						let _ = Self::docker_api_post(
							&client,
							&format!("/containers{}/kill", dl_name),
							None,
							None,
							false,
						)
						.await;
						// The container may have been launched with --rm.
						let _ = Self::docker_api_delete(
							&client,
							&format!("/containers{}?v=true&force=true&link=true", dl_name,),
							None,
							None,
							false,
						)
						.await;
					}
				}
			}
			Err(container_json_err) => {
				error!(
					"Not cleaning up docker containers, failed to fetch them: [{:?}]",
					container_json_err
				);
			}
		}

		// Next checkout networks...
		let network_json_res = Self::docker_api_get(&client, "/networks", None, true).await;
		match network_json_res {
			Ok(res) => {
				if let Some(networks) = res.as_array() {
					for network in networks {
						if let Some(name_untyped) = network.get("Name") {
							if let Some(name_str) = name_untyped.as_str() {
								if name_str.starts_with("dl-") {
									info!("Found dev-loop network: [{}]", name_str);

									if let Err(delete_err) = Self::docker_api_delete(
										&client,
										&format!("/networks/{}", name_str),
										None,
										None,
										false,
									)
									.await
									{
										error!("Failed to delete network: [{:?}]", delete_err);
									}
								}
							}
						}
					}
				}
			}
			Err(network_err) => {
				error!(
					"Not cleaning up docker networks, failed to fetch them: [{:?}]",
					network_err
				);
			}
		}

		// Done! \o/
		info!("Cleaned!");
	}

	/// Determines if this `DockerExecutor` is compatible with the system.
	#[allow(clippy::single_match_else)]
	pub async fn is_compatible() -> CompatibilityStatus {
		let client = if cfg!(target_os = "windows") {
			HttpClientBuilder::new()
				.version_negotiation(VersionNegotiation::http11())
				.build()
		} else {
			HttpClientBuilder::new()
				.unix_socket(SOCKET_PATH.to_owned())
				.version_negotiation(VersionNegotiation::http11())
				.build()
		};
		if let Err(client_err) = client {
			error!("Failed to construct HTTP CLIENT: [{:?}]", client_err);
			return CompatibilityStatus::CannotBeCompatible(Some(
				"Failed to construct HTTP CLIENT!".to_owned(),
			));
		}
		let client = client.unwrap();

		let version_resp_res = Self::docker_api_get(&client, "/version", None, true).await;

		match version_resp_res {
			Ok(data) => match data.get("Version") {
				Some(_) => CompatibilityStatus::Compatible,
				None => {
					warn!("Failed to get key: `Version` from docker executor api!");
					CompatibilityStatus::CouldBeCompatible("install docker".to_owned())
				}
			},
			Err(http_err) => {
				warn!("Failed to reach out to docker engine api: [{:?}]", http_err);
				CompatibilityStatus::CouldBeCompatible("install docker".to_owned())
			}
		}
	}

	/// Get the container name used for this Docker Executor.
	#[must_use]
	pub fn get_container_name(&self) -> &str {
		&self.container_name
	}

	/// Ensure a particular network exists.
	///
	/// `client`: the http client.
	/// `pipeline_id`: the pipeline id for the task.
	///
	/// # Errors
	///
	/// If we cannot talk to the docker socket, or there is an error creating the network.
	pub async fn ensure_network_exists(client: &HttpClient, pipeline_id: &str) -> Result<()> {
		let network_id = format!("dl-{}", pipeline_id);
		let network_url = format!("/networks/{}", network_id);
		let res = Self::docker_api_get(client, &network_url, None, true).await;
		if res.is_err() {
			let json_body = serde_json::json!({
				"Name": network_id,
			});
			let _ = Self::docker_api_post(client, "/networks/create", Some(json_body), None, false)
				.await?;
		}
		Ok(())
	}

	/// Download the Image for this docker executor.
	///
	/// # Errors
	///
	/// Errors when it the docker api cannot be talked too, or the image cannot be downloaded.
	pub async fn download_image(&self) -> Result<()> {
		let image_tag_split = self.image.rsplitn(2, ':').collect::<Vec<&str>>();
		let (image_name, tag_name) = if image_tag_split.len() == 2 {
			(image_tag_split[1], image_tag_split[0])
		} else {
			(image_tag_split[0], "latest")
		};
		info!(
			"Downloading Image: {}name: {:?}, tag: {:?}{}",
			"{", image_name, tag_name, "}"
		);
		let url = format!("/images/create?fromImage={}&tag={}", image_name, tag_name);
		let _ = Self::docker_api_post(
			&self.client,
			&url,
			None,
			Some(std::time::Duration::from_secs(3600)),
			false,
		)
		.await?;

		Ok(())
	}

	/// Determine if the container is created, and then if it's wondering.
	///
	/// # Errors
	///
	/// Errors when the docker api cannot be talked to.
	pub async fn is_container_created_and_running(&self) -> Result<(bool, bool)> {
		let url = format!("/containers/{}/json?size=false", &self.container_name);
		let mut is_created = false;
		let mut is_running = false;

		if let Ok(value) = Self::docker_api_get(&self.client, &url, None, true).await {
			is_created = true;
			let is_running_status = &value["State"]["Running"];
			if is_running_status.is_boolean() {
				is_running = is_running_status.as_bool().unwrap();
			}
		}

		Ok((is_created, is_running))
	}

	/// Creates the container, should only be called when it does not yet exist.
	///
	/// # Errors
	///
	/// Errors when the docker socket cannot be talked too, or there is a conflict
	/// creating the container.
	pub async fn create_container(&self) -> Result<()> {
		let tmp_dir = get_tmp_dir();
		let tmp_path = tmp_dir.to_str().unwrap();

		let mut mounts = Vec::new();
		mounts.push(serde_json::json!({
			"Source": self.project_root,
			"Target": "/mnt/dl-root",
			"Type": "bind",
			"Consistency": "consistent",
		}));
		mounts.push(serde_json::json!({
			"Source": tmp_path,
			"Target": "/tmp",
			"Type": "bind",
			"Consistency": "consistent",
		}));
		for emount in &self.extra_mounts {
			let mut split = emount.split(':');
			let source: &str = split.next().unwrap();
			let target: &str = split.next().unwrap();
			mounts.push(serde_json::json!({
				"Source": source,
				"Target": target,
				"Type": "bind",
				"Consistency": "consistent",
			}));
		}

		let mut port_mapping = serde_json::map::Map::<String, serde_json::Value>::new();
		let mut host_config_mapping = serde_json::map::Map::<String, serde_json::Value>::new();

		for tcp_port in &self.tcp_ports_to_expose {
			port_mapping.insert(format!("{}/tcp", tcp_port), serde_json::json!({}));
			host_config_mapping.insert(
				format!("{}/tcp", tcp_port),
				serde_json::json!([serde_json::json!({ "HostPort": format!("{}", tcp_port) }),]),
			);
		}
		for udp_port in &self.udp_ports_to_expose {
			port_mapping.insert(format!("{}/udp", udp_port), serde_json::json!({}));
			host_config_mapping.insert(
				format!("{}/udp", udp_port),
				serde_json::json!([serde_json::json!({ "HostPort": format!("{}", udp_port) }),]),
			);
		}

		let url = format!("/containers/create?name={}", &self.container_name);
		let body = serde_json::json!({
			"Cmd": ["tail", "-f", "/dev/null"],
			"Entrypoint": "",
			"Image": self.image,
			"Hostname": self.hostname,
			"User": self.user,
			"HostConfig": {
				"AutoRemove": true,
				"Mounts": mounts,
				"Privileged": true,
				"PortBindings": host_config_mapping,
			},
			"WorkingDir": "/mnt/dl-root",
			"AttachStdout": true,
			"AttachStderr": true,
			"Privileged": true,
			"Tty": true,
			"ExposedPorts": port_mapping,
		});
		let _ = Self::docker_api_post(&self.client, &url, Some(body), None, false).await?;

		Ok(())
	}

	/// Execute a raw command, returning the "execution id" to check back in on it.
	///
	/// `command`: the command to execute.
	/// `use_user_ids`: if to use the setup user ids (always true during non-setup).
	///
	/// # Errors
	///
	/// Errors if we fail to create an exec instance with docker.
	pub async fn raw_execute(&self, command: &[String], use_user_ids: bool) -> Result<String> {
		let url = format!("/containers/{}/exec", &self.container_name);
		let body = if use_user_ids && (self.run_as_user_id != 0 || self.run_as_group_id != 0) {
			serde_json::json!({
				"AttachStdout": true,
				"AttachStderr": true,
				"Tty": false,
				"User": &format!("{}:{}", self.run_as_user_id, self.run_as_group_id),
				"Privileged": true,
				"Cmd": command,
				"Env": &self.environment_to_export,
			})
		} else {
			serde_json::json!({
				"AttachStdout": true,
				"AttachStderr": true,
				"Tty": false,
				"User": &self.user,
				"Privileged": true,
				"Cmd": command,
				"Env": &self.environment_to_export,
			})
		};

		let resp = Self::docker_api_post(&self.client, &url, Some(body), None, true).await?;
		let potential_id = &resp["Id"];
		if !potential_id.is_string() {
			return Err(anyhow!(
				"Failed to find \"Id\" in response from docker: [{:?}]",
				resp,
			));
		}
		let exec_id = potential_id.as_str().unwrap().to_owned();

		let start_url = format!("/exec/{}/start", &exec_id);
		let start_body = serde_json::json!({
			"Detach": true,
			"Tty": false,
		});

		let _ =
			Self::docker_api_post(&self.client, &start_url, Some(start_body), None, false).await?;

		Ok(exec_id)
	}

	/// Execute a raw command, and wait til it's finished. Returns execution id so you can checkup on it.
	///
	/// `command`: the command to execute.
	/// `use_user_ids`: if to use the setup user ids (always true during non-setup).
	///
	/// # Errors
	///
	/// Errors if we cannot talk to docker to create an exec instance.
	pub async fn raw_execute_and_wait(
		&self,
		command: &[String],
		use_user_ids: bool,
	) -> Result<String> {
		let execution_id = self.raw_execute(command, use_user_ids).await?;

		loop {
			if Self::has_execution_finished(&self.client, &execution_id).await {
				break;
			}

			async_std::task::sleep(std::time::Duration::from_micros(10)).await;
		}

		Ok(execution_id)
	}

	/// Determine if a particular execution ID has finished executing.
	///
	/// `client`: the http client instance to query the docker socket.
	/// `execution_id`: the execution id to check if it's finished.
	pub async fn has_execution_finished(client: &HttpClient, execution_id: &str) -> bool {
		let url = format!("/exec/{}/json", execution_id);
		let resp_res = Self::docker_api_get(client, &url, None, true).await;
		if resp_res.is_err() {
			return false;
		}
		let resp = resp_res.unwrap();
		let is_running_opt = &resp["Running"];
		if !is_running_opt.is_boolean() {
			return false;
		}

		!is_running_opt.as_bool().unwrap()
	}

	/// Get the exit code for a particular execution
	///
	/// `client`: the HTTP Client to talk to the docker engine api.
	/// `execution_id`: the execution id.
	///
	/// # Errors
	///
	/// If we cannot find an `ExitCode` in the docker response, or talk to the docker socket.
	pub async fn get_execution_status_code(client: &HttpClient, execution_id: &str) -> Result<i64> {
		let url = format!("/exec/{}/json", execution_id);
		let resp = Self::docker_api_get(client, &url, None, true).await?;
		let exit_code_opt = &resp["ExitCode"];
		if !exit_code_opt.is_i64() {
			return Err(anyhow!(
				"Failed to find integer ExitCode in response: [{:?}]",
				resp,
			));
		}

		Ok(exit_code_opt.as_i64().unwrap())
	}

	/// Setup the permission helper for this docker container if it's been configured.
	///
	/// # Errors
	///
	/// If we cannot talk to the docker socket, or cannot create the user.
	pub async fn setup_permission_helper(&self) -> Result<()> {
		if self.run_as_user_id != 0 || self.run_as_group_id != 0 {
			let sudo_execution_id = self
				.raw_execute_and_wait(
					&[
						"/usr/bin/env".to_owned(),
						"bash".to_owned(),
						"-c".to_owned(),
						"hash sudo".to_owned(),
					],
					false,
				)
				.await?;
			let has_sudo =
				Self::get_execution_status_code(&self.client, &sudo_execution_id).await? == 0;

			// This may be a re-used docker container in which case a user with 'dl'
			// already exists.
			let user_exist_id = self
				.raw_execute_and_wait(
					&[
						"/usr/bin/env".to_owned(),
						"bash".to_owned(),
						"-c".to_owned(),
						"getent passwd dl".to_owned(),
					],
					false,
				)
				.await?;
			if Self::get_execution_status_code(&self.client, &user_exist_id).await? == 0 {
				return Ok(());
			}

			// Create the user.
			let creation_execution_id = match (self.user == "root", has_sudo) {
				(true, _) | (false, false) => {
					self.raw_execute_and_wait(&[
						"/usr/bin/env".to_owned(),
						"bash".to_owned(),
						"-c".to_owned(),
						format!("groupadd -g {} -o dl && useradd -u {} -g {} -o -c '' -m dl", self.run_as_group_id, self.run_as_user_id, self.run_as_group_id)
					], false).await?
				},
				(false, true) => {
					self.raw_execute_and_wait(&[
						"/usr/bin/env".to_owned(),
						"bash".to_owned(),
						"-c".to_owned(),
						format!("sudo -n groupadd -g {} -o dl && sudo -n useradd -u {} -g {} -o -c '' -m dl", self.run_as_group_id, self.run_as_user_id, self.run_as_group_id)
					], false).await?
				},
			};
			if Self::get_execution_status_code(&self.client, &creation_execution_id).await? != 0 {
				return Err(anyhow!(
					"Failed to get successful ExitCode from docker on user creation!"
				));
			}

			// Allow the user to sudo, if sudo is installed.
			if has_sudo {
				let sudo_user_creation_id = if self.user == "root" {
					self.raw_execute_and_wait(&[
						"/usr/bin/env".to_owned(),
						"bash".to_owned(),
						"-c".to_owned(),
						"mkdir -p /etc/sudoers.d && echo \"dl ALL=(root) NOPASSWD:ALL\" > /etc/sudoers.d/dl && chmod 0440 /etc/sudoers.d/dl".to_owned()
					], false).await?
				} else {
					self.raw_execute_and_wait(&[
							"/usr/bin/env".to_owned(),
							"bash".to_owned(),
							"-c".to_owned(),
							"sudo -n mkdir -p /etc/sudoers.d && echo \"dl ALL=(root) NOPASSWD:ALL\" | sudo -n tee /etc/sudoers.d/dl && sudo -n chmod 0440 /etc/sudoers.d/dl".to_owned()
						], false).await?
				};

				if Self::get_execution_status_code(&self.client, &sudo_user_creation_id).await? != 0
				{
					return Err(anyhow!(
						"Failed to setup passwordless sudo access for user!"
					));
				}
			}

			Ok(())
		} else {
			Ok(())
		}
	}

	/// Ensure the docker container exists.
	///
	/// # Errors
	///
	/// If we cannot talk to the docker socket, or there is a conflict creating the container.
	pub async fn ensure_docker_container(&self) -> Result<()> {
		let image_exists_url = format!("/images/{}/json", &self.image);
		let image_exists = Self::docker_api_get(&self.client, &image_exists_url, None, false)
			.await
			.is_ok();

		if !image_exists {
			self.download_image().await?;
		}
		let (container_exists, container_running) = self.is_container_created_and_running().await?;

		if !container_exists {
			self.create_container().await?;
		}

		if !container_running {
			let url = format!("/containers/{}/start", self.container_name);
			let _ = Self::docker_api_post(&self.client, &url, None, None, false).await?;
		}

		let execution_id = self
			.raw_execute_and_wait(
				&[
					"/usr/bin/env".to_owned(),
					"bash".to_owned(),
					"-c".to_owned(),
					"hash bash".to_owned(),
				],
				false,
			)
			.await?;

		let has_bash = Self::get_execution_status_code(&self.client, &execution_id).await?;
		if has_bash != 0 {
			return Err(anyhow!(
				"Docker Image: [{}] does not seem to have bash! This is required for dev-loop!",
				self.image,
			));
		}

		let perm_helper_setup = self.setup_permission_helper().await;
		if let Err(perm_helper_err) = perm_helper_setup {
			return Err(perm_helper_err);
		}

		Ok(())
	}

	/// Determine if the container is attached to a particular network.
	///
	/// `network_id`: the id of the network to attach.
	pub async fn is_network_attached(&self, network_id: &str) -> bool {
		let url = format!("/containers/{}/json", self.container_name);
		let body_res = Self::docker_api_get(&self.client, &url, None, true).await;
		if body_res.is_err() {
			return false;
		}
		let body = body_res.unwrap();
		let id_as_opt = body["Id"].as_str();
		if id_as_opt.is_none() {
			return false;
		}
		let id = id_as_opt.unwrap();

		let network_url = format!("/networks/dl-{}", network_id);
		let network_body_res = Self::docker_api_get(&self.client, &network_url, None, true).await;
		if network_body_res.is_err() {
			return false;
		}
		let network_body = network_body_res.unwrap();
		let networks_obj_opt = network_body["Containers"].as_object();
		if networks_obj_opt.is_none() {
			return false;
		}
		let networks_obj = networks_obj_opt.unwrap();

		networks_obj.contains_key(id)
	}

	/// Ensure a particular network has been attached to this container.
	///
	/// `network_id`: The Network ID to attach too.
	///
	/// # Errors
	///
	/// If we fail to talk to the docker socket, or connect the container to the network.
	pub async fn ensure_network_attached(&self, network_id: &str) -> Result<()> {
		if !self.is_network_attached(network_id).await {
			let url = format!("/networks/dl-{}/connect", network_id);
			let body = serde_json::json!({
				"Container": self.container_name,
				"EndpointConfig": {
					"Aliases": [self.hostname],
				}
			});

			let _ = Self::docker_api_post(&self.client, &url, Some(body), None, false).await?;
		}

		Ok(())
	}
}

#[async_trait::async_trait]
impl Executor for DockerExecutor {
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
					warn!(
						"Skipping requirement for: [{}] invalid semver version",
						req.get_name()
					);
					continue;
				}
			}
		}

		met
	}

	#[allow(
		clippy::cast_possible_truncation,
		clippy::cognitive_complexity,
		clippy::too_many_lines,
		clippy::used_underscore_binding,
		unused_assignments
	)]
	#[must_use]
	async fn execute(
		&self,
		log_channel: Sender<(String, String, bool)>,
		should_stop: Arc<AtomicBool>,
		helper_src_line: &str,
		task: &ExecutableTask,
	) -> isize {
		// Execute a particular task inside the docker executor.
		//
		// 1. Create the network, and stand it up if not.
		// 2. Create the container, and stand it up if not.
		// 3. Connect the container to the appropriate network.
		// 4. Create a temporary directory for the pipeline id, and the task name.
		// 5. Write the task file the user specified.
		// 6. Write an "entrypoint" that sources in the helpers, and calls the script.
		// 7. Execute the script, and wait for it to finish.

		let res = Self::ensure_network_exists(&self.client, task.get_pipeline_id()).await;
		if let Err(network_creation_error) = res {
			error!("Failed to create network: [{:?}]", network_creation_error);
			return 10 as isize;
		}
		let container_res = self.ensure_docker_container().await;
		if let Err(container_err) = container_res {
			error!("Failed to create docker container: [{:?}]", container_err);
			return 10 as isize;
		}
		let attach_res = self.ensure_network_attached(task.get_pipeline_id()).await;
		if let Err(attach_err) = attach_res {
			error!("Failed to attach to network: [{:?}]", attach_err);
			return 10 as isize;
		}

		let mut tmp_path = get_tmp_dir();
		let mut tmp_path_in_docker = async_std::path::PathBuf::from("/tmp");
		tmp_path.push(task.get_pipeline_id().to_owned() + "-dl-host");
		tmp_path_in_docker.push(task.get_pipeline_id().to_owned() + "-dl-host");
		let res = async_std::fs::create_dir_all(tmp_path.clone()).await;
		if let Err(dir_err) = res {
			error!(
				"Failed to create pipeline directory due to: [{:?}]",
				dir_err,
			);
			return 10;
		}

		let mut regular_task = tmp_path.clone();
		let mut regular_task_in_docker = tmp_path_in_docker.clone();
		regular_task.push(task.get_task_name().to_owned() + ".sh");
		regular_task_in_docker.push(task.get_task_name().to_owned() + ".sh");
		info!("Task writing to path: [{:?}]", regular_task);
		let write_res =
			async_std::fs::write(&regular_task, task.get_contents().get_contents()).await;
		if let Err(write_err) = write_res {
			error!("Failed to write script file due to: [{:?}]", write_err);
			return 10;
		}
		let path_as_str = regular_task_in_docker.to_str().unwrap();

		let epoch = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_secs();
		let mut stdout_log_path = tmp_path.clone();
		let mut stdout_log_path_in_docker = tmp_path_in_docker.clone();
		stdout_log_path.push(format!("{}-{}-out.log", epoch, task.get_task_name()));
		stdout_log_path_in_docker.push(format!("{}-{}-out.log", epoch, task.get_task_name()));
		let mut stderr_log_path = tmp_path.clone();
		let mut stderr_log_path_in_docker = tmp_path_in_docker.clone();
		stderr_log_path.push(format!("{}-{}-err.log", epoch, task.get_task_name()));
		stderr_log_path_in_docker.push(format!("{}-{}-err.log", epoch, task.get_task_name()));
		{
			if let Err(log_err) = async_std::fs::File::create(&stdout_log_path).await {
				error!("Failed to create stdout log file: [{:?}]", log_err);
				return 10;
			}
		}
		{
			if let Err(log_err) = async_std::fs::File::create(&stderr_log_path).await {
				error!("Failed to create stderr log file: [{:?}]", log_err);
				return 10;
			}
		}
		if cfg!(target_family = "unix") {
			{
				use std::os::unix::fs::PermissionsExt;
				let log_permissions = std::fs::Permissions::from_mode(0o666);

				if std::fs::set_permissions(&stdout_log_path, log_permissions.clone()).is_err() {
					error!("Failed to mark stdout_log as world writable! May cause errors if using a lower-priveleged user!");
				}
				if std::fs::set_permissions(&stderr_log_path, log_permissions).is_err() {
					error!("Failed to mark stderr_log as world writable! May cause errors if using a lower-priveleged user!");
				}
			}
		}

		let stdout_path_as_str = stdout_log_path_in_docker.to_str().unwrap();
		let stderr_path_as_str = stderr_log_path_in_docker.to_str().unwrap();

		let entry_point_file = format!(
			"#!/usr/bin/env bash

{opening_bracket}

cd /mnt/dl-root/

# Source Helpers
{helper}
eval \"$(declare -F | sed -e 's/-f /-fx /')\"

{script} {arg_str}

{closing_bracket} >{stdout_log_path} 2>{stderr_log_path}",
			helper = helper_src_line,
			script = path_as_str,
			arg_str = task.get_arg_string(),
			opening_bracket = "{",
			closing_bracket = "}",
			stdout_log_path = stdout_path_as_str,
			stderr_log_path = stderr_path_as_str,
		);
		tmp_path.push(task.get_task_name().to_owned() + "-entrypoint.sh");
		tmp_path_in_docker.push(task.get_task_name().to_owned() + "-entrypoint.sh");
		info!("Task entrypoint is being written too: [{:?}]", tmp_path);
		let write_res = std::fs::write(&tmp_path, entry_point_file);
		if let Err(write_err) = write_res {
			error!("Failed to write entrypoint file due to: [{:?}]", write_err);
			return 10;
		}

		if cfg!(target_family = "unix") {
			use std::os::unix::fs::PermissionsExt;
			let executable_permissions = std::fs::Permissions::from_mode(0o777);

			if let Err(exec_err) =
				std::fs::set_permissions(&tmp_path, executable_permissions.clone())
			{
				error!("Failed to mark entrypoint as executable: [{:?}]", exec_err);
			}
			if let Err(exec_err) = std::fs::set_permissions(&regular_task, executable_permissions) {
				error!("Failed to mark task file as executable: [{:?}]", exec_err);
			}
		}

		let entrypoint_as_str = tmp_path_in_docker.to_str().unwrap();

		let command_res = self
			.raw_execute(&[entrypoint_as_str.to_owned()], true)
			.await;
		if let Err(command_err) = command_res {
			error!("Failed to execute command: [{:?}]", command_err);
			return 10;
		}
		let exec_id = command_res.unwrap();

		let has_finished = Arc::new(AtomicBool::new(false));

		let flush_channel_clone = log_channel.clone();
		let flush_task_name = task.get_task_name().to_owned();
		let flush_is_finished_clone = has_finished.clone();

		let flush_task = async_std::task::spawn(async move {
			let mut line = String::new();
			let file = std::fs::File::open(stdout_log_path)
				.expect("Failed to open log file even though we created it!");
			let err_file = std::fs::File::open(stderr_log_path)
				.expect("Failed to open stderr log file even though we created it!");
			let mut reader = BufReader::new(file);
			let mut stderr_reader = BufReader::new(err_file);

			while !flush_is_finished_clone.load(Ordering::Relaxed) {
				while let Ok(read) = reader.read_line(&mut line) {
					if read == 0 {
						break;
					}

					let _ = flush_channel_clone.send((flush_task_name.clone(), line, false));
					line = String::new();
				}
				while let Ok(read) = stderr_reader.read_line(&mut line) {
					if read == 0 {
						break;
					}

					let _ = flush_channel_clone.send((flush_task_name.clone(), line, true));
					line = String::new();
				}

				async_std::task::sleep(std::time::Duration::from_millis(10)).await;
			}
		});

		let mut rc = 0;

		// Loop until completion...
		loop {
			// Has the exec finished?
			if Self::has_execution_finished(&self.client, &exec_id).await {
				let rc_res = Self::get_execution_status_code(&self.client, &exec_id).await;
				if let Err(rc_err) = rc_res {
					error!("Failed to read child status: [{:?}]", rc_err);
					rc = 10;
					break;
				}
				rc = rc_res.unwrap();
				break;
			}

			// Have we been requested to stop?
			if should_stop.load(Ordering::SeqCst) {
				error!("Docker Executor was told to stop!");
				rc = 10;
				break;
			}

			async_std::task::sleep(std::time::Duration::from_millis(10)).await;
		}

		has_finished.store(true, Ordering::SeqCst);
		flush_task.await;

		rc as isize
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
				DockerExecutor::new(&pb, &args, &provided_conf, None,).is_err(),
				"Docker Executor without a name_prefix should error!",
			);
		}

		{
			let mut args = HashMap::new();
			args.insert("name_prefix".to_owned(), "asdf-".to_owned());
			let provided_conf = Vec::new();
			let pb = PathBuf::from("/tmp/non-existant");

			assert!(
				DockerExecutor::new(&pb, &args, &provided_conf, None,).is_err(),
				"Docker executor without an image should error!",
			);
		}

		{
			let mut args = HashMap::new();
			args.insert("name_prefix".to_owned(), "asdf-".to_owned());
			args.insert("image".to_owned(), "localhost:5000/blah:latest".to_owned());
			let provided_conf = Vec::new();
			let pb = PathBuf::from("/tmp/non-existant");

			assert!(
				DockerExecutor::new(&pb, &args, &provided_conf, None,).is_ok(),
				"Docker executor with an image/name prefix should succeed!",
			);
		}
	}

	#[test]
	fn get_container_name() {
		let mut args = HashMap::new();
		args.insert("name_prefix".to_owned(), "name-prefix-".to_owned());
		args.insert("image".to_owned(), "localhost:5000/blah:latest".to_owned());
		let provided_conf = Vec::new();
		let pb = PathBuf::from("/tmp/non-existant");

		let de = DockerExecutor::new(&pb, &args, &provided_conf, None)
			.expect("Docker Executor in get_name should be able to be constructed!");

		assert!(
			de.get_container_name().starts_with("dl-name-prefix-"),
			"Docker Executor Name needs to start with dl-name-prefix-!",
		);
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

		let de = DockerExecutor::new(&pb, &args, &provided_conf, None)
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
