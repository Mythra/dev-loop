use super::{docker_api_get, docker_api_post};

use color_eyre::{
	eyre::{eyre, WrapErr},
	Result, Section,
};
use isahc::HttpClient;
use std::{convert::TryFrom, time::Duration};

/// Execute a command, and return the "execution id" to check back on it.
///
/// # Errors
///
/// Errors if we fail to create an exec instance with docker.
#[allow(clippy::too_many_arguments)]
pub async fn execute_command_in_container_async(
	client: &HttpClient,
	container_name: &str,
	command: &[String],
	environment_to_export: &[String],
	user: &str,
	needs_forced_ids: bool,
	force_user_id: Option<u32>,
	force_group_id: Option<u32>,
) -> Result<String> {
	let url = format!("/containers/{}/exec", container_name);
	let body = if needs_forced_ids && (force_user_id.is_some() && force_group_id.is_some()) {
		serde_json::json!({
			"AttachStdout": true,
			"AttachStderr": true,
			"Tty": false,
			"User": &format!("{}:{}", force_user_id.unwrap(), force_group_id.unwrap()),
			"Privileged": true,
			"Cmd": command,
			"Env": environment_to_export,
		})
	} else {
		serde_json::json!({
			"AttachStdout": true,
			"AttachStderr": true,
			"Tty": false,
			"User": user,
			"Privileged": true,
			"Cmd": command,
			"Env": environment_to_export,
		})
	};

	let resp = docker_api_post(
		client,
		&url,
		"Docker is taking awhile to start running a new command. Will wait up to 30 seconds."
			.to_owned(),
		Some(body),
		None,
		true,
	)
	.await
	.wrap_err("Failed to send new command to Docker container")?;

	let potential_id = &resp["Id"];
	if !potential_id.is_string() {
		return Err(eyre!(
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

	let _ = docker_api_post(
		client,
		&start_url,
		"Docker is taking awhile to start running a new command. Will wait up to 30 seconds."
			.to_owned(),
		Some(start_body),
		None,
		false,
	)
	.await
	.wrap_err("Failed to tell Docker container to start executing command")?;

	Ok(exec_id)
}

/// Determine if a particular execution ID has finished executing.
pub async fn has_command_finished(client: &HttpClient, execution_id: &str) -> bool {
	let url = format!("/exec/{}/json", execution_id);
	let resp_res = docker_api_get(
		client,
		&url,
		"Taking awhile to determine if command has finished running in docker. Will wait up to 30 seconds.".to_owned(),
		None,
		true
	).await;
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

/// Execute a raw command, and wait til it's finished. Returns execution id so you can checkup on it.
///
/// # Errors
///
/// Errors if we cannot talk to docker to create an exec instance.
#[allow(clippy::too_many_arguments)]
pub async fn execute_command_in_container(
	client: &HttpClient,
	container_name: &str,
	command: &[String],
	environment_to_export: &[String],
	user: &str,
	needs_forced_ids: bool,
	force_user_id: Option<u32>,
	force_group_id: Option<u32>,
) -> Result<String> {
	let execution_id = execute_command_in_container_async(
		client,
		container_name,
		command,
		environment_to_export,
		user,
		needs_forced_ids,
		force_user_id,
		force_group_id,
	)
	.await?;

	loop {
		if has_command_finished(client, &execution_id).await {
			break;
		}

		async_std::task::sleep(Duration::from_micros(10)).await;
	}

	Ok(execution_id)
}

/// Get the exit code for a particular execution
///
/// # Errors
///
/// If we cannot find an `ExitCode` in the docker response, or talk to the docker socket.
pub async fn get_command_exit_code(client: &HttpClient, execution_id: &str) -> Result<i32> {
	let url = format!("/exec/{}/json", execution_id);
	let resp = docker_api_get(
		client,
		&url,
		"Taking awhile to query exit code of command from docker. Will wait up to 30 seconds."
			.to_owned(),
		None,
		true,
	)
	.await
	.wrap_err("Failed to query exit code from Docker")?;
	let exit_code_opt = &resp["ExitCode"];
	if !exit_code_opt.is_i64() {
		return Err(eyre!(
			"Failed to find integer ExitCode in response: [{:?}]",
			resp,
		))
		.wrap_err("Failure querying exit code")
		.suggestion("This is an internal error, please file an issue.");
	}

	Ok(i32::try_from(exit_code_opt.as_i64().unwrap())
		.ok()
		.unwrap_or(255))
}
