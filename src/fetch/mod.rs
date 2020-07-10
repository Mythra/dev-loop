//! The "fetcher" module is where all the implementations, and trait definition
//! lives for anything that "fetches" a particular location.
//!
//! So for example the `FilesystemFetcher` fetches data from a filesystem.
//! The `HttpFetcher` fetched data from a remote endpoint over http.

use crate::config::types::{LocationConf, LocationType};
use color_eyre::Result;
use std::{
	fmt::{Debug, Formatter},
	path::PathBuf,
};

/// Describes the result of a fetch. This is a two part response
/// containing the raw bytes it went, and fetched. Then on the other hand it
/// contains the source of where it fetched it from for error context.
#[derive(Clone, Debug)]
pub struct FetchedItem {
	/// The contents of whatever was fetched.
	contents: Vec<u8>,
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
	pub fn new(contents: Vec<u8>, source: String) -> Self {
		Self { contents, source }
	}

	/// Get the contents of this fetched item.
	#[must_use]
	pub fn get_contents(&self) -> &[u8] {
		&self.contents
	}

	/// Get the source location.
	#[must_use]
	pub fn get_source(&self) -> &str {
		&self.source
	}
}

pub(crate) mod fs;
pub(crate) mod remote;

/// A wrapper around all the fetchers at once, so you just have one type to
/// deal with.
pub struct FetcherRepository {
	http_fetcher: remote::HttpFetcher,
	path_fetcher: fs::PathFetcher,
	project_root: PathBuf,
}

impl Debug for FetcherRepository {
	fn fmt(&self, formatter: &mut Formatter) -> Result<(), std::fmt::Error> {
		formatter.write_str("FetcherRepository {http, path}")
	}
}

impl FetcherRepository {
	/// Implement a fetcher that can fetch from any location type.
	///
	/// # Errors
	///
	/// If creating any of the underlying fetchers fails.
	pub fn new(project_root: PathBuf) -> Result<Self> {
		let http_fetcher = remote::HttpFetcher::default();
		let path_fetcher = fs::PathFetcher::default();

		Ok(Self {
			http_fetcher,
			path_fetcher,
			project_root,
		})
	}

	/// Fetch from a particular location, while filtering on filename.
	///
	/// # Errors
	///
	/// - Bubbled error from underlying fetchers when there is an error fetching
	///   the item.
	pub async fn fetch_filter(
		&self,
		location: &LocationConf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		match *location.get_type() {
			LocationType::HTTP => self.http_fetcher.fetch_http(location).await,
			LocationType::Path => {
				self.path_fetcher
					.fetch_from_fs(
						location,
						&self.project_root,
						&self.project_root,
						filter_filename,
					)
					.await
			}
		}
	}

	/// Fetch from a particular location, while filtering on a filename, and
	/// specifying the directory to be relative at.
	///
	/// # Errors
	///
	/// - Bubbled error from underlying fetchers when there is an error fetching
	///   the item.
	pub async fn fetch_with_root_and_filter(
		&self,
		location: &LocationConf,
		root_dir: &PathBuf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		match *location.get_type() {
			LocationType::HTTP => self.http_fetcher.fetch_http(location).await,
			LocationType::Path => {
				self.path_fetcher
					.fetch_from_fs(location, &self.project_root, root_dir, filter_filename)
					.await
			}
		}
	}
}
