use super::{
	docker_api_delete, docker_api_get, docker_api_post, download_image,
	execute_command_in_container, get_command_exit_code, setup_permission_helper,
	DockerContainerInfo,
};

use color_eyre::{
	eyre::{eyre, WrapErr},
	Result, Section,
};
use isahc::HttpClient;

/// List all the devloop containers.
pub async fn list_devloop_containers(client: &HttpClient) -> Result<Vec<String>> {
	let resp = docker_api_get(
		client,
		"/containers/json?all=true",
		"Taking awhile to query containers from docker. Will wait up to 30 seconds.".to_owned(),
		None,
		true,
	)
	.await?;
	let mut container_names = Vec::new();

	if let Some(containers) = resp.as_array() {
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

			container_names.push(dl_name);
		}
	}

	Ok(container_names)
}

pub async fn delete_container(client: &HttpClient, container_name: &str) {
	let _ = docker_api_post(
		client,
		&format!("/containers{}/kill", container_name),
		"Docker is not killing the container in a timely manner. Will wait up to 30 seconds."
			.to_owned(),
		None,
		None,
		false,
	)
	.await;
	let _ = docker_api_delete(
		&client,
		&format!("/containers{}?v=true&force=true&link=true", container_name),
		"Docker is taking awhile to remove the container. Will wait up to 30 seconds.".to_owned(),
		None,
		None,
		false,
	)
	.await;
}

/// Determine if the container is created, and then if it's wondering.
///
/// # Errors
///
/// Errors when the docker api cannot be talked to.
pub async fn is_container_created_and_running(
	client: &HttpClient,
	container_name: &str,
) -> Result<(bool, bool)> {
	let url = format!("/containers/{}/json?size=false", container_name);
	let mut is_created = false;
	let mut is_running = false;

	// Ignore errors since a 404 for no container is an Error.
	if let Ok(value) = docker_api_get(
		client,
		&url,
		"Taking awhile to query container status from docker. Will wait up to 30 seconds."
			.to_owned(),
		None,
		true,
	)
	.await
	{
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
pub async fn create_container(
	client: &HttpClient,
	project_root: &str,
	tmp_dir: &str,
	docker_container: &DockerContainerInfo,
) -> Result<()> {
	let mut mounts = Vec::new();
	mounts.push(serde_json::json!({
		"Source": project_root,
		"Target": "/mnt/dl-root",
		"Type": "bind",
		"Consistency": "consistent",
	}));
	mounts.push(serde_json::json!({
		"Source": tmp_dir,
		"Target": "/tmp",
		"Type": "bind",
		"Consistency": "consistent",
	}));
	for emount in docker_container.get_extra_mounts() {
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

	for tcp_port in docker_container.get_tcp_ports_to_expose() {
		port_mapping.insert(format!("{}/tcp", tcp_port), serde_json::json!({}));
		host_config_mapping.insert(
			format!("{}/tcp", tcp_port),
			serde_json::json!([serde_json::json!({ "HostPort": format!("{}", tcp_port) }),]),
		);
	}
	for udp_port in docker_container.get_udp_ports_to_expose() {
		port_mapping.insert(format!("{}/udp", udp_port), serde_json::json!({}));
		host_config_mapping.insert(
			format!("{}/udp", udp_port),
			serde_json::json!([serde_json::json!({ "HostPort": format!("{}", udp_port) }),]),
		);
	}

	let url = format!(
		"/containers/create?name={}",
		docker_container.get_container_name()
	);
	let body = serde_json::json!({
		"Cmd": ["tail", "-f", "/dev/null"],
		"Entrypoint": "",
		"Image": docker_container.get_image(),
		"Hostname": docker_container.get_hostname(),
		"User": docker_container.get_base_user(),
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
	let _ = docker_api_post(
		client,
		&url,
		"Docker is not creating the container in a timely manner. Will wait up to 30 seconds."
			.to_owned(),
		Some(body),
		None,
		false,
	)
	.await
	.wrap_err("Failed to create the docker container")?;

	Ok(())
}

/// Ensure the docker container exists.
///
/// # Errors
///
/// If we cannot talk to the docker socket, or there is a conflict creating the container.
pub async fn ensure_docker_container(
	client: &HttpClient,
	project_root: &str,
	tmp_dir: &str,
	container: &DockerContainerInfo,
) -> Result<()> {
	let image_exists_url = format!("/images/{}/json", container.get_image());
	let image_exists = docker_api_get(
		client,
		&image_exists_url,
		"Taking awhile to query if image is downloaded from docker. Will wait up to 30 seconds."
			.to_owned(),
		None,
		false,
	)
	.await
	.wrap_err("Failed to check if image has downloaded.")
	.is_ok();

	if !image_exists {
		download_image(client, container.get_image()).await?;
	}
	let (container_exists, container_running) =
		is_container_created_and_running(client, container.get_container_name()).await?;

	if !container_exists {
		create_container(client, project_root, tmp_dir, container).await?;
	}

	if !container_running {
		let url = format!("/containers/{}/start", container.get_container_name());
		let _ = docker_api_post(
			client,
			&url,
			"Docker is taking awhile to start the container. Will wait up to 30 seconds."
				.to_owned(),
			None,
			None,
			false,
		)
		.await
		.wrap_err("Failed to tell docker to start running the Docker container")?;
	}

	let execution_id = execute_command_in_container(
		client,
		container.get_container_name(),
		&[
			"/usr/bin/env".to_owned(),
			"bash".to_owned(),
			"-c".to_owned(),
			"hash bash".to_owned(),
		],
		&[],
		container.get_base_user(),
		false,
		None,
		None,
	)
	.await
	.wrap_err("Failed to check for existance of bash in Docker container")?;

	let has_bash = get_command_exit_code(client, &execution_id).await?;
	if has_bash != 0 {
		return Err(eyre!(
			"Docker Image: [{}] does not have bash! This is required for dev-loop!",
			container.get_image(),
		))
		.note(format!(
			"To replicate you can run: `docker run --rm -it {} /usr/bin/env bash -c \"hash bash\"`",
			container.get_image()
		))
		.note(format!(
			"The container is also still running with the name: [{}]",
			container.get_container_name()
		));
	}

	setup_permission_helper(client, container).await?;

	Ok(())
}
