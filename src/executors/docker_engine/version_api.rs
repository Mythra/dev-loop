use super::docker_api_get;

use color_eyre::Result;
use isahc::HttpClient;
use serde_json::Value as JsonValue;

pub async fn docker_version_check(client: &HttpClient) -> Result<JsonValue> {
	docker_api_get(
		client,
		"/version",
		"Taking awhile to query version from docker. Will wait up to 30 seconds.".to_owned(),
		None,
		true,
	)
	.await
}
