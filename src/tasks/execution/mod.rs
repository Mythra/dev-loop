//! Contains a list of functions that help with the execution of tasks. These
//! are thins like building the full list of "Task's" to run into an ordered
//! vector.

pub mod preparation;

use crate::fetch::FetchedItem;
use crate::get_tmp_dir;
use crate::has_ctrlc_been_hit;
use crate::tasks::execution::preparation::ExecutableTask;
use crate::terminal::task_indicator::TaskChange;
use crate::terminal::Term;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use tracing::{error, info};

/// Execute a particular "line" of tasks.
#[allow(clippy::too_many_arguments)]
async fn execute_task_line(
	tlid: usize,
	helpers: Vec<FetchedItem>,
	task_line: Vec<ExecutableTask>,
	rc: Arc<AtomicIsize>,
	should_stop: Arc<AtomicBool>,
	log_channel: Sender<(String, String, bool)>,
	task_channel: Sender<TaskChange>,
) {
	// The order of executing a task line goes like this:
	//
	//  1. Write helpers to a temporary diretory, using the task line id as a unique factor.
	//     We do this, cause we've seen bugs caused by helpers overwriting themselves, and when
	//     only one "line" is screwed up it's easier to debug which "line" did it.
	//  2. For each task, send an update over the task channel that it's started.
	//  3. After each task finishes send an update on the task channel.
	//  4. Check the rc. If it's not 0, break.
	//  5. Check should_stop, if we should stop, break.
	//  6. Otherwise keep iterating through the line.
	//  7. At the end of the line return the rc.

	let mut helper_dir = get_tmp_dir().await;
	helper_dir.push(format!("tlid-{}-helpers-dl-host/", tlid));
	let helpers_res = async_std::fs::create_dir_all(helper_dir.clone()).await;
	if let Err(helper_dir_err) = helpers_res {
		error!(
			"Failed to create helper directory due to: [{:?}]",
			helper_dir_err
		);
		rc.store(10, Ordering::SeqCst);
		return;
	}

	// We build the string to source all the helper files and copy that around since it's cheaper.
	//
	// We need to write to `/tmp` for a couple reasons:
	//   1. `/tmp` must always be mounted by the executors, and will always be present.
	//   2. We need a local way to source files that were fetched remotely.
	let mut src_string = String::new();
	for (idx, fetched_helper) in helpers.into_iter().enumerate() {
		let mut helper_path = helper_dir.clone();
		helper_path.push(format!("helper-{}.sh", idx));
		let helper_write_res =
			async_std::fs::write(helper_path, fetched_helper.get_contents()).await;
		if let Err(write_err) = helper_write_res {
			error!(
				"Failed to write helper script to temporary directory due to: [{:?}]",
				write_err
			);
			rc.store(10, Ordering::SeqCst);
			return;
		}

		if src_string.is_empty() {
			src_string = format!(
				"source /tmp/tlid-{}-helpers-dl-host/helper-{}.sh",
				tlid, idx
			);
		} else {
			src_string += &format!(
				" && source /tmp/tlid-{}-helpers-dl-host/helper-{}.sh",
				tlid, idx
			);
		}
	}

	// Incase we hit a stop before we actually started executing.
	if should_stop.load(Ordering::SeqCst) {
		rc.store(10, Ordering::SeqCst);
		return;
	}

	let mut new_rc = 0;
	for task in task_line {
		let _ = task_channel.send(TaskChange::StartedTask(task.get_task_name().to_owned()));
		let task_rc = task
			.get_executor()
			.execute(log_channel.clone(), should_stop.clone(), &src_string, &task)
			.await;
		new_rc = task_rc;
		let _ = task_channel.send(TaskChange::FinishedTask(task.get_task_name().to_owned()));
		if new_rc != 0 {
			break;
		}
	}

	rc.store(new_rc, Ordering::SeqCst);
}

/// Execute a series of tasks in parallel.
///
/// `helpers`: The list of helpers to render for each task.
/// `tasks`: the list of list of tasks. the top level list indicates a unit
///          of parralelization. the second list executes within order.
/// `task_count`: the total count of tasks. yes we can derive this, but it's easier
///               for it to be derived as the list of lists is being created, and passed in.
/// `terminal`: the terminal to render status progress too.
#[allow(clippy::cast_possible_truncation)]
pub async fn execute_tasks_in_parallel(
	helpers: Vec<FetchedItem>,
	tasks: Vec<Vec<ExecutableTask>>,
	task_count: usize,
	terminal: &Term,
) -> i32 {
	let mut rc_indicators = Vec::new();
	let should_stop = Arc::new(AtomicBool::new(false));

	let (mut task_indicator, log_sender, task_sender) = terminal.create_task_indicator(task_count);

	for (idx, task_set) in tasks.into_iter().enumerate() {
		let helper_clone = helpers.clone();
		let cloned_should_stop = should_stop.clone();
		let cloned_log_sender = log_sender.clone();
		let cloned_task_sender = task_sender.clone();

		let finished_line = Arc::new(AtomicIsize::new(-1));
		let finished_clone = finished_line.clone();

		info!("Found task line! Spawning Thread.");
		async_std::task::spawn(async move {
			execute_task_line(
				idx,
				helper_clone,
				task_set,
				finished_clone,
				cloned_should_stop,
				cloned_log_sender,
				cloned_task_sender,
			)
			.await;
		});
		rc_indicators.push(finished_line);
	}

	let mut rc: i32 = 0;

	loop {
		task_indicator.tick();

		if has_ctrlc_been_hit() {
			info!("Detected Ctrl-C being hit! Setting RC to 10, and shutting down.");
			rc = 10;
		}

		if rc != 0 {
			should_stop.store(true, Ordering::SeqCst);
		}

		let mut any_more = false;
		for potential_rc in &rc_indicators {
			let new_rc = potential_rc.load(Ordering::SeqCst);
			if new_rc == -1 {
				any_more = true;
				break;
			} else {
				info!("Found finished task rc: [{}]", new_rc);
				// If it's already not equal to 0 preserve the original exit code.
				if rc == 0 {
					let mut new_rc_as_i32 = new_rc as i32;
					if new_rc_as_i32 > 255 {
						new_rc_as_i32 = 255;
					}
					if new_rc_as_i32 < 0 {
						new_rc_as_i32 = 255;
					}

					rc += new_rc_as_i32;
				}
			}
		}

		if !any_more {
			break;
		}

		async_std::task::sleep(std::time::Duration::from_millis(50)).await;
	}

	task_indicator.stop_and_flush();

	rc
}
