//! Handles any logging utilities that we need in our crate for dev-loop.

use anyhow::Result;
use std::env::var as env_var;
use tracing::subscriber;
use tracing_log::LogTracer;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Describes the format of the logger.
///
/// It is controlled through `RUST_LOG_FORMAT`.
enum Format {
	/// Format the logs as a text based version. Default.
	Text,
}

/// Initialize the logging for this crate. Should be called at startup.
///
/// `format` - force a specific format. if `None`, falls back to:
///            `RUST_LOG_FORMAT`.
///
/// # Errors
///
/// If we fail to initialize the log tracer.
pub fn initialize_crate_logging(format: Option<String>) -> Result<()> {
	LogTracer::builder().ignore_crate("async_std").init()?;

	let chosen_format = match format
		.unwrap_or_else(|| env_var("RUST_LOG_FORMAT").unwrap_or_else(|_| "".to_owned()))
	{
		_ => Format::Text,
	};

	match chosen_format {
		Format::Text => {
			let subscriber = FmtSubscriber::builder()
				.with_env_filter(EnvFilter::from_default_env())
				.finish();

			subscriber::set_global_default(subscriber)?;

			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Tests are meant to prove that dev-loop works on a platform.
	///
	/// `initialize_crate_logging()` should always pass on a supported platform.
	#[test]
	fn can_get_home_directory() {
		let logging = initialize_crate_logging(None);
		assert!(logging.is_ok());
	}
}
