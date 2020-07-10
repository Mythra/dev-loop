//! Contains the code for the "Host" executor, or the executor
//! that just uses the Host System.

use crate::{
	config::types::NeedsRequirement,
	dirs::get_tmp_dir,
	executors::{
		shared::{create_entrypoint, create_executor_shared_dir},
		CompatibilityStatus, Executor as ExecutorTrait,
	},
	tasks::execution::preparation::ExecutableTask,
};

use color_eyre::{
	eyre::{eyre, WrapErr},
	Result, Section,
};
use crossbeam_channel::Sender;
use std::{
	fs::{read_dir, remove_dir_all},
	io::{BufRead, BufReader, Error as IoError},
	path::PathBuf,
	process::{Command, Stdio},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};
use tracing::{debug, error, warn};

/// Determine if an error is an "ETXTFILEBUSY" error, e.g. someone
/// else is actively executing bash.
#[cfg(any(
	target_os = "macos",
	target_os = "ios",
	target_os = "linux",
	target_os = "android",
	target_os = "freebsd",
	target_os = "dragonfly",
	target_os = "openbsd",
	target_os = "netbsd"
))]
fn is_etxtfilebusy(os_err: &IoError) -> bool {
	if let Some(os_err_code) = os_err.raw_os_error() {
		// This stands for ETXTBUSY, since it's pretty weird to match on
		// message of rust.
		//
		// This seems to be correct for all OSs, listed above.
		//  - Linux: https://mariadb.com/kb/en/operating-system-error-codes/
		//  - OSX/iOS: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/intro.2.html
		//  - FreeBSD: https://www.freebsd.org/cgi/man.cgi?query=errno&sektion=2&manpath=freebsd-release-ports
		//  - Android: https://android.googlesource.com/kernel/lk/+/dima/for-travis/include/errno.h
		//  - DragonflyBSD: https://man.dragonflybsd.org/?command=errno&section=2
		//  - OpenBSD: https://man.openbsd.org/errno.2
		//  - NetBSD: https://netbsd.gw.com/cgi-bin/man-cgi?errno+2+NetBSD-6.0
		if os_err_code == 26 {
			return true;
		}
	}

	false
}

/// Determine if an error is an "ETXTFILEBUSY" error, e.g. someone
/// else is actively executing bash.
#[cfg(not(any(
	target_os = "macos",
	target_os = "ios",
	target_os = "linux",
	target_os = "android",
	target_os = "freebsd",
	target_os = "dragonfly",
	target_os = "openbsd",
	target_os = "netbsd"
)))]
fn is_etxtfilebusy(os_err: &IoError) -> bool {
	false
}

/// Represents the actual `Executor` for the host system.
#[derive(Debug)]
pub struct Executor {
	/// The root of the project, so we know where to "cd" into.
	project_root: String,
}

impl Executor {
	/// Create a new host executor, with nothing but the project root.
	///
	/// # Errors
	///
	/// If the project root is not on a valid utf-8 string path.
	pub fn new(project_root: &PathBuf) -> Result<Self> {
		let pr_as_string = project_root.to_str();
		if pr_as_string.is_none() {
			return Err(eyre!(
				"Failed to turn the project directory: [{:?}] into a utf8-string.",
				project_root,
			))
			.suggestion(
				"Please move the project directory to somewhere that is a UTF-8 only file path.",
			);
		}

		Ok(Self {
			project_root: pr_as_string.unwrap().to_owned(),
		})
	}

	/// Performs a clean up of all host resources.
	pub async fn clean() {
		// To clean all we would possibly have leftover is files in $TMPDIR.
		// So we iterate through everything in the temporary directory...
		if let Ok(entries) = read_dir(get_tmp_dir()) {
			for resulting_entry in entries {
				// Did we get something?
				if let Ok(entry_de) = resulting_entry {
					let entry = entry_de.path();
					// If it's not a directory ignore it.
					if !entry.is_dir() {
						debug!(
							"Found a non-directory in your temporary directory, skipping: [{:?}]",
							entry
						);
						continue;
					}
					// If it's not UTF-8 ignore it. We can't do a string comparison, and
					// we'd never write a non-utf-8 path anyway.
					let potential_str = entry.to_str();
					if potential_str.is_none() {
						debug!(
							"Found a non utf8 path in your temporary directory: [{:?}], dev-loop is guaranteed to place utf8 directories, so skipping.",
							entry,
						);
						continue;
					}
					let entry_str = potential_str.unwrap();

					// Is it a directory that ends with `-dl-host` the
					// identifier of dev-loop host executor?
					if !entry_str.ends_with("-dl-host") {
						debug!(
							"Skipping entry: [{:?}] does not appear to be a dev-loop temporary directory (dev-loop dirs end with -dl-host)",
							entry,
						);
						continue;
					}

					// If it is... remove the directory and everything underneath it.
					if let Err(remove_err) = remove_dir_all(&entry) {
						warn!(
							"{:?}",
							Err::<(), IoError>(remove_err)
								.wrap_err("Failed to clean temporary directory, trying to continue")
								.suggestion(
									format!("Try removing the directory manually with the command: `sudo rm -rf {}`", entry.to_string_lossy())
								).unwrap_err()
						);
					}
				}
			}
		}
	}

	/// Determines if this `Executor` is compatible with the system.
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

		match Command::new("bash").args(&["-c", "\"hash bash\""]).output() {
			Ok(_) => CompatibilityStatus::Compatible,
			Err(os_err) => {
				if is_etxtfilebusy(&os_err) {
					// Tail recurse.
					return Self::is_compatible();
				}

				CompatibilityStatus::CouldBeCompatible("install bash".to_owned())
			}
		}
	}
}

#[async_trait::async_trait]
impl ExecutorTrait for Executor {
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

	#[must_use]
	async fn execute(
		&self,
		log_channel: Sender<(String, String, bool)>,
		should_stop: Arc<AtomicBool>,
		helper_src_line: &str,
		task: &ExecutableTask,
		worker_count: usize,
	) -> Result<i32> {
		debug!("Host Executor executing task: [{}]", task.get_task_name());

		// Write out the small wrapper script that sources in the helpers, and runs the task.
		let shared_dir = create_executor_shared_dir(task.get_pipeline_id())
			.wrap_err("Failed to create pipeline directory")?;
		debug!(
			"Host Executor will be using temporary directory: [{:?}]",
			shared_dir
		);
		let entrypoint_path = create_entrypoint(
			&self.project_root,
			&get_tmp_dir().to_string_lossy().to_string(),
			shared_dir,
			helper_src_line,
			task,
			false,
			None,
			None,
		)?;
		let entrypoint_as_str = entrypoint_path.to_str().unwrap();

		// Spawn the command itself, retry if we get an ETXTFILEBUSY error incase we try to start two
		// bash processes at the same time.
		let mut command_res = Command::new(entrypoint_as_str)
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn();
		while let Err(command_err) = command_res {
			if is_etxtfilebusy(&command_err) {
				// Respawn the command again!
				command_res = Command::new(entrypoint_as_str)
					.stdin(Stdio::null())
					.stdout(Stdio::piped())
					.stderr(Stdio::piped())
					.spawn();
			} else {
				return Err(command_err)
					.wrap_err("Failed to run bash script on the host system")
					.note(format!("The script is located at: [{}]", entrypoint_as_str,));
			}
		}
		let mut command_pid = command_res.unwrap();

		let has_finished = Arc::new(AtomicBool::new(false));
		let mut child_stdout = BufReader::new(command_pid.stdout.take().unwrap());
		let mut child_stderr = BufReader::new(command_pid.stderr.take().unwrap());

		let flush_channel_clone = log_channel.clone();
		let flush_task_name = task.get_task_name().to_owned();
		let flush_has_finished_clone = has_finished.clone();

		let flush_task = async_std::task::spawn(async move {
			let mut line = String::new();
			let channel_name = format!("{}-{}", worker_count, flush_task_name);

			while !flush_has_finished_clone.load(Ordering::Relaxed) {
				while let Ok(read) = child_stdout.read_line(&mut line) {
					if read == 0 {
						break;
					}

					let _ = flush_channel_clone.send((channel_name.clone(), line, false));

					line = String::new();
				}
				while let Ok(read) = child_stderr.read_line(&mut line) {
					if read == 0 {
						break;
					}

					let _ = flush_channel_clone.send((channel_name.clone(), line, true));

					line = String::new();
				}

				async_std::task::sleep(std::time::Duration::from_millis(10)).await;
			}
		});

		let rc;
		// Loop until completion.
		loop {
			// Has the child exited?
			let child_opt_res = command_pid.try_wait();
			if let Err(child_err) = child_opt_res {
				error!("Failed to read child status: [{:?}]", child_err);
				rc = 10;
				let _ = command_pid.kill();
				break;
			}
			let child_opt = child_opt_res.unwrap();
			if child_opt.is_some() {
				rc = child_opt.unwrap().code().unwrap_or(10);
				break;
			}

			// Have we been requested to stop?
			if should_stop.load(Ordering::Acquire) {
				error!("Executor was told to stop! killing child...");
				rc = 10;
				let _ = command_pid.kill();
				break;
			}

			async_std::task::sleep(std::time::Duration::from_millis(10)).await;
		}

		has_finished.store(true, Ordering::Release);
		flush_task.await;

		Ok(rc)
	}
}

#[cfg(test)]
mod unit_tests {
	use super::*;

	#[test]
	fn is_compatible() {
		let compat = Executor::is_compatible();
		assert_eq!(compat, CompatibilityStatus::Compatible);
	}

	#[test]
	fn meets_requirements() {
		let pb = PathBuf::from("/tmp/non-existant");
		let he = Executor::new(&pb).expect("Should always be able to construct Executor for host.");

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
