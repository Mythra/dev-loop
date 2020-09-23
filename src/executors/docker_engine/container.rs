use color_eyre::{eyre::eyre, Report, Result, Section};
use std::{collections::HashMap, env::var as env_var, path::PathBuf};
use tracing::warn;

const CONTAINER_NAME_ARG: &str = "name_prefix";
const HOSTNAME_ARG: &str = "hostname";
const IMAGE_ARG: &str = "image";
const ENV_TO_EXPORT_ARG: &str = "export_env";
const MOUNTS_ARG: &str = "extra_mounts";
const PERMISSION_HELPER_ARG: &str = "experimental_permission_helper";
const TCP_PORTS_TO_EXPOSE_ARG: &str = "tcp_ports_to_expose";
const USER_ARG: &str = "user";
const UDP_PORTS_TO_EXPOSE_ARG: &str = "udp_ports_to_expose";

/// Represents a `DockerContainer` managed by the docker-engine/docker executor.
#[derive(Debug)]
pub struct DockerContainerInfo {
	/// The container name to use.
	container_name: String,
	/// The docker image.
	image: String,
	/// A list of environment variables to export.
	environment_to_export: Vec<String>,
	/// A list of extra mounts.
	extra_mounts: Vec<String>,
	/// The list of tcp ports to expose.
	tcp_ports_to_expose: Vec<u32>,
	/// The list of udp ports to expose.
	udp_ports_to_expose: Vec<u32>,
	/// The hostname of this container.
	hostname: String,
	/// The base user to use.
	base_user: String,
	/// The proxied user id.
	proxy_user_id: Option<u32>,
	/// The proxied group id.
	proxy_group_id: Option<u32>,
}

impl DockerContainerInfo {
	pub fn new(
		executor_args: &HashMap<String, String>,
		project_root_str: &str,
		random_str: &str,
	) -> Result<Self> {
		let (proxy_user, proxy_group) = get_proxy_user_information(executor_args);

		Ok(Self {
			container_name: container_name_from_arg(executor_args, random_str)?,
			image: image_from_arg(executor_args)?,
			environment_to_export: get_env_vars_to_export(executor_args),
			extra_mounts: get_extra_mounts(executor_args, project_root_str),
			tcp_ports_to_expose: tcp_ports_to_expose(executor_args),
			udp_ports_to_expose: udp_ports_to_expose(executor_args),
			hostname: get_hostname(executor_args),
			base_user: get_user(executor_args),
			proxy_user_id: proxy_user,
			proxy_group_id: proxy_group,
		})
	}

	pub fn get_container_name(&self) -> &str {
		&self.container_name
	}

	pub fn get_image(&self) -> &str {
		&self.image
	}

	pub fn get_environment_to_export(&self) -> &[String] {
		&self.environment_to_export
	}

	pub fn get_extra_mounts(&self) -> &[String] {
		&self.extra_mounts
	}

	pub fn get_tcp_ports_to_expose(&self) -> &[u32] {
		&self.tcp_ports_to_expose
	}

	pub fn get_udp_ports_to_expose(&self) -> &[u32] {
		&self.udp_ports_to_expose
	}

	pub fn get_hostname(&self) -> &str {
		&self.hostname
	}

	pub fn get_base_user(&self) -> &str {
		&self.base_user
	}

	pub fn get_proxy_user_id(&self) -> Option<&u32> {
		self.proxy_user_id.as_ref()
	}

	pub fn get_cloned_proxy_user_id(&self) -> Option<u32> {
		self.proxy_user_id
	}

	pub fn get_proxy_group_id(&self) -> Option<&u32> {
		self.proxy_group_id.as_ref()
	}

	pub fn get_cloned_proxy_group_id(&self) -> Option<u32> {
		self.proxy_group_id
	}
}

fn container_name_from_arg(args: &HashMap<String, String>, random_str: &str) -> Result<String> {
	let mut container_name = "dl-".to_owned();
	if let Some(user_specified_prefix) = args.get(CONTAINER_NAME_ARG) {
		container_name += user_specified_prefix;
	} else {
		return Err(eyre!(
			"Docker Container require a `name_prefix` field to know how to name containers!"
		)).suggestion("Add a `name_prefix` field to `params` that specifys the name prefix for containers")
			.note("You can find the full list of fields here: https://dev-loop.kungfury.dev/docs/schemas/executor-conf");
	}
	container_name += &random_str;

	Ok(container_name)
}

fn image_from_arg(args: &HashMap<String, String>) -> Result<String> {
	let image;
	if let Some(image_identifier) = args.get(IMAGE_ARG) {
		image = image_identifier.to_owned();
	} else {
		return Err(eyre!(
			"Docker Container requires an `image` to know which docker image to use."
		)).suggestion("Add an `image` field to `params` that specifys the docker image to use.")
			.note("You can find the full list of fields here: https://dev-loop.kungfury.dev/docs/schemas/executor-conf");
	}

	Ok(image)
}

fn get_env_vars_to_export(args: &HashMap<String, String>) -> Vec<String> {
	let mut env_vars = Vec::new();

	if let Some(envs_to_export) = args.get(ENV_TO_EXPORT_ARG) {
		env_vars = envs_to_export
			.split(',')
			.map(|the_str| {
				env_var(the_str)
					.map_or_else(|_| the_str.to_owned(), |val| format!("{}={}", the_str, val))
			})
			.collect::<Vec<String>>();
	}

	env_vars
}

fn get_extra_mounts(args: &HashMap<String, String>, project_root_str: &str) -> Vec<String> {
	let mut extra_mounts = Vec::new();

	if let Some(mount_str_ref) = args.get(MOUNTS_ARG) {
		extra_mounts = mount_str_ref
			.split(',')
			.filter_map(|item| {
				let mounts = item.split(':').collect::<Vec<&str>>();
				if mounts.len() != 2 {
					warn!(
						"{:?}",
						Err::<(), Report>(eyre!(
							"Mount String for Docker Container: [{}] is invalid, missing path for container. Will not mount.",
							item,
						))
						.note("Mounts should be in the format: `host_path:path_in_container`")
						.unwrap_err()
					);
					return None;
				}

				let src = mounts[0];
				let dest = mounts[1];

				let src =
					if src.starts_with('~') {
						let potential_home_dir = crate::dirs::home_dir();
						if potential_home_dir.is_none() {
							warn!(
								"{:?}",
								Err::<(), Report>(eyre!(
									"Mount String: [{}] for Docker Container's source path is relative to the home directory, but the home directory couldn't be found. Will not mount.",
									item,
								))
								.suggestion("You can manually specify the home directory with the `HOME` environment variable.")
								.unwrap_err()
							);
							return None;
						}
						let home_dir = potential_home_dir.unwrap();
						let home_dir = home_dir.to_str();
						if home_dir.is_none() {
							warn!(
								"{:?}",
								Err::<(), Report>(eyre!(
									"Home directory is not set to a UTF-8 only string."
								)).note("If you're not sure how to solve this error, please open an issue.").unwrap_err(),
							);
							return None;
						}
						let home_dir = home_dir.unwrap();

            src.replace("~", home_dir)
					} else if src.starts_with('/') {
						src.to_owned()
					} else {
						project_root_str.to_owned() + "/" + src
					};

        let src_as_pb = PathBuf::from(&src);
				if !src_as_pb.exists() {
					warn!(
						"{:?}",
						Err::<(), Report>(eyre!(
							"Mount String: [{}] specified a source directory: [{}] that does not exist. Will not mount.",
							item,
							src,
						)).unwrap_err(),
					);
					return None;
				}

					Some(format!("{}:{}", src, dest))
				})
				.collect::<Vec<String>>();
	}

	extra_mounts
}

fn tcp_ports_to_expose(args: &HashMap<String, String>) -> Vec<u32> {
	let mut tcp_ports_to_expose = Vec::new();
	if let Some(ports_to_expose) = args.get(TCP_PORTS_TO_EXPOSE_ARG) {
		tcp_ports_to_expose = ports_to_expose
			.split(',')
			.filter_map(|item| {
				let item_pr = item.parse::<u32>();
				if item_pr.is_err() {
					warn!(
						"Not exposing tcp port: [{}] as it is not a valid positive number.",
						item
					);
				}
				item_pr.ok()
			})
			.collect::<Vec<u32>>();
	}

	tcp_ports_to_expose
}

fn udp_ports_to_expose(args: &HashMap<String, String>) -> Vec<u32> {
	let mut udp_ports_to_expose = Vec::new();
	if let Some(ports_to_expose) = args.get(UDP_PORTS_TO_EXPOSE_ARG) {
		udp_ports_to_expose = ports_to_expose
			.split(',')
			.filter_map(|item| {
				let item_pr = item.parse::<u32>();
				if item_pr.is_err() {
					warn!(
						"Not exposing udp port: [{}] as it is not a valid positive number.",
						item
					);
				}

				item_pr.ok()
			})
			.collect::<Vec<u32>>();
	}

	udp_ports_to_expose
}

fn get_hostname(args: &HashMap<String, String>) -> String {
	if let Some(hostname_ref) = args.get(HOSTNAME_ARG) {
		hostname_ref.to_owned()
	} else {
		let mut string = args.get(CONTAINER_NAME_ARG).unwrap().to_owned();
		string.pop();
		string
	}
}

fn get_user(args: &HashMap<String, String>) -> String {
	args.get(USER_ARG)
		.map_or_else(|| "root".to_owned(), String::from)
}

fn get_proxy_user_information(args: &HashMap<String, String>) -> (Option<u32>, Option<u32>) {
	let mut proxy_user_id = None;
	let mut proxy_group_id = None;
	if let Some(permission_helper_active) = args.get(PERMISSION_HELPER_ARG) {
		if &permission_helper_active.to_ascii_lowercase() == "true" {
			proxy_user_id = Some(users::get_effective_uid());
			proxy_group_id = Some(users::get_effective_gid());
		}
	}

	(proxy_user_id, proxy_group_id)
}
