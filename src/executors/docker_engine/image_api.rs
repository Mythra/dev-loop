use super::docker_api_post;

use color_eyre::{eyre::WrapErr, Result};
use isahc::HttpClient;
use std::time::Duration;

/// Download the Image for this docker executor.
///
/// # Errors
///
/// Errors when it the docker api cannot be talked too, or the image cannot be downloaded.
pub async fn download_image(client: &HttpClient, image: &str) -> Result<()> {
	let image_tag_split = image.rsplitn(2, ':').collect::<Vec<&str>>();
	let (image_name, tag_name) = if image_tag_split.len() == 2 {
		(image_tag_split[1], image_tag_split[0])
	} else {
		(image_tag_split[0], "latest")
	};
	let url = format!("/images/create?fromImage={}&tag={}", image_name, tag_name);

	let _ = docker_api_post(
		client,
		&url,
		format!(
			"Downloading the docker_image: [{}:{}]",
			image_name, tag_name
		),
		None,
		Some(Duration::from_secs(3600)),
		false,
	)
	.await
	.wrap_err(format!(
		"Failed to download image: [{}:{}]",
		image_name, tag_name
	))?;

	Ok(())
}
