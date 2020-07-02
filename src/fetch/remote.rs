//! Contains any fetchers that can fetch content from any remote web server.

use crate::{
	config::types::{LocationConf, LocationType},
	fetch::FetchedItem,
	future_helper::timeout_with_log_msg,
};
use color_eyre::{eyre::eyre, section::help::Help, Result};
use isahc::prelude::*;
use std::time::Duration;

/// A fetcher that is capable of fetching from an http like endpoint.
#[derive(Default)]
pub struct HttpFetcher {}

impl HttpFetcher {
	/// Fetch a HTTP Location.
	///
	/// # Errors
	///
	/// - When an invalid location type is passed.
	/// - When we timed out reading from the endpoint.
	/// - When there was some HTTP Error reading from the endpoint.
	/// - When the endpoint didn't respond in the 2XX HTTP range.
	pub async fn fetch_http(&self, location: &LocationConf) -> Result<Vec<FetchedItem>> {
		if location.get_type() != &LocationType::HTTP {
			return Err(eyre!(
				"Internal-Error: Location: [{:?}] was passed to HttpFetcher but is not a http location.",
				location
			))
			.suggestion("Please report this as an issue, and include your configuration.");
		}

		let log_dur = Duration::from_secs(3);
		let dur = Duration::from_secs(30);

		let mut resp = timeout_with_log_msg(
			format!(
				"HTTP Response from location ({}) is taking awhile, will wait up to 30 seconds...",
				location.get_at()
			),
			log_dur,
			dur,
			isahc::get_async(location.get_at()),
		)
		.await
		.map_err(|_| {
			eyre!(
				"HTTP Location: [{}] failed to fetch data within 30 seconds",
				location.get_at(),
			)
		})?
		.map_err(|http_err| http_err)
		.note(format!("Attempted to fetch: [{}]", location.get_at()))?;

		let status_code = resp.status().as_u16();
		if status_code < 200 || status_code > 299 {
			return Err(eyre!(
				"HTTP Location: [{}] returned status code: [{}] which is not in the 200-300 range.",
				location.get_at(),
				status_code,
			));
		}

		let mut results = Vec::with_capacity(1);
		let string = resp.text()?;
		let bytes = Vec::from(string.as_bytes());
		results.push(FetchedItem::new(
			bytes,
			LocationType::HTTP,
			location.get_at().to_owned(),
		));

		Ok(results)
	}
}
