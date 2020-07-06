use async_std::task::{Context, Poll};
use color_eyre::{eyre::eyre, Result};
use pin_project_lite::pin_project;
use smol::Timer;
use std::{future::Future, pin::Pin, time::Duration};
use tracing::warn;

/// Awaits a future or times out after a duration of time.
///
/// # Errors
///
/// - When the timeout is hit.
pub async fn timeout_with_log_msg<F, T>(
	log_msg: String,
	log_dur: Duration,
	dur: Duration,
	fut: F,
) -> Result<T>
where
	F: Future<Output = T>,
{
	TimeoutFuture::new(fut, dur, log_dur, log_msg).await
}

pin_project! {
  /// A future that times out after a duration of time.
  pub struct TimeoutFuture<F> {
	#[pin]
	future: F,
	#[pin]
	follow_up_delay: Timer,
	#[pin]
	log_delay: Timer,
	log_msg: String,
	has_logged: bool,
  }
}

impl<F> TimeoutFuture<F> {
	pub fn new(future: F, dur: Duration, log_dur: Duration, log_msg: String) -> TimeoutFuture<F> {
		Self {
			future,
			follow_up_delay: Timer::after(dur),
			log_delay: Timer::after(log_dur),
			log_msg,
			has_logged: false,
		}
	}
}

impl<F: Future> Future for TimeoutFuture<F> {
	type Output = Result<F::Output>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let this = self.project();

		match this.future.poll(cx) {
			Poll::Ready(v) => Poll::Ready(Ok(v)),
			Poll::Pending => {
				if *this.has_logged {
					match this.follow_up_delay.poll(cx) {
						Poll::Ready(_) => Poll::Ready(Err(eyre!("future timeout"))),
						Poll::Pending => Poll::Pending,
					}
				} else {
					match this.log_delay.poll(cx) {
						Poll::Ready(_) => {
							*this.has_logged = true;
							warn!("{}", this.log_msg);

							// Also check if we've hit follow_up_delay yet, if so we can short circut.
							// This also informs future's we want to be poll'd when this timer,
							// is hit.
							match this.follow_up_delay.poll(cx) {
								Poll::Ready(_) => Poll::Ready(Err(eyre!("future timeout"))),
								Poll::Pending => Poll::Pending,
							}
						}
						Poll::Pending => Poll::Pending,
					}
				}
			}
		}
	}
}
