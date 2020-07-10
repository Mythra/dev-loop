use crate::{log::HAS_OUTPUT_LOG_MSG, terminal::throttle::Throttle};

use colored::Colorize;
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::{HashMap, HashSet};
use term_size::dimensions as terminal_dimensions;

/// Represents a `TaskChange` or a task starting/stopping.
pub enum TaskChange {
	/// Indicates a task starting.
	StartedTask(String),
	/// Indicates a task finishing.
	FinishedTask(String),
}

/// The `TaskIndicator` is used to help output the current tasks that are
/// running. It is inspired by `bazel`'s output, however not quite exactly
/// the same since they do serve seperate purposes.
///
/// This does always print to STDERR, and will properly respect "colour"
/// settings for STDERR. If colour is disabled we will simply just output the
/// logs of the tasks coming in. If colour is enabled we will throttle, and
/// output the logs of the task coming in as well as a list of the tasks
/// currently running.
///
/// The `TaskIndicator` works off a series of channels to receive updates from
/// other threads. Both of these channels are multi-producer single consumer
/// channels. They each serve a distinct purpose:
///
///   1. `TaskQueue`: Used to update the indicator on which tasks are currently
///                   running. This way it's possible to know what to render to
///                   a user. This will only render in colour mode.
///   2. `OutputQueue`: This should be the actual output coming from each task.
///                     It takes in a `task_name`, and the output to show. When
///                     in colour mode this will "line buffer" so we can
///                     prepend the task name that is running. For situations
///                     like `run` where multiple tasks are executing at once.
pub struct TaskIndicator {
	/// The amount of lines we'll need to erase to render the indicator again.
	lines_previously_rendered: usize,
	// The receiver for lines from STDOUT, and the task it's outputting for.
	log_channel: Receiver<(String, String, bool)>,
	// The receiver for tasks starting/finishing/etc.
	task_changes: Receiver<TaskChange>,
	/// The total number of tasks that there will be to execute.
	task_count: usize,
	// A series of buffers for output on tasks.
	//
	// We want to buffer on newlines for task_line_buffers
	// so when doing colour'd output we can show:
	// `task_name | blah blah blah`, and not have it split
	// between lines.
	//
	// We will also flush lines if a task gets removed from the
	// `tasks_running` hashset.
	task_line_buffers: HashMap<String, String>,
	// Like `task_line_buffers` but for STDERR.
	task_line_buffers_err: HashMap<String, String>,
	/// The total number of tasks that have run.
	tasks_ran: usize,
	/// The list of tasks that are currently running.
	tasks_running: HashSet<String>,
	/// The width of the terminal.
	terminal_width: usize,
	/// The "throttle" instance that helps us not output
	/// to often to the terminal if rendering with colour.
	throttle: Throttle,
	/// Should we show colour for STDOUT?
	use_colour_out: bool,
	/// Should we show colour for STDERR?
	use_colour_err: bool,
}

impl TaskIndicator {
	// Print a log line of colour.
	fn print_new_log_line_colour(task_name: String, line: &str, is_stderr: bool) {
		if line.is_empty() {
			return;
		}

		let ftn = if task_name.len() > 10 {
			task_name.chars().take(7).collect::<String>() + "..."
		} else {
			task_name
		};

		let mut padding = String::new();
		for _ in ftn.len()..10 {
			padding += " ";
		}
		if is_stderr {
			eprintln!("{}{}| {}", ftn.cyan(), padding, line);
		} else {
			println!("{}{}| {}", ftn.cyan(), padding, line);
		}
	}

	// Print any new log lines that have come in.
	fn print_new_log_lines_maybe_colour(&self, new_lines: Vec<(String, String, bool)>) {
		for (task_name, line, is_err) in new_lines {
			if (is_err && self.use_colour_err) || (!is_err && self.use_colour_out) {
				Self::print_new_log_line_colour(task_name, &line, is_err);
			} else if is_err {
				eprintln!("{}", line);
			} else {
				println!("{}", line);
			}
		}
	}

	/// Create a new `TaskIndicator`.
	///
	/// `task_count`: The total number of tasks that we have to run.
	/// `use_colour_out`: If we should bother outputting colour for STDOUT.
	/// `use_colour_err`: If we should bother outputting colour for STDERR.
	///
	/// Returns a tuple of:
	///   1. The task indicator instance.
	///   2. A channel sender to send logs (and the task that created them).
	///   3. A channel sender to send task changes too.
	#[must_use]
	pub fn new(
		task_count: usize,
		use_colour_out: bool,
		use_colour_err: bool,
	) -> (Self, Sender<(String, String, bool)>, Sender<TaskChange>) {
		let (log_sender, log_receiver) = unbounded();
		let (tc_sender, tc_receiver) = unbounded();

		(
			Self {
				lines_previously_rendered: 0,
				log_channel: log_receiver,
				task_changes: tc_receiver,
				task_count,
				task_line_buffers: HashMap::new(),
				task_line_buffers_err: HashMap::new(),
				tasks_ran: 0,
				tasks_running: HashSet::new(),
				terminal_width: 80,
				throttle: Throttle::new(),
				use_colour_out,
				use_colour_err,
			},
			log_sender,
			tc_sender,
		)
	}

	/// "Tick", or update the task indicator.
	///
	/// This may be a no-op if the indicator decides so, and the indicator is
	/// resilient to no set tick time.
	pub fn tick(&mut self) {
		// First ensure the throttler is allowing us to render.
		if !self.throttle.allowed() {
			return;
		}

		if !self.use_colour_out && !self.use_colour_err {
			self.tick_no_colour();
			return;
		}

		// First process any changes to tasks that we have.
		let mut tasks_need_flushing = HashSet::<String>::new();
		let mut has_task_changes = false;
		while let Ok(change) = self.task_changes.try_recv() {
			has_task_changes = true;

			match change {
				TaskChange::StartedTask(task_name) => {
					self.tasks_running.insert(task_name);
				}
				TaskChange::FinishedTask(task_name) => {
					self.tasks_ran += 1;
					self.tasks_running.remove(&task_name);
					tasks_need_flushing.insert(task_name);
				}
			}
		}

		// Next determine if we have any log lines that need to change.
		let mut new_log_lines = Vec::<(String, String, bool)>::new();

		// First check for any new lines that came in, buffering on new lines.
		while let Ok((task_name, str_data, is_err)) = self.log_channel.try_recv() {
			let mut lines = if is_err {
				if self.task_line_buffers_err.contains_key(&task_name) {
					self.task_line_buffers_err.remove(&task_name).unwrap() + &str_data
				} else {
					str_data
				}
			} else if self.task_line_buffers.contains_key(&task_name) {
				self.task_line_buffers.remove(&task_name).unwrap() + &str_data
			} else {
				str_data
			}
			.split('\n')
			.map(String::from)
			.collect::<Vec<String>>();

			if lines.len() == 1 {
				if is_err {
					self.task_line_buffers_err
						.insert(task_name, lines.swap_remove(0));
				} else {
					self.task_line_buffers
						.insert(task_name, lines.swap_remove(0));
				}
			} else {
				for line in lines.iter().take(lines.len() - 1) {
					new_log_lines.push((task_name.clone(), line.to_owned(), is_err));
				}

				if !lines[lines.len() - 1].is_empty() {
					if is_err {
						self.task_line_buffers_err
							.insert(task_name, lines.swap_remove(lines.len() - 1));
					} else {
						self.task_line_buffers
							.insert(task_name, lines.swap_remove(lines.len() - 1));
					}
				}
			}
		}

		// Next if any tasks finished, forcefully flush.
		// We do this after processing new lines incase they come in after.
		// It is possible a line gets really really delayed, but we shrug at that
		// case. We'll print on the very end.
		//
		// The order of the one task will still be in order, there just may be things
		// inbetween.
		for flushable_task in tasks_need_flushing {
			if self.task_line_buffers.contains_key(&flushable_task) {
				let partial_line = self.task_line_buffers.remove(&flushable_task).unwrap();
				new_log_lines.push((flushable_task.clone(), partial_line, false));
			}
			if self.task_line_buffers_err.contains_key(&flushable_task) {
				let partial_line = self.task_line_buffers_err.remove(&flushable_task).unwrap();
				new_log_lines.push((flushable_task, partial_line, true));
			}
		}

		// Update the terminal width incase someone changed their terminal.
		let updated_width = self.update_term_width();

		// If we have changes, it's time to re-render...
		if has_task_changes || !new_log_lines.is_empty() || updated_width {
			// Erase the previous task lines...
			self.erase_task_lines();
			// Print any new log lines that have come in...
			self.print_new_log_lines_maybe_colour(new_log_lines);
			// Print the new tasks string.
			self.print_tasks_colour();
		}
	}

	/// Stop this task indicator, and flush all remaining logs.
	pub fn stop_and_flush(mut self) {
		// Ignore task status updates.
		while let Ok(_) = self.task_changes.try_recv() {}

		if !self.use_colour_out && !self.use_colour_err {
			self.tick_no_colour();
			return;
		}

		self.erase_task_lines();
		for (key, value) in self.task_line_buffers {
			if self.use_colour_out {
				Self::print_new_log_line_colour(key.clone(), &value, false);
			} else {
				println!("{}", value);
			}
		}
		for (key, value) in self.task_line_buffers_err {
			if self.use_colour_err {
				Self::print_new_log_line_colour(key.clone(), &value, true);
			} else {
				eprintln!("{}", value);
			}
		}
		while let Ok((task_name, str_data, is_stderr)) = self.log_channel.try_recv() {
			let lines = str_data
				.split('\n')
				.map(String::from)
				.collect::<Vec<String>>();
			for line in lines {
				if (is_stderr && self.use_colour_err) || (!is_stderr && self.use_colour_out) {
					Self::print_new_log_line_colour(task_name.clone(), &line, is_stderr);
				} else if is_stderr {
					eprintln!("{}", line);
				} else {
					println!("{}", line);
				}
			}
		}
	}

	fn tick_no_colour(&mut self) {
		// Make sure the buffer doesn't fill up, but we don't care about task/tasks.
		while let Ok(_) = self.task_changes.try_recv() {}

		// Print out any lines that have come in...
		while let Ok((_, str_data, is_err)) = self.log_channel.try_recv() {
			if is_err {
				eprint!("{}", str_data);
			} else {
				print!("{}", str_data);
			}
		}
	}

	// Query for an updated terminal width.
	fn update_term_width(&mut self) -> bool {
		let old_width = self.terminal_width;

		if let Some((term_width, _)) = terminal_dimensions() {
			self.terminal_width = term_width;
			if old_width != self.terminal_width {
				return true;
			}
		}

		false
	}

	// Erase the previously rendered task lines.
	fn erase_task_lines(&mut self) {
		// For each line we previously rendered.
		if self.lines_previously_rendered == 0 {
			HAS_OUTPUT_LOG_MSG.store(false, std::sync::atomic::Ordering::Release);
			return;
		}
		// Don't earse if a log line flew by.
		if HAS_OUTPUT_LOG_MSG.swap(false, std::sync::atomic::Ordering::AcqRel) {
			return;
		}

		// Erase the current line.
		let mut line = "\x1B[2K".to_owned();
		// For each extra line (more than one..) move the cursor up, and erase it.
		for _ in 0..(self.lines_previously_rendered - 1) {
			line += "\x1B[1A\x1B[2K";
		}
		// Move the cursor back over to the far left. 1000 for extra padding, and the current width.
		line += &format!("\x1B[{}D", 1000 + self.terminal_width);
		eprint!("{}", line);
	}

	// Print the "tasks" string with colour.
	fn print_tasks_colour(&mut self) {
		if self.tasks_running.is_empty() {
			eprint!(
				"[{}/{}] {} Tasks Running...\n",
				self.tasks_ran, self.task_count, 0
			);

			self.lines_previously_rendered = 2;
		} else {
			let mut task_output_str = String::new();
			for running_task in &self.tasks_running {
				if task_output_str.is_empty() {
					task_output_str += "  ";
				} else {
					task_output_str += "\n  ";
				}
				task_output_str += running_task.as_str();
			}

			eprint!(
				"{} {} Tasks Running...\n{}\n",
				&format!("[{}/{}]", self.tasks_ran, self.task_count).green(),
				self.tasks_running.len(),
				task_output_str,
			);

			self.lines_previously_rendered = self.tasks_running.len() + 2;
		}
	}
}
