//! Contains any fetchers that can fetch content from any remote web server.

use crate::config::types::LocationConf;
use crate::fetch::{FetchedItem, Fetcher};
use anyhow::{anyhow, Result};
use async_std::future;
use async_std::path::PathBuf;
use isahc::prelude::*;
use std::time::Duration;

/// A fetcher that is capable of fetching from an http like endpoint.
#[derive(Debug)]
pub struct HttpFetcher {}

impl HttpFetcher {
	/// Create a new http fetcher.
	///
	/// # Errors
	///
	/// None for now, but kept for non-breaking as we evolve.
	pub fn new() -> Result<Self> {
		Ok(Self {})
	}

	#[must_use]
	pub fn fetches_for() -> String {
		"http".to_owned()
	}
}

#[async_trait::async_trait]
impl Fetcher for HttpFetcher {
	#[must_use]
	#[tracing::instrument]
	async fn fetch(&self, location: &LocationConf) -> Result<Vec<FetchedItem>> {
		if location.get_type() != "http" {
			return Err(anyhow!(
				"Location: [{:?}] was passed to HttpFetcher but is not a http reesult",
				location
			));
		}

		let dur = Duration::from_millis(30000);
		let resp_res_fut = future::timeout(dur, isahc::get_async(location.get_at())).await;
		if let Err(fut_err) = resp_res_fut {
			return Err(anyhow!(
				"Failed to read from location: [{}] due to: [{:?}]",
				location.get_at(),
				fut_err,
			));
		}
		let resp_res = resp_res_fut.unwrap();
		if let Err(resp_err) = resp_res {
			return Err(anyhow!(
				"Failed to read from location: [{}] due to: [{:?}]",
				location.get_at(),
				resp_err,
			));
		}
		let mut resp = resp_res.unwrap();
		let status_code = resp.status().as_u16();
		if status_code < 200 || status_code > 299 {
			return Err(anyhow!(
				"Location: [{}] returned status code: [{}] which is not in the 200-300 range.",
				location.get_at(),
				status_code
			));
		}

		let mut results = Vec::new();
		let string_res = resp.text();
		if let Err(string_err) = string_res {
			return Err(anyhow!(
				"Location: [{}] failed to ready body due to: [{:?}]",
				location.get_at(),
				string_err,
			));
		}
		let string = string_res.unwrap();
		let bytes = Vec::from(string.as_bytes());
		results.push(FetchedItem::new(
			bytes,
			"http".to_owned(),
			location.get_at().to_owned(),
		));

		Ok(results)
	}

	#[must_use]
	async fn fetch_filter(
		&self,
		location: &LocationConf,
		_: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		self.fetch(location).await
	}

	#[must_use]
	async fn fetch_with_root_and_filter(
		&self,
		location: &LocationConf,
		_: &PathBuf,
		_: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		self.fetch(location).await
	}
}
