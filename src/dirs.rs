use std::ffi::{CStr, OsString};
use std::path::PathBuf;
use std::{env, mem, ptr};

cfg_if::cfg_if! {
  if #[cfg(unix)] {
		use std::os::unix::ffi::OsStringExt;
  } else if #[cfg(win)] {
		use std::os::windows::ffi::OsStringExt;
  }
}

/// Calculate the home directory of a user.
#[allow(clippy::items_after_statements)]
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
	return env::var_os("HOME")
		.and_then(|h| if h.is_empty() { None } else { Some(h) })
		.or_else(|| unsafe { fallback() })
		.map(PathBuf::from);

	#[cfg(any(target_os = "android", target_os = "ios", target_os = "emscripten"))]
	unsafe fn fallback() -> Option<OsString> {
		None
	}
	#[cfg(not(any(target_os = "android", target_os = "ios", target_os = "emscripten")))]
	#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
	unsafe fn fallback() -> Option<OsString> {
		let amt = match libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) {
			n if n < 0 => 512 as usize,
			n => n as usize,
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
