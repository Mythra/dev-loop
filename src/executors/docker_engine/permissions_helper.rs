use super::{execute_command_in_container, get_command_exit_code, DockerContainerInfo};

use color_eyre::{
	eyre::{eyre, WrapErr},
	Result, Section,
};
use isahc::HttpClient;
use once_cell::sync::Lazy;

static DOCK_USER_LOCK: Lazy<async_std::sync::Mutex<()>> =
	Lazy::new(|| async_std::sync::Mutex::new(()));
const _PERMISSIONS_HELPER_EXPERIMENTAL_SUGGESTION: &str = "The permissions helper is still experimental. Please report this so it can be fixed before stabilization.";

/// Setup the permission helper for this docker container if it's been configured.
///
/// # Errors
///
/// If we cannot talk to the docker socket, or cannot create the user.
pub async fn setup_permission_helper(
	client: &HttpClient,
	container: &DockerContainerInfo,
) -> Result<()> {
	let _guard = DOCK_USER_LOCK.lock().await;
	if container.get_proxy_user_id().is_none() || container.get_proxy_group_id().is_none() {
		return Ok(());
	}

	let forced_user_id = container.get_proxy_user_id().unwrap();
	let forced_group_id = container.get_proxy_group_id().unwrap();
	let has_sudo = container_has_sudo(
		client,
		container.get_container_name(),
		container.get_base_user(),
	)
	.await
	.suggestion(_PERMISSIONS_HELPER_EXPERIMENTAL_SUGGESTION)?;
	if has_created_proxy_user_before(
		client,
		container.get_container_name(),
		container.get_base_user(),
	)
	.await
	.suggestion(_PERMISSIONS_HELPER_EXPERIMENTAL_SUGGESTION)?
	{
		return Ok(());
	}
	create_permissions_proxy_user(
		client,
		container.get_container_name(),
		container.get_base_user(),
		*forced_user_id,
		*forced_group_id,
		has_sudo,
	)
	.await
	.suggestion(_PERMISSIONS_HELPER_EXPERIMENTAL_SUGGESTION)?;

	// Allow the user to sudo, if sudo is installed.
	if has_sudo {
		allow_proxy_user_to_sudo(
			client,
			container.get_container_name(),
			container.get_base_user(),
		)
		.await
		.suggestion(_PERMISSIONS_HELPER_EXPERIMENTAL_SUGGESTION)?;
	}

	Ok(())
}

/// Perform a very simple execute and wait for command to finish.
async fn execute_and_wait_simple(
	client: &HttpClient,
	container_name: &str,
	user: &str,
	command: &[String],
) -> Result<String> {
	execute_command_in_container(
		client,
		container_name,
		command,
		&[],
		user,
		false,
		None,
		None,
	)
	.await
}

/// Check if a container has sudo installed.
async fn container_has_sudo(client: &HttpClient, container_name: &str, user: &str) -> Result<bool> {
	let sudo_execution_id = execute_and_wait_simple(
		client,
		container_name,
		user,
		&[
			"/usr/bin/env".to_owned(),
			"bash".to_owned(),
			"-c".to_owned(),
			"hash sudo".to_owned(),
		],
	)
	.await
	.wrap_err(
		"Failure Checking for sudo existance inside docker container for permissions helper.",
	)?;

	Ok(get_command_exit_code(client, &sudo_execution_id)
		.await
		.wrap_err(
			"Failure Checking for sudo existance inside docker container for permissions helper.",
		)? == 0)
}

/// Check if this user has already created the dev-loop permissions helper user.
async fn has_created_proxy_user_before(
	client: &HttpClient,
	container_name: &str,
	user: &str,
) -> Result<bool> {
	let user_exist_id = execute_and_wait_simple(
		client,
		container_name,
		user,
		&[
			"/usr/bin/env".to_owned(),
			"bash".to_owned(),
			"-c".to_owned(),
			"getent passwd dl".to_owned(),
		],
	)
	.await
	.wrap_err("Failure checking if user has already been created for permissions helper.")?;

	if get_command_exit_code(client, &user_exist_id)
		.await
		.wrap_err("Failure checking if user has already been created for permissions helper.")?
		== 0
	{
		Ok(true)
	} else {
		Ok(false)
	}
}

/// Create a user in the docker container that has the same user id/group id
/// as the user on the host so we can proxy permissions.
async fn create_permissions_proxy_user(
	client: &HttpClient,
	container_name: &str,
	user: &str,
	forced_user_id: u32,
	forced_group_id: u32,
	has_sudo: bool,
) -> Result<()> {
	let creation_execution_id = match (user == "root", has_sudo) {
		(true, _) | (false, false) => execute_and_wait_simple(
			client,
			container_name,
			user,
			&[
				"/usr/bin/env".to_owned(),
				"bash".to_owned(),
				"-c".to_owned(),
				format!(
					"groupadd -g {} -o dl && useradd -u {} -g {} -o -c '' -m dl",
					forced_group_id, forced_user_id, forced_group_id
				),
			],
		)
		.await
		.wrap_err("Failure creating user for permissions helper")?,
		(false, true) => execute_and_wait_simple(
			client,
			container_name,
			user,
			&[
				"/usr/bin/env".to_owned(),
				"bash".to_owned(),
				"-c".to_owned(),
				format!(
					"sudo -n groupadd -g {} -o dl && sudo -n useradd -u {} -g {} -o -c '' -m dl",
					forced_group_id, forced_user_id, forced_group_id
				),
			],
		)
		.await
		.wrap_err("Failure creating user for permissions helper")?,
	};

	if get_command_exit_code(client, &creation_execution_id).await? != 0 {
		return Err(eyre!(
			"Failed to get successful ExitCode from docker on user creation for permissions helper"
		));
	}

	Ok(())
}

/// Allow the permissions proxy user to sudo.
async fn allow_proxy_user_to_sudo(
	client: &HttpClient,
	container_name: &str,
	user: &str,
) -> Result<()> {
	let sudo_user_creation_id = if user == "root" {
		execute_and_wait_simple(
      client,
  container_name,
  user,
  &[
      "/usr/bin/env".to_owned(),
      "bash".to_owned(),
      "-c".to_owned(),
      "mkdir -p /etc/sudoers.d && echo \"dl ALL=(root) NOPASSWD:ALL\" > /etc/sudoers.d/dl && chmod 0440 /etc/sudoers.d/dl".to_owned()
    ],
    ).await.wrap_err("Failure adding user to sudoers for permissions helper")?
	} else {
		execute_and_wait_simple(
      client,
      container_name,
      user,
      &[
        "/usr/bin/env".to_owned(),
        "bash".to_owned(),
        "-c".to_owned(),
        "sudo -n mkdir -p /etc/sudoers.d && echo \"dl ALL=(root) NOPASSWD:ALL\" | sudo -n tee /etc/sudoers.d/dl && sudo -n chmod 0440 /etc/sudoers.d/dl".to_owned()
      ],
    ).await.wrap_err("Failure adding user to sudoers for permissions helper")?
	};

	if get_command_exit_code(client, &sudo_user_creation_id)
		.await
		.wrap_err("Failure adding user to sudoers for permissions helper.")?
		!= 0
	{
		return Err(eyre!(
			"Failed to setup passwordless sudo access for permissions helper!"
		));
	}

	Ok(())
}
