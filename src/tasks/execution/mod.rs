//! Contains a list of functions that help with the execution of tasks. These
//! are thins like building the full list of "Task's" to run into an ordered
//! vector.

pub(crate) mod preparation;

use crate::{
	dirs::get_tmp_dir,
	fetch::FetchedItem,
	sigint::has_ctrlc_been_hit,
	tasks::execution::preparation::WorkUnit,
	terminal::{task_indicator::TaskChange, TERM},
};
use color_eyre::Result;
use crossbeam_channel::Sender;
use crossbeam_deque::{Stealer, Worker};
use std::{
	fs::create_dir_all,
	sync::{
		atomic::{AtomicBool, AtomicI32, Ordering},
		Arc,
	},
	time::{SystemTime, UNIX_EPOCH},
};
use tracing::{debug, error};

/// Execute a particular "line" of tasks.
async fn execute_task_line(
	src_string: Arc<String>,
	stealer: Stealer<WorkUnit>,
	rc: Arc<AtomicI32>,
	should_stop: Arc<AtomicBool>,
	log_channel: Sender<(String, String, bool)>,
	task_channel: Sender<TaskChange>,
	worker_count: usize,
) {
	// The order of executing a task line goes like this:
	//
	//  1. For each task, send an update over the task channel that it's started.
	//  2. After each task finishes send an update on the task channel.
	//  3. Check the rc. If it's not 0, break.
	//  4. Check should_stop, if we should stop, break.
	//  5. Otherwise keep iterating through the line.
	//  6. At the end of the line return the rc.

	// Incase we hit a stop before we actually started executing.
	if should_stop.load(Ordering::Acquire) {
		rc.store(10, Ordering::Release);
		return;
	}

	let mut new_rc = 0;
	loop {
		let stolen = stealer.steal();
		if stolen.is_empty() {
			break;
		}
		if stolen.is_retry() {
			continue;
		}

		let work_unit = stolen.success().unwrap();
		match work_unit {
			WorkUnit::SingleTask(task) => {
				let _ = task_channel.send(TaskChange::StartedTask(format!(
					"{}-{}",
					worker_count,
					task.get_task_name()
				)));
				let task_rc_res = task
					.get_executor()
					.execute(
						log_channel.clone(),
						should_stop.clone(),
						&(*src_string),
						&task,
						worker_count,
					)
					.await;
				match task_rc_res {
					Ok(rc) => {
						new_rc = rc;
					}
					Err(error) => {
						error!("{:?}", error);
						new_rc = 10;
					}
				}
				let _ = task_channel.send(TaskChange::FinishedTask(format!(
					"{}-{}",
					worker_count,
					task.get_task_name()
				)));
			}
			WorkUnit::Pipeline(tasks) => {
				for task in tasks {
					let _ = task_channel.send(TaskChange::StartedTask(format!(
						"{}-{}",
						worker_count,
						task.get_task_name()
					)));
					let task_rc_res = task
						.get_executor()
						.execute(
							log_channel.clone(),
							should_stop.clone(),
							&(*src_string),
							&task,
							worker_count,
						)
						.await;
					match task_rc_res {
						Ok(rc) => {
							new_rc = rc;
						}
						Err(error) => {
							error!("{:?}", error);
							new_rc = 10;
						}
					}
					let _ = task_channel.send(TaskChange::FinishedTask(format!(
						"{}-{}",
						worker_count,
						task.get_task_name()
					)));

					if new_rc != 0 {
						break;
					}
				}
			}
		}

		if new_rc != 0 {
			break;
		}
	}

	rc.store(new_rc, Ordering::Release);
}

/// Get the current epoch second count.
#[must_use]
fn get_epoch_seconds() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("Dev-Loop does not support running on a system where time is before unix epoch!")
		.as_secs()
}

/// Build the "Source string", or the string to source all the helpers.
fn build_helpers_source_string(helpers: Vec<FetchedItem>) -> Result<String> {
	let epoch = get_epoch_seconds();
	let mut helper_dir = get_tmp_dir();
	helper_dir.push(format!("{}-helpers-dl-host/", epoch));
	create_dir_all(helper_dir.clone())?;

	// We build the string to source all the helper files and copy that around since it's cheaper.
	//
	// We need to write to `/tmp` for a couple reasons:
	//   1. `/tmp` must always be mounted by the executors, and will always be present.
	//   2. We need a local way to source files that were fetched remotely.
	let mut src_string = String::new();
	for (idx, fetched_helper) in helpers.into_iter().enumerate() {
		let mut helper_path = helper_dir.clone();
		helper_path.push(format!("helper-{}.sh", idx));
		std::fs::write(helper_path.clone(), fetched_helper.get_contents())?;

		let tmp_path = format!("/tmp/{}-helpers-dl-host/helper-{}.sh", epoch, idx);
		if src_string.is_empty() {
			src_string = format!(
				"[[ -f \"{}\" ]] && source \"{}\" || source {:?}",
				tmp_path, tmp_path, helper_path
			);
		} else {
			src_string += &format!(
				" && [[ -f \"{}\" ]] && source \"{}\" || source {:?}",
				tmp_path, tmp_path, helper_path
			);
		}
	}

	Ok(src_string)
}

/// Execute a series of tasks in parallel.
///
/// `helpers`: The list of helpers to render for each task.
/// `tasks`: the list of list of tasks. the top level list indicates a unit
///          of parralelization. the second list executes within order.
/// `task_count`: the total count of tasks. yes we can derive this, but it's easier
///               for it to be derived as the list of lists is being created, and passed in.
///
/// # Errors
///
/// If we could not execute the tasks in parallel.
pub async fn execute_tasks_in_parallel(
	helpers: Vec<FetchedItem>,
	tasks: Worker<WorkUnit>,
	task_count: usize,
	worker_size: usize,
) -> Result<i32> {
	let mut rc_indicators = Vec::new();
	let should_stop = Arc::new(AtomicBool::new(false));

	let (mut task_indicator, log_sender, task_sender) = TERM.create_task_indicator(task_count);
	let src_string = build_helpers_source_string(helpers)?;
	let src_string_ref = Arc::new(src_string);

	for wc in 0..worker_size {
		let cloned_src_string_ref = src_string_ref.clone();
		let cloned_should_stop = should_stop.clone();
		let cloned_log_sender = log_sender.clone();
		let cloned_task_sender = task_sender.clone();
		let stealer = tasks.stealer();

		let finished_line = Arc::new(AtomicI32::new(-1));
		let finished_clone = finished_line.clone();

		async_std::task::spawn(async move {
			execute_task_line(
				cloned_src_string_ref,
				stealer,
				finished_clone,
				cloned_should_stop,
				cloned_log_sender,
				cloned_task_sender,
				wc,
			)
			.await;
		});
		rc_indicators.push(finished_line);
	}

	let mut rc: i32 = 0;

	loop {
		task_indicator.tick();

		if has_ctrlc_been_hit() {
			debug!("Detected Ctrl-C being hit! Shutting down.");
			should_stop.store(true, Ordering::Release);
		}

		if rc != 0 {
			should_stop.store(true, Ordering::Release);
		}

		let mut any_more = false;
		for potential_rc in &rc_indicators {
			let mut new_rc = potential_rc.load(Ordering::Acquire);
			if new_rc == -1 {
				any_more = true;
				break;
			} else {
				debug!("Found finished task rc: [{}]", new_rc);
				// If it's already not equal to 0 preserve the original exit code.
				if rc == 0 {
					if new_rc > 255 {
						new_rc = 255;
					}
					if new_rc < 0 {
						new_rc = 255;
					}

					rc += new_rc;
				}
			}
		}

		if !any_more {
			break;
		}

		async_std::task::sleep(std::time::Duration::from_millis(50)).await;
	}

	task_indicator.stop_and_flush();

	Ok(rc)
}
