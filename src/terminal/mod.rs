//! Handles outputing fancy things to a TTY.
//!
//! This is really only ever used by the LIST Command, but it's setup
//! as a module for now because it makes sense to be incase more than the list
//! command ever needs to do something fancy.

use atty::Stream;
use colored::Colorize;
use crossbeam_channel::Sender;
use lazy_static::lazy_static;
use std::sync::Arc;
use term_size::dimensions as terminal_dimensions;

pub(crate) mod task_indicator;
pub(crate) mod throttle;

lazy_static! {
	pub static ref TERM: Arc<Term> = Arc::new(Term::new());
}

/// Represents a `Term`, or terminal. the output needed in order to properly render
/// colours/progress bars/etc.
pub struct Term {
	/// Should we actually allow colour to STDOUT?
	is_colour: bool,
	/// Should we allow colour to STDERR?
	is_colour_err: bool,
	/// The width of the terminal currently.
	term_width: usize,
}

/// The "default" terminal.
impl Default for Term {
	#[must_use]
	fn default() -> Self {
		Self::new()
	}
}

impl Term {
	/// Create a new "Terminal" instance. Will determine if colour is allowed by checking for:
	///
	/// 1. If STDOUT & STDERR are a tty.
	/// 2. There is no existance of a `NO_COLOR`
	/// 3. There is no existance of a `CI` variable.
	#[must_use]
	pub fn new() -> Self {
		let tty_out = atty::is(Stream::Stdout);
		let tty_err = atty::is(Stream::Stderr);

		let mut term_width: usize = 80;
		if let Some((new_term_width, _)) = terminal_dimensions() {
			term_width = new_term_width;
		}

		let mut use_colour = true;
		if use_colour {
			if let Ok(value) = std::env::var("NO_COLOR") {
				if !value.is_empty() {
					use_colour = false;
				}
			}
		}

		let mut force_stdout_colour = false;
		let mut force_stderr_colour = false;

		if let Ok(value) = std::env::var("DL_FORCE_COLOR") {
			if value == "true" {
				force_stdout_colour = true;
				force_stderr_colour = true;
			}
		}
		if let Ok(value) = std::env::var("DL_FORCE_STDOUT_COLOR") {
			if value == "true" {
				force_stdout_colour = true;
			}
		}
		if let Ok(value) = std::env::var("DL_FORCE_STDERR_COLOR") {
			if value == "true" {
				force_stderr_colour = true;
			}
		}

		Self {
			is_colour: force_stdout_colour || (use_colour && tty_out),
			is_colour_err: force_stderr_colour || (use_colour && tty_err),
			term_width,
		}
	}

	/// Render the text needed for a title bar.
	///
	/// `left` - the text to appear on the left side of the title bar.
	/// `right` - the text to appear on the right side of the title bar.
	/// `term_width` - the width of the terminal.
	#[must_use]
	pub fn render_title_bar(&self, left: &str, right: &str) -> String {
		let mut usable_width = self.term_width;

		let mut finalized_right = right.to_owned();
		let mut finalized_left = left.to_owned();

		if usable_width <= finalized_left.len() {
			finalized_left = finalized_left.chars().take(usable_width).collect();
		}
		usable_width -= finalized_left.len();
		if usable_width <= finalized_right.len() {
			finalized_right = finalized_right.chars().take(usable_width).collect();
		}
		usable_width -= finalized_right.len();

		let mut padded = String::new();
		for _ in 0..usable_width {
			padded += " ";
		}

		if self.is_colour {
			format!(
				"{}{}{}",
				finalized_left.bold(),
				padded,
				finalized_right.bold()
			)
		} else {
			format!("{}{}{}", finalized_left, padded, finalized_right)
		}
	}

	/// Determine if this terminal should color STDOUT.
	#[must_use]
	pub fn should_color_stdout(&self) -> bool {
		self.is_colour
	}

	/// Determine if this terminal should color STDERR.
	#[must_use]
	pub fn should_color_stderr(&self) -> bool {
		self.is_colour_err
	}

	/// Render a list of items with a particular description.
	///
	/// `list_with_descriptions`: A pair of <item, description>.
	#[must_use]
	pub fn render_list_with_description(
		&self,
		list_with_descriptions: &[(String, String)],
	) -> String {
		if list_with_descriptions.is_empty() {
			return String::new();
		}

		let mut longest_key: usize = 0;
		for (key, _) in list_with_descriptions {
			let mut len = key.len();
			if len > 15 {
				len = 15;
			}
			if len > longest_key {
				longest_key = len;
			}
		}

		let mut result = String::new();

		for (key, description) in list_with_descriptions {
			let mut actual_key;
			if key.len() > 15 {
				actual_key = key.chars().take(12).collect();
				actual_key += "...";
			} else {
				actual_key = key.to_owned();
			}

			let padding_needed = longest_key - actual_key.len();
			let mut padded = String::new();
			for _ in 0..padding_needed {
				padded += " ";
			}

			if self.is_colour {
				result += &format!("  {}  {}{}\n", actual_key.cyan(), padded, description);
			} else {
				result += &format!("  {}  {}{}\n", actual_key, padded, description);
			}
		}

		result
	}

	/// Render a "list section", or a list of items with a title card.
	///
	/// `title`: The title of this list.
	/// `list_with_descriptions`: The list of items with their descriptions.
	#[must_use]
	pub fn render_list_section(
		&self,
		title: &str,
		list_with_descriptions: &[(String, String)],
	) -> String {
		format!(
			"{}\n\n{}",
			self.render_title_bar(title, &format!("[{}]", list_with_descriptions.len())),
			self.render_list_with_description(list_with_descriptions)
		)
	}

	/// Create an indicator for outputting tasks to a tty.
	///
	/// Returns a tuple of:
	///   1. The task indicator instance.
	///   2. A channel sender to send logs (and the task that created them).
	///   3. A channel sender to send task changes too.
	#[must_use]
	pub fn create_task_indicator(
		&self,
		task_count: usize,
	) -> (
		task_indicator::TaskIndicator,
		Sender<(String, String, bool)>,
		Sender<task_indicator::TaskChange>,
	) {
		task_indicator::TaskIndicator::new(
			task_count,
			self.should_color_stdout(),
			self.should_color_stderr(),
		)
	}
}
