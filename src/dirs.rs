use color_eyre::{eyre::WrapErr, Result, Section};
use std::{
	convert::TryFrom,
	env,
	ffi::{CStr, OsString},
	fs::set_permissions,
	mem,
	path::PathBuf,
	ptr,
};

cfg_if::cfg_if! {
  if #[cfg(unix)] {
		use std::os::unix::ffi::OsStringExt;
  } else if #[cfg(win)] {
		use std::os::windows::ffi::OsStringExt;
  }
}

/// Rewrite the temporary directory.
pub fn rewrite_tmp_dir(host_tmp_dir: &str, path: &PathBuf) -> String {
	let replacement_str = if host_tmp_dir.ends_with('/') {
		"/tmp/"
	} else {
		"/tmp"
	};

	path.to_string_lossy()
		.replace(&host_tmp_dir, replacement_str)
}

#[cfg(target_family = "unix")]
pub fn mark_as_world_editable(path: &PathBuf) -> Result<()> {
	use std::fs::Permissions;
	use std::os::unix::fs::PermissionsExt;

	let executable_permissions = Permissions::from_mode(0o666);
	set_permissions(path, executable_permissions)
		.map(|_| ())
		.wrap_err(format!("Failed to mark file as editable which is needed: [{:?}]", path))
		.suggestion("If the error isn't immediately clear, please file an issue as it's probably a bug in dev-loop with your system.")
}

#[cfg(not(target_family = "unix"))]
pub fn mark_as_world_editable(path: &PathBuf) -> Result<()> {
	Ok(())
}

#[cfg(target_family = "unix")]
pub fn mark_file_as_executable(path: &PathBuf) -> Result<()> {
	use std::fs::Permissions;
	use std::os::unix::fs::PermissionsExt;

	let executable_permissions = Permissions::from_mode(0o777);

	set_permissions(path, executable_permissions)
		.map(|_| ())
		.wrap_err(format!("Failed to mark file as executable which is needed: [{:?}]", path))
		.suggestion("If the error isn't immediately clear, please file an issue as it's probably a bug in dev-loop with your system.")
}

#[cfg(not(target_family = "unix"))]
pub fn mark_file_as_executable(path: &PathBuf) -> Result<()> {
	Ok(())
}

/// Get the temporary directory for this host.
#[must_use]
pub fn get_tmp_dir() -> PathBuf {
	// Mac OS X actually uses "TMPDIR" for a user specified temporary directory
	// as opposed to `/tmp`. There are subtle differences between the two, and
	// without getting into details the key thing is we should use it if it
	// is set.
	//
	// We've seen numerous problems trying to use `/tmp` on OSX.
	if let Ok(tmpdir_env) = std::env::var("TMPDIR") {
		let pbte = PathBuf::from(tmpdir_env);
		if pbte.is_dir() {
			pbte
		} else {
			PathBuf::from("/tmp")
		}
	} else {
		PathBuf::from("/tmp")
	}
}

#[cfg(any(target_os = "android", target_os = "ios", target_os = "emscripten"))]
unsafe fn home_dir_fallback() -> Option<OsString> {
	None
}

#[cfg(not(any(target_os = "android", target_os = "ios", target_os = "emscripten")))]
unsafe fn home_dir_fallback() -> Option<OsString> {
	let amt = match libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) {
		n if n < 0 => 512 as usize,
		n => usize::try_from(n).unwrap_or(512),
	};
	let mut buf = Vec::with_capacity(amt);
	let mut passwd: libc::passwd = mem::zeroed();
	let mut result = ptr::null_mut();
	match libc::getpwuid_r(
		libc::getuid(),
		&mut passwd,
		buf.as_mut_ptr(),
		buf.capacity(),
		&mut result,
	) {
		0 if !result.is_null() => {
			let ptr = passwd.pw_dir as *const _;
			let bytes = CStr::from_ptr(ptr).to_bytes();
			if bytes.is_empty() {
				None
			} else {
				Some(OsStringExt::from_vec(bytes.to_vec()))
			}
		}
		_ => None,
	}
}

/// Calculate the home directory of a user.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
	env::var_os("HOME")
		.and_then(|h| if h.is_empty() { None } else { Some(h) })
		.or_else(|| unsafe { home_dir_fallback() })
		.map(PathBuf::from)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Tests are meant to prove that dev-loop works on a platform.
	///
	/// `home_dir()` should always pass on a supported platform.
	#[test]
	fn can_get_home_directory() {
		let home_dir = home_dir();
		assert!(home_dir.is_some());
		let home_dir = home_dir.unwrap();
		assert!(home_dir.is_dir());
	}
}
