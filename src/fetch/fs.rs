//! Contains any fetchers that can fetch content from the filesystem.

use crate::config::types::LocationConf;
use crate::fetch::{FetchedItem, Fetcher};
use anyhow::{anyhow, Result};
use async_std::fs::{canonicalize, File};
use async_std::path::PathBuf;
use async_std::prelude::*;
use std::fs::read_dir;
use tracing::trace;

/// Handles all fetching based on the 'path' directive.
///
/// In effect this only allows fetching from relative directories,
/// or from the $HOME directory (referred to as: `~`). The reasoning
/// for this is because we don't want users writing config that will
/// immediately break on someone elses machine (who doesn't have the
/// same path). This does mean it may be harder to get stood up in some cases,
/// but the end result will be better.
#[derive(Debug)]
pub struct PathFetcher {
	/// The root of the current project-directory.
	///
	/// Unless otherwise specified, this is the relative directory we will fetch from.
	project_root: PathBuf,
}

/// Deteremines if a path is a child of a parent.
///
/// `parent` - the parent path.
/// `child` - the child to check if a child of the parent.
#[must_use]
pub fn path_is_child_of_parent(parent: &PathBuf, child: &PathBuf) -> bool {
	let parent_as_str = parent.to_str();
	let child_as_str = child.to_str();

	if parent_as_str.is_none() || child_as_str.is_none() {
		return false;
	}

	let parent_str = parent_as_str.unwrap();
	let child_str = child_as_str.unwrap();

	child_str.starts_with(parent_str)
}

/// Iterate a directory, getting all possible directory entries.
///
/// `dir`: the directory to iterate over.
/// `should_recurse`: if we should recursively look at this directory.
#[must_use]
#[tracing::instrument]
pub fn iterate_directory(dir: &PathBuf, should_recurse: bool) -> Result<Vec<PathBuf>> {
	let mut results = Vec::new();

	let entries = read_dir(dir)?;
	for entry in entries {
		let found_path = entry?.path();

		if found_path.is_dir() && should_recurse {
			let apb = PathBuf::from(found_path);
			let new_results = iterate_directory(&apb, should_recurse)?;
			results.extend(new_results);
		} else if found_path.is_file() {
			results.push(PathBuf::from(found_path));
		} else {
			trace!(
				"Skipping directory because not recursing, or non-file: [{:?}]",
				found_path
			);
		}
	}

	Ok(results)
}

/// Read a particular path as a FetchedItem.
///
/// `file`: The file to attempt to read into a fetched item.
#[must_use]
#[tracing::instrument]
async fn read_path_as_item(file: PathBuf) -> Result<FetchedItem> {
	let as_str = file.to_str();
	let source_location = if let Some(the_str) = as_str {
		the_str.to_owned()
	} else {
		"unknown".to_owned()
	};

	let mut fh = File::open(&file).await?;
	let mut contents = Vec::new();
	fh.read_to_end(&mut contents).await?;

	Ok(FetchedItem::new(
		contents,
		"path".to_owned(),
		source_location,
	))
}

impl PathFetcher {
	/// Create a new Fetcher that fetches from Paths.
	///
	/// `project_root_directory`: the root of the project.
	///
	/// # Errors
	///
	/// Will return error if project directory is not a full
	/// directory.
	pub fn new(project_root_directory: PathBuf) -> Result<Self> {
		if !project_root_directory.has_root() {
			return Err(anyhow!(
				"A PathFetcher needs a root directory that is absolute!"
			));
		}

		Ok(Self {
			project_root: project_root_directory,
		})
	}

	/// Fetch data from the filesystem, but manually specify where the "root" is. Can be used
	/// if you want to specify a different directory rather than the project directory.
	///
	/// `location`: the location to fetch from.
	/// `root_dir`: the root directory to fetch from
	/// `filter_filename`: the filename to potentially filter by.
	#[tracing::instrument]
	async fn internal_fetch(
		&self,
		location: &LocationConf,
		root_dir: &PathBuf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		if location.get_type() != "path" {
			return Err(anyhow!(
				"Location: [{:?}] was passed to PathFetcher but is not a path",
				location
			));
		}

		// We only allow fetching from within the repository.
		// This is because the "dl root dir" is thought of as
		// within a git/svn/etc. repo where the path can exist on everyones
		// machines.
		//
		// Running say a script from /usr/bin/blah is inherently un-repeatable.
		// Within an actual bash script it's okay because that bash script may
		// be running in docker or remotely which may always have that tool there.
		let mut built_path = root_dir.clone();
		built_path.push(location.get_at());
		let canonicalized = canonicalize(built_path).await?;
		if !path_is_child_of_parent(&self.project_root, &canonicalized) {
			return Err(anyhow!(
				"Path: [{:?}] is not a child of project directory: [{:?}], which is required for PathFetcher, so we can ensure it exists everywhere.",
				&canonicalized,
				&self.project_root,
			));
		}

		let mut results = Vec::new();

		if canonicalized.is_dir().await {
			let path_entries = iterate_directory(&canonicalized, location.get_recurse())?;

			if let Some(ffn) = filter_filename {
				let mut futures = Vec::new();

				for file_to_read in path_entries {
					if let Some(utf8_str) = file_to_read.to_str() {
						if utf8_str.ends_with(&ffn) {
							futures.push(read_path_as_item(file_to_read));
						}
					}
				}

				for future in futures {
					results.push(future.await?);
				}
			} else {
				let mut futures = Vec::new();

				for file_to_read in path_entries {
					futures.push(read_path_as_item(file_to_read));
				}

				for future in futures {
					results.push(future.await?);
				}
			}
		} else if canonicalized.is_file().await {
			results.push(read_path_as_item(canonicalized).await?);
		} else {
			return Err(anyhow!(
				"PathFetcher can only fetch file or directory, and the path: [{:?}] is not either.",
				canonicalized
			));
		}

		Ok(results)
	}

	#[must_use]
	pub fn fetches_for() -> String {
		"path".to_owned()
	}
}

#[async_trait::async_trait]
impl Fetcher for PathFetcher {
	#[must_use]
	async fn fetch(&self, location: &LocationConf) -> Result<Vec<FetchedItem>> {
		self.internal_fetch(location, &self.project_root, None)
			.await
	}

	#[must_use]
	async fn fetch_filter(
		&self,
		location: &LocationConf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		self.internal_fetch(location, &self.project_root, filter_filename)
			.await
	}

	#[must_use]
	async fn fetch_with_root_and_filter(
		&self,
		location: &LocationConf,
		root_dir: &PathBuf,
		filter_filename: Option<String>,
	) -> Result<Vec<FetchedItem>> {
		self.internal_fetch(location, root_dir, filter_filename)
			.await
	}
}
