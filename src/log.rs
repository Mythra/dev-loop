//! Handles any logging utilities that we need in our crate for dev-loop.

use color_eyre::{config::HookBuilder, Result};
use lazy_static::lazy_static;
use std::sync::{
	atomic::{AtomicBool, Ordering},
	Arc,
};
use tracing::{event::Event, metadata::Metadata, subscriber::Interest, Subscriber};
use tracing_error::ErrorLayer;
use tracing_subscriber::{
	filter::LevelFilter,
	fmt::layer as fmt_layer,
	layer::{Context, Layer, SubscriberExt},
	util::SubscriberInitExt,
	EnvFilter,
};

lazy_static! {
	pub static ref HAS_OUTPUT_LOG_MSG: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

struct TracingSubscriber {}

impl<S: Subscriber> Layer<S> for TracingSubscriber {
	fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
		Interest::always()
	}

	fn on_event(&self, _: &Event, _: Context<S>) {
		HAS_OUTPUT_LOG_MSG.store(true, Ordering::Release);
	}
}

/// Describes the format of the logger.
///
/// It is controlled through `RUST_LOG_FORMAT`.
enum Format {
	/// Format the logs as a JSON Message.
	Json,
	/// Format the logs as a text based version. Default.
	Text,
}

/// Initialize the logging for this crate. Should be called at startup.
///
/// # Errors
///
/// - If we fail to initialize the log tracer.
/// - If `color_eyre` fails to initialize.
pub fn initialize_crate_logging() -> Result<()> {
	let chosen_format = match std::env::var("RUST_LOG_FORMAT")
		.as_ref()
		.map(String::as_str)
	{
		Ok("json") => Format::Json,
		_ => Format::Text,
	};

	let chosen_level = match std::env::var("RUST_LOG_LEVEL").as_ref().map(String::as_str) {
		Ok("off") => LevelFilter::OFF,
		Ok("error") => LevelFilter::ERROR,
		Ok("warn") => LevelFilter::WARN,
		Ok("debug") => LevelFilter::DEBUG,
		Ok("trace") => LevelFilter::TRACE,
		_ => LevelFilter::INFO,
	};

	let add_spantrace =
		!std::env::var("RUST_BACKTRACE")
			.unwrap_or_default()
			.is_empty() || !std::env::var("RUST_LIB_BACKTRACE")
			.unwrap_or_default()
			.is_empty() || !std::env::var("RUST_SPANTRACE")
			.unwrap_or_default()
			.is_empty();

	HookBuilder::new()
		.capture_span_trace_by_default(add_spantrace)
		.install()?;

	let filter_layer = EnvFilter::from_default_env().add_directive(chosen_level.into());
	let fmt_layer = fmt_layer().with_target(false);
	let tracing_layer = TracingSubscriber {};

	match chosen_format {
		Format::Text => {
			tracing_subscriber::registry()
				.with(filter_layer)
				.with(fmt_layer)
				.with(tracing_layer)
				.with(ErrorLayer::default())
				.init();
		}
		Format::Json => {
			tracing_subscriber::registry()
				.with(filter_layer)
				.with(fmt_layer.json())
				.with(tracing_layer)
				.with(ErrorLayer::default())
				.init();
		}
	};

	Ok(())
}
