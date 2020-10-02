use super::{docker_api_delete, docker_api_get, docker_api_post};

use color_eyre::{eyre::WrapErr, Result, Section};
use isahc::HttpClient;
use once_cell::sync::Lazy;
use tracing::error;

static NETWORK_CREATION_LOCK: Lazy<async_std::sync::Mutex<()>> =
	Lazy::new(|| async_std::sync::Mutex::new(()));
static NETWORK_ATTACH_LOCK: Lazy<async_std::sync::Mutex<()>> =
	Lazy::new(|| async_std::sync::Mutex::new(()));

pub async fn list_devloop_networks(client: &HttpClient) -> Result<Vec<String>> {
	let json_networks = docker_api_get(
		&client,
		"/networks",
		"Taking ahwile to query networks from docker. Will wait up til 30 seconds.".to_owned(),
		None,
		true,
	)
	.await?;

	let mut devloop_networks = Vec::new();
	if let Some(networks) = json_networks.as_array() {
		for network in networks {
			if let Some(name_untyped) = network.get("Name") {
				if let Some(name_str) = name_untyped.as_str() {
					if name_str.starts_with("dl-") {
						devloop_networks.push(name_str.to_owned());
					}
				}
			}
		}
	}

	Ok(devloop_networks)
}

pub async fn delete_network(client: &HttpClient, network: &str) {
	let err = docker_api_delete(
		&client,
		&format!("/networks/{}", network),
		"Docker is taking awhile to delete a docker network. Will wait up to 30 seconds."
			.to_owned(),
		None,
		None,
		false,
	)
	.await;

	if err.is_err() {
		let formatted_err = err
			.wrap_err(format!("Failed to delete docker network: [{}]", network))
			.suggestion(format!(
				"You can try deleting the network manually with: `docker network rm {}`",
				network
			))
			.unwrap_err();

		error!("{:?}", formatted_err,);
	}
}

/// Ensure a particular network exists.
///
/// # Errors
///
/// If we cannot talk to the docker socket, or there is an error creating the network.
pub async fn ensure_network_exists(client: &HttpClient, pipeline_id: &str) -> Result<()> {
	let _guard = NETWORK_CREATION_LOCK.lock().await;

	let network_id = format!("dl-{}", pipeline_id);
	let network_url = format!("/networks/{}", network_id);
	let res = docker_api_get(
		client,
		&network_url,
		"Taking awhile to query network existance status from docker. Will wait up to 30 seconds."
			.to_owned(),
		None,
		true,
	)
	.await
	.wrap_err(format!(
		"Failed to query network information: {}",
		network_id
	));

	if res.is_err() {
		let json_body = serde_json::json!({
			"Name": network_id,
		});

		let _ = docker_api_post(
			client,
			"/networks/create",
			"Docker is not creating the network in a timely manner. Will wait up to 30 seconds."
				.to_owned(),
			Some(json_body),
			None,
			false,
		)
		.await
		.wrap_err(format!("Failed to create docker network: {}", network_id))?;
	}

	Ok(())
}

/// Determine if the container is attached to a particular network.
///
/// If there were any errors we'll just return false.
async fn is_network_attached(client: &HttpClient, container_name: &str, pipeline_id: &str) -> bool {
	let url = format!("/containers/{}/json", container_name);
	let body_res = docker_api_get(
		&client,
		&url,
		"Taking awhile to get container status from docker. Will wait up to 30 seconds.".to_owned(),
		None,
		true,
	)
	.await;
	if body_res.is_err() {
		return false;
	}
	let body = body_res.unwrap();
	let id_as_opt = body["Id"].as_str();
	if id_as_opt.is_none() {
		return false;
	}
	let id = id_as_opt.unwrap();

	let network_url = format!("/networks/dl-{}", pipeline_id);
	let network_body_res = docker_api_get(
		client,
		&network_url,
		"Taking awhile to get network status from docker. Will wait up to 30 seconds.".to_owned(),
		None,
		true,
	)
	.await;
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
/// # Errors
///
/// If we fail to talk to the docker socket, or connect the container to the network.
pub async fn ensure_network_attached(
	client: &HttpClient,
	container_name: &str,
	hostname: &str,
	pipeline_id: &str,
) -> Result<()> {
	let _guard = NETWORK_ATTACH_LOCK.lock().await;

	if !is_network_attached(client, container_name, pipeline_id).await {
		let url = format!("/networks/dl-{}/connect", pipeline_id);
		let body = serde_json::json!({
			"Container": container_name,
			"EndpointConfig": {
				"Aliases": [hostname],
			}
		});

		let _ = docker_api_post(
			client,
			&url,
			"Docker is taking awhile to attach a network to the container. Will wait up to 30 seconds.".to_owned(),
			Some(body),
			None,
			false
		).await
		 .wrap_err("Failed to attach network to Docker Container.")?;
	}

	Ok(())
}
