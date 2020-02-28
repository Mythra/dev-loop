//! Contains the code for the "Host" executor, or the executor
//! that just uses the Host System.

use crate::config::types::NeedsRequirement;
use crate::executors::{CompatibilityStatus, Executor};
use crate::get_tmp_dir;
use crate::tasks::execution::preparation::ExecutableTask;
use anyhow::{anyhow, Result};
use async_std::fs::{read_dir, remove_dir_all};
use async_std::prelude::*;
use crossbeam_channel::Sender;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Represents the actual executor for the host system.
#[derive(Debug)]
pub struct HostExecutor {
	/// The root of the project, so we know where to "cd" into.
	project_root: String,
	/// The temporary directory to use.
	tmp_dir: PathBuf,
}

impl HostExecutor {
	/// Create a new host executor, with nothing but the project root.
	///
	/// # Errors
	///
	/// If the project root is not on a valid utf-8 string path.
	pub fn new(project_root: &PathBuf) -> Result<Self> {
		let tmp_dir = get_tmp_dir();
		let pr_as_string = project_root.to_str();
		if pr_as_string.is_none() {
			return Err(anyhow!("Failed to turn project root into a utf-8 string!"));
		}

		Ok(Self {
			project_root: pr_as_string.unwrap().to_owned(),
			tmp_dir,
		})
	}

	/// Performs a clean up of all host resources.
	#[allow(clippy::cognitive_complexity)]
	pub async fn clean() {
		info!("Starting Cleanup for the host executor");

		// To clean all we would possibly have leftover is files in $TMPDIR.
		// So we iterate through everything in the temporary directory...
		if let Ok(mut entries) = read_dir(get_tmp_dir()).await {
			while let Some(resulting_entry) = entries.next().await {
				// Did we get something?
				if let Ok(entry_de) = resulting_entry {
					let entry = entry_de.path();
					// If it's not a directory ignore it.
					if !entry.is_dir().await {
						debug!("Skipping entry, is not directory: [{:?}]", entry);
						continue;
					}
					// If it's not UTF-8 ignore it. We can't do a string comparison, and
					// we'd never write a non-utf-8 path anyway.
					let potential_str = entry.to_str();
					if potential_str.is_none() {
						debug!(
							"Skipping entry, cannot turn into a UTF-8 String: [{:?}]",
							entry,
						);
						continue;
					}
					let entry_str = potential_str.unwrap();

					// Is it a directory that ends with `-dl-host` the
					// identifier of dev-loop host executor?
					if !entry_str.ends_with("-dl-host") {
						debug!(
							"Skipping entry, does not appear to be a dev-loop temporary folder: [{:?}] (ends with -dl-host)",
							entry,
						);
						continue;
					}

					// If it is... remove the directory and everything underneath it.
					if let Err(remove_err) = remove_dir_all(&entry).await {
						error!(
							"Failed to cleanup temporary directory: [{:?}] due to : [{:?}] trying to continue.",
							entry,
							remove_err,
						);
					}
				}
			}
		}

		info!("Cleaned");
	}

	/// Determines if this `HostExecutor` is compatible with the system.
	#[must_use]
	pub fn is_compatible() -> CompatibilityStatus {
		// This command expands to: `bash -c "hash bash"`, while this may sound like
		// beating up a popular breakfast food it is actually a way to determine if
		// bash is capable of actually executing bash, using nothing but bash itself.
		//
		// This comes out of the fact that: `hash` is a bash builtin that shows the list
		// of recently used commands, and can take an argument to show a commands history.
		//
		// All in all this command works the best because:
		//
		//  we ensure the "bash" command on the host system has not been aliased
		//  to something like `sh` (e.g.: `alias bash="sh"`) which would break
		//  scripts potentially. if they do try to do this the "hash" command
		//  will not properly since it is not an actual binary.
		if Command::new("bash")
			.args(&["-c", "\"hash bash\""])
			.output()
			.is_ok()
		{
			CompatibilityStatus::Compatible
		} else {
			CompatibilityStatus::CouldBeCompatible("install bash".to_owned())
		}
	}
}

#[async_trait::async_trait]
impl Executor for HostExecutor {
	#[must_use]
	fn meets_requirements(&self, reqs: &[NeedsRequirement]) -> bool {
		let mut meets_reqs = true;

		for req in reqs {
			if req.get_name() != "host" {
				meets_reqs = false;
				break;
			}
		}

		meets_reqs
	}

	#[allow(
		clippy::cognitive_complexity,
		clippy::suspicious_else_formatting,
		clippy::too_many_lines,
		clippy::unnecessary_unwrap,
		unused_assignments
	)]
	#[must_use]
	async fn execute(
		&self,
		log_channel: Sender<(String, String, bool)>,
		should_stop: Arc<AtomicBool>,
		helper_src_line: &str,
		task: &ExecutableTask,
	) -> isize {
		// Execute a particular task:
		//
		//  1. Create a temporary directory for the pipeline id, and the task name.
		//  2. Write the task file the user specified.
		//  3. Write an "entrypoint" that sources in the helpers, and calls
		//     the script.
		//  4. Execute the script and wait for it to finish.

		info!("Executing task: [{}]", task.get_task_name());

		// Create the pipeline directory.
		let mut tmp_path = get_tmp_dir();
		tmp_path.push(task.get_pipeline_id().to_owned() + "-dl-host");
		let res = async_std::fs::create_dir_all(tmp_path.clone()).await;
		if let Err(dir_err) = res {
			error!(
				"Failed to create pipeline directory due to: [{:?}]",
				dir_err
			);
			return 10;
		}

		// Write the task file.
		let mut regular_task = tmp_path.clone();
		regular_task.push(task.get_task_name().to_owned() + ".sh");
		info!("Task writing to path: [{:?}]", regular_task);
		let write_res =
			async_std::fs::write(&regular_task, task.get_contents().get_contents()).await;
		if let Err(write_err) = write_res {
			error!("Failed to write script file due to: [{:?}]", write_err);
			return 10;
		}
		let path_as_str = regular_task.to_str().unwrap();

		// Write the entrypoint script.
		let entry_point_file = format!(
			"#!/usr/bin/env bash

cd {project_root}

# Source Helpers
{helper}

eval \"$(declare -F | sed -e 's/-f /-fx /')\"

{script} {arg_str}",
			project_root = self.project_root,
			helper = helper_src_line,
			script = path_as_str,
			arg_str = task.get_arg_string(),
		);
		tmp_path.push(task.get_task_name().to_owned() + "-entrypoint.sh");
		info!("Task entrypoint is being written too: [{:?}]", tmp_path);
		let write_res = async_std::fs::write(&tmp_path, entry_point_file).await;
		if let Err(write_err) = write_res {
			error!("Failed to write entrypoint file due to: [{:?}]", write_err);
			return 10;
		}

		if cfg!(target_os = "linux") {
			use std::os::unix::fs::PermissionsExt;
			let executable_permissions = std::fs::Permissions::from_mode(0o777);

			if let Err(exec_err) =
				std::fs::set_permissions(&tmp_path, executable_permissions.clone())
			{
				error!("Failed to mark entrypoint as executable: [{:?}]", exec_err);
			}
			if let Err(exec_err) = std::fs::set_permissions(&regular_task, executable_permissions) {
				error!("Failed to mark task file as executable: [{:?}]", exec_err);
			}
		}

		let entrypoint_as_str = tmp_path.to_str().unwrap();

		// Spawn the task.
		let command_res = Command::new(entrypoint_as_str)
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn();
		if let Err(command_err) = command_res {
			error!("Failed to spawn bash command: [{:?}]", command_err);
			return 10;
		}
		let mut child = command_res.unwrap();

		let has_finished = Arc::new(AtomicBool::new(false));

		let mut child_stdout = BufReader::new(child.stdout.take().unwrap());
		let mut child_stderr = BufReader::new(child.stderr.take().unwrap());

		let flush_channel_clone = log_channel.clone();
		let flush_task_name = task.get_task_name().to_owned();
		let flush_has_finished_clone = has_finished.clone();

		let flush_task = async_std::task::spawn(async move {
			let mut line = String::new();

			while !flush_has_finished_clone.load(Ordering::Relaxed) {
				while let Ok(read) = child_stdout.read_line(&mut line) {
					if read == 0 {
						break;
					}

					let _ = flush_channel_clone.send((flush_task_name.clone(), line, false));

					line = String::new();
				}
				while let Ok(read) = child_stderr.read_line(&mut line) {
					if read == 0 {
						break;
					}

					let _ = flush_channel_clone.send((flush_task_name.clone(), line, true));

					line = String::new();
				}

				async_std::task::sleep(std::time::Duration::from_millis(10)).await;
			}
		});

		let mut rc = 0;
		// Loop until completion.
		loop {
			// Has the child exited?
			let child_opt_res = child.try_wait();
			if let Err(child_err) = child_opt_res {
				error!("Failed to read child status: [{:?}]", child_err);
				rc = 10;
				let _ = child.kill();
				break;
			}
			let child_opt = child_opt_res.unwrap();
			if child_opt.is_some() {
				rc = child_opt.unwrap().code().unwrap_or(10);
				break;
			}

			// Have we been requested to stop?
			if should_stop.load(Ordering::SeqCst) {
				error!("HostExecutor was told to stop! killing child...");
				rc = 10;
				let _ = child.kill();
				break;
			}

			async_std::task::sleep(std::time::Duration::from_millis(10)).await;
		}

		has_finished.store(true, Ordering::SeqCst);
		flush_task.await;

		// Return exit code.
		rc as isize
	}
}

#[cfg(test)]
mod unit_tests {
	use super::*;

	#[test]
	fn is_compatible() {
		let compat = HostExecutor::is_compatible();
		assert_eq!(compat, CompatibilityStatus::Compatible);
	}

	#[test]
	fn meets_requirements() {
		let pb = PathBuf::from("/tmp/non-existant");
		let he = HostExecutor::new(&pb).expect("Should always be able to construct HostExecutor");

		assert!(
			he.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"host".to_owned(),
				None
			)])
		);
		assert!(
			!he.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"blah".to_owned(),
				None
			)])
		);
		assert!(
			he.meets_requirements(&vec![crate::config::types::NeedsRequirement::new(
				"host".to_owned(),
				Some("*".to_owned())
			)])
		);
		assert!(!he.meets_requirements(&vec![
			crate::config::types::NeedsRequirement::new("host".to_owned(), None),
			crate::config::types::NeedsRequirement::new("another-service".to_owned(), None)
		]));
	}
}
