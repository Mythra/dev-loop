//! Represents interacting with the Docker Engine API.

use crate::future_helper::timeout_with_log_msg;

use color_eyre::{
	eyre::{eyre, WrapErr},
	Result, Section,
};
use isahc::{http::request::Request, prelude::*, Body, HttpClient};
use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use std::time::Duration;
use tracing::debug;

/// This is the api version we use for talking to the docker socket.
///
/// The docker socket allows us to choose a versioned api like this, which is
/// why we use it as opposed to using a terminal command (not to mention we
/// don't have to worry about escaping correctly).
///
/// `v1.40` is chosen because as of the time of writing this
/// `v1.40` is the version for Docker Engine 19.03, which at
/// the time of writing this (July 3rd, 2020) is the lowest supported
/// version according to docker:
///
/// <https://success.docker.com/article/compatibility-matrix>
///
/// We can bump this in the future when we know it won't run into anyone.
const DOCKER_API_VERSION: &str = "/v1.40";
const DOCKER_STATUS_CODES_ERR_NOTE: &str = "To find out what the status code means you can check the Docker documentation: https://docs.docker.com/engine/api/v1.40/.";

cfg_if::cfg_if! {
  if #[cfg(unix)] {
		pub const SOCKET_PATH: &str = "/var/run/docker.sock";
  } else if #[cfg(win)] {
		// TODO(xxx): named pipes? url?
		pub const SOCKET_PATH: &str = "UNIMPLEMENTED";
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

async fn docker_api_call<B: Into<Body>>(
	client: &HttpClient,
	req: Request<B>,
	long_call_msg: String,
	timeout: Option<Duration>,
	is_json: bool,
) -> Result<JsonValue> {
	let _guard = DOCK_SOCK_LOCK.lock().await;
	let uri = req.uri().to_string();

	let log_timeout = Duration::from_secs(3);
	let timeout_frd = timeout.unwrap_or_else(|| Duration::from_secs(30));
	let mut resp = timeout_with_log_msg(
		long_call_msg,
		log_timeout,
		timeout_frd,
		client.send_async(req),
	)
	.await??;

	let status = resp.status().as_u16();
	if status < 200 || status > 299 {
		return Err(eyre!(
			"Docker responded with a status code: [{}] which is not in the 200-300 range.",
			status,
		))
		.note(DOCKER_STATUS_CODES_ERR_NOTE)
		.context(uri);
	}

	if is_json {
		Ok(resp
			.json()
			.wrap_err("Failure to response Docker response as JSON")
			.context(uri)?)
	} else {
		// Ensure the response body is read in it's entirerty. Otherwise
		// the body could still be writing, but we think we're done with the
		// request, and all of a sudden we're writing to a socket while
		// a response body is all being written and it's all bad.
		let _ = resp.text();
		Ok(serde_json::Value::default())
	}
}

/// Call the docker engine api using the GET http method.
///
/// `client`: the http client to use.
/// `path`: the path to call (along with Query Args).
/// `long_call_msg`: the message to print when docker is taking awhile to respond.
/// `timeout`: The optional timeout. Defaults to 30 seconds.
/// `is_json`: whether or not to parse the response as json.
pub(self) async fn docker_api_get(
	client: &HttpClient,
	path: &str,
	long_call_msg: String,
	timeout: Option<Duration>,
	is_json: bool,
) -> Result<JsonValue> {
	let url = format!("http://localhost{}{}", DOCKER_API_VERSION, path);
	debug!("URL for get will be: {}", url);
	let req = Request::get(url)
		.header("Accept", "application/json; charset=UTF-8")
		.header("Content-Type", "application/json; charset=UTF-8")
		.body(())
		.wrap_err("Internal-Error: Failed to construct http request.")
		.suggestion("Please report this as an issue so it can be fixed.")?;

	docker_api_call(client, req, long_call_msg, timeout, is_json)
		.await
		.context(format!("URL: {}", path))
}

/// Call the docker engine api using the POST http method.
///
/// `client`: the http client to use.
/// `path`: the path to call (along with Query Args).
/// `long_call_msg`: the message to print when docker is taking awhile to respond.
/// `body`: The body to send to the remote endpoint.
/// `timeout`: the optional timeout. Defaults to 30 seconds.
/// `is_json`: whether to attempt to read the response body as json.
pub(self) async fn docker_api_post(
	client: &HttpClient,
	path: &str,
	long_call_msg: String,
	body: Option<serde_json::Value>,
	timeout: Option<Duration>,
	is_json: bool,
) -> Result<JsonValue> {
	let url = format!("http://localhost{}{}", DOCKER_API_VERSION, path);
	debug!("URL for post will be: {}", url);
	let req_part = Request::post(url)
		.header("Accept", "application/json; charset=UTF-8")
		.header("Content-Type", "application/json; charset=UTF-8")
		.header("Expect", "");
	let req = if let Some(body_data) = body {
		req_part
			.body(
				serde_json::to_vec(&body_data)
					.wrap_err("Failure converting HTTP Request Body to JSON")
					.suggestion("This is an internal error, please report this issue.")?,
			)
			.wrap_err("Failed to write body to request")
			.suggestion("This is an internal error, please report this issue.")?
	} else {
		req_part
			.body(Vec::new())
			.wrap_err("Failed to write body to request")
			.suggestion("This is an internal error, please report this issue.")?
	};

	docker_api_call(client, req, long_call_msg, timeout, is_json)
		.await
		.context(format!("URL: {}", path))
}

/// Call the docker engine api using the POST http method.
///
/// `client`: the http client to use.
/// `path`: the path to call (along with Query Args).
/// `long_call_msg`: the message to print when docker is taking awhile to respond.
/// `body`: The body to send to the remote endpoint.
/// `timeout`: the timeout for this requests, defaults to 30 seconds.
/// `is_json`: whether to actually try to read the response body as json.
pub(self) async fn docker_api_delete(
	client: &HttpClient,
	path: &str,
	long_call_msg: String,
	body: Option<serde_json::Value>,
	timeout: Option<Duration>,
	is_json: bool,
) -> Result<JsonValue> {
	let url = format!("http://localhost{}{}", DOCKER_API_VERSION, path);
	debug!("URL for delete will be: {}", url);
	let req_part = Request::delete(url)
		.header("Accept", "application/json; charset=UTF-8")
		.header("Content-Type", "application/json; charset=UTF-8")
		.header("Expect", "");
	let req = if let Some(body_data) = body {
		req_part
			.body(
				serde_json::to_vec(&body_data)
					.wrap_err("Failure converting HTTP Request Body to JSON")
					.suggestion("This is an internal error, please report this issue.")?,
			)
			.wrap_err("Failed to write body to request")
			.suggestion("This is an internal error, please report this issue.")?
	} else {
		req_part
			.body(Vec::new())
			.wrap_err("Failed to write body to request")
			.suggestion("This is an internal error, please report this issue.")?
	};

	docker_api_call(client, req, long_call_msg, timeout, is_json)
		.await
		.context(format!("URL: {}", path))
}

pub(crate) mod container;
pub(crate) mod container_api;
pub(crate) mod execution_api;
pub(crate) mod image_api;
pub(crate) mod network_api;
pub(crate) mod permissions_helper;
pub(crate) mod version_api;

pub use container::*;
pub use container_api::*;
pub use execution_api::*;
pub use image_api::*;
pub use network_api::*;
pub use permissions_helper::*;
pub use version_api::*;
