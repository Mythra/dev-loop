use crate::{
	dirs::{get_tmp_dir, mark_as_world_editable, mark_file_as_executable, rewrite_tmp_dir},
	executors::ExecutableTask,
};

use color_eyre::{eyre::WrapErr, Result, Section};
use std::{
	fs::{create_dir_all, write as write_file, File},
	path::PathBuf,
	time::{SystemTime, UNIX_EPOCH},
};
use tracing::warn;

/// Create the shared directory to execute in.
pub fn create_executor_shared_dir(pipeline_id: &str) -> Result<PathBuf> {
	let mut tmp_path = get_tmp_dir();
	tmp_path.push(format!("{}-dl-host", pipeline_id));
	create_dir_all(tmp_path.clone())?;
	Ok(tmp_path)
}

/// Create a series of files that can be used to capture logs for an entrypoint.
pub fn create_log_proxy_files(
	shared_dir: &PathBuf,
	task: &ExecutableTask,
) -> Result<(PathBuf, PathBuf)> {
	let epoch = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.wrap_err(
			"System Clock is before unix start time? Please make sure your clock is accurate.",
		)?
		.as_secs();

	let mut stdout_log_path = shared_dir.clone();
	stdout_log_path.push(format!("{}-{}-out.log", epoch, task.get_task_name()));
	let mut stderr_log_path = shared_dir.clone();
	stderr_log_path.push(format!("{}-{}-err.log", epoch, task.get_task_name()));

	File::create(&stdout_log_path)
		.wrap_err("Failed to create file for logs to stdout")
		.note("If the issue isn't immediately clear (e.g. disk full), please file an issue.")?;
	File::create(&stderr_log_path)
		.wrap_err("Failed to create file for logs to stderr")
		.note("If the issue isn't immediately clear (e.g. disk full), please file an issue.")?;

	if let Err(err) = mark_as_world_editable(&stdout_log_path) {
		warn!("NOTE Failed to mark stdout log file: [{:?}] as world writable, this may cause a lack of logs to be written.\n{:?}", stdout_log_path, err);
	}
	if let Err(err) = mark_as_world_editable(&stderr_log_path) {
		warn!("NOTE Failed to mark stderr log file: [{:?}] as world writable, this may cause a lack of logs to be written.\n{:?}", stderr_log_path, err);
	}

	Ok((stdout_log_path, stderr_log_path))
}

/// Create an entrypoint to run for tasks.
#[allow(clippy::too_many_arguments)]
pub fn create_entrypoint(
	project_root: &str,
	tmp_dir: &str,
	shared_dir: PathBuf,
	helper_src_line: &str,
	task: &ExecutableTask,
	rewrite_tmp: bool,
	stdout_log_path: Option<String>,
	stderr_log_path: Option<String>,
) -> Result<PathBuf> {
	let mut task_path = shared_dir.clone();
	task_path.push(format!("{}.sh", task.get_task_name()));

	let script_to_run = if rewrite_tmp {
		rewrite_tmp_dir(tmp_dir, &task_path)
	} else {
		task_path.to_string_lossy().to_string()
	};

	let mut entrypoint_path = shared_dir;
	entrypoint_path.push(format!("{}-entrypoint.sh", task.get_task_name()));

	write_file(&task_path, task.get_contents().get_contents())
		.wrap_err("Failed to copy your task script to temporary directory")?;

	let mut entrypoint_script = format!(
		"#!/usr/bin/env bash

{opening_bracket}

cd {project_root}

# Source Helpers
{helper}

eval \"$(declare -F | sed -e 's/-f /-fx /')\"

{script} {arg_str}

{closing_bracket}",
		opening_bracket = "{",
		project_root = project_root,
		helper = helper_src_line,
		script = script_to_run,
		arg_str = task.get_arg_string(),
		closing_bracket = "}",
	);
	match (stdout_log_path.is_some(), stderr_log_path.is_some()) {
		(true, true) => {
			entrypoint_script += &format!(
				" >{} 2>{}",
				stdout_log_path.unwrap(),
				stderr_log_path.unwrap()
			);
		}
		(true, false) => {
			entrypoint_script += &format!(" >{}", stdout_log_path.unwrap());
		}
		(false, true) => {
			entrypoint_script += &format!(" 2>{}", stderr_log_path.unwrap());
		}
		(false, false) => {}
	}

	write_file(&entrypoint_path, entrypoint_script).wrap_err("Failed to write entrypoint file")?;

	mark_file_as_executable(&task_path)?;
	mark_file_as_executable(&entrypoint_path)?;

	if rewrite_tmp {
		Ok(PathBuf::from(rewrite_tmp_dir(tmp_dir, &entrypoint_path)))
	} else {
		Ok(entrypoint_path)
	}
}
