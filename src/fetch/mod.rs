//! The "fetcher" module is where all the implementations, and trait definition
//! lives for anything that "fetches" a particular location.
//!
//! So for example the `FilesystemFetcher` fetches data from a filesystem.
//! The `HttpFetcher` fetched data from a remote endpoint over http.

use crate::config::types::LocationConf;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;

/// Describes the result of a fetch. This is a two part response
/// containing the raw bytes it went, and fetched. Then on the other hand it
/// contains the source of where it fetched it from for error context.
#[derive(Clone, Debug)]
pub struct FetchedItem {
	/// The contents of whatever was fetched.
	contents: Vec<u8>,
	/// The fetcher that fetched this item.
	fetched_by: String,
	/// An end-user understood idea of where this item came from.
	source: String,
}

impl FetchedItem {
	/// Construct a new fetched item.
	///
	/// `contents`: The contents of the file.
	/// `fetched_by`: The fetcher that fetched this task.
	/// `source`: The source of where this came from.
	#[must_use]
	pub fn new(contents: Vec<u8>, fetched_by: String, source: String) -> Self {
		Self {
			contents,
			fetched_by,
			source,
		}
	}

	/// Get the contents of this fetched item.
	#[must_use]
	pub fn get_contents(&self) -> &[u8] {
		&self.contents
	}

	/// Get who fetched this particular item.
	#[must_use]
	pub fn get_fetched_by(&self) -> &str {
		&self.fetched_by
	}

	/// Get the source location.
	#[must_use]
	pub fn get_source(&self) -> &str {
		&self.source
	}
}

/// Describes a "fetcher", or something that can
/// fetch data from a particular location.
#[async_trait::async_trait]
pub trait Fetcher {
	/// Attempt to fetch an item from an actual location, will default to a filter that allows all.
	///
	/// `location`: The actual location to fetch.
	#[must_use]
	async fn fetch(&self, location: &LocationConf) -> Result<Vec<FetchedItem>>;

	/// Attempt to fetch an item from an actual location.
	///
	/// `location`: The actual location to fetch.
	/// `filter_filename`: The filename to filter by (e.g. only task files).
	#[must_use]
	async fn fetch_filter(
		&self,
		location: &LocationConf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>>;

	/// Attempt to fetch an item from an actual location.
	///
	/// `location`: The actual location to fetch.
	/// `root_dir`: The root directory to relatively fetch from if it's a path.
	/// `filter_filename`: The filename to filter by (e.g. only task files).
	#[must_use]
	async fn fetch_with_root_and_filter(
		&self,
		location: &LocationConf,
		root_dir: &PathBuf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>>;
}

pub mod fs;
pub mod remote;

/// Describes a repository of fetchers. Allows fetching from multiple
/// sources all at once.
pub struct FetcherRepository {
	/// `repo`: The repository of fetchers.
	repo: Arc<HashMap<String, Box<dyn Fetcher + Sync + Send>>>,
}

impl Debug for FetcherRepository {
	fn fmt(&self, formatter: &mut Formatter) -> Result<(), std::fmt::Error> {
		let mut keys_str = String::new();
		for key in self.repo.keys() {
			keys_str += key;
			keys_str += ",";
		}

		formatter.write_str(&format!("FetcherRepository {}{}{}", "{", keys_str, "}"))
	}
}

impl FetcherRepository {
	/// Implement a new repository fetcher.
	///
	/// `project_root`: The root of this project.
	///
	/// # Errors
	///
	/// If creating any of the underlying fetchers fails.
	pub fn new(project_root: std::path::PathBuf) -> Result<Self> {
		let mut fetchers: HashMap<String, Box<dyn Fetcher + Sync + Send>> = HashMap::new();

		let fs_fetcher = fs::PathFetcher::new(project_root)?;
		let http_fetcher = remote::HttpFetcher::new()?;

		fetchers.insert(fs::PathFetcher::fetches_for(), Box::new(fs_fetcher));
		fetchers.insert(remote::HttpFetcher::fetches_for(), Box::new(http_fetcher));

		Ok(Self {
			repo: Arc::new(fetchers),
		})
	}
}

#[async_trait::async_trait]
impl Fetcher for FetcherRepository {
	#[must_use]
	async fn fetch(&self, location: &LocationConf) -> Result<Vec<FetchedItem>> {
		if !self.repo.contains_key(location.get_type()) {
			return Err(anyhow!(
				"Do not know how to get location type: [{}]",
				location.get_type()
			));
		}

		let fetcher = &self.repo[location.get_type()];
		fetcher.fetch(location).await
	}

	#[must_use]
	async fn fetch_filter(
		&self,
		location: &LocationConf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		if !self.repo.contains_key(location.get_type()) {
			return Err(anyhow!(
				"Do not know how to get location type: [{}]",
				location.get_type()
			));
		}

		let fetcher = &self.repo[location.get_type()];
		fetcher.fetch_filter(location, filter_filename).await
	}

	#[must_use]
	async fn fetch_with_root_and_filter(
		&self,
		location: &LocationConf,
		root_dir: &PathBuf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		if !self.repo.contains_key(location.get_type()) {
			return Err(anyhow!(
				"Do not know how to get location type: [{}]",
				location.get_type()
			));
		}

		let fetcher = &self.repo[location.get_type()];
		fetcher
			.fetch_with_root_and_filter(location, root_dir, filter_filename)
			.await
	}
}
