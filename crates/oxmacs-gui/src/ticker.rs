use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{future, thread};

use iced::Subscription;
use iced::futures::Stream;
use iced::time::Instant;

pub const MINUTE: Duration = Duration::from_secs(60);
pub const SECOND: Duration = Duration::from_secs(1);

pub fn ticks(period: Option<Duration>) -> Subscription<()> {
    match period {
        Some(period) => Subscription::run_with(period, tick),
        None => Subscription::none(),
    }
}

pub fn once_after(delay: Duration, since: Instant) -> Subscription<()> {
    Subscription::run_with((delay, since), once)
}

pub fn at(deadline: Instant) -> Subscription<()> {
    Subscription::run_with(deadline, until)
}

fn until(deadline: &Instant) -> impl Stream<Item = ()> + use<> {
    let deadline = *deadline;
    iced::stream::channel(1, async move |mut sender| {
        thread::spawn(move || {
            thread::sleep(deadline.saturating_duration_since(Instant::now()));
            let _ = sender.try_send(());
        });
        future::pending::<()>().await;
    })
}

fn once((delay, _): &(Duration, Instant)) -> impl Stream<Item = ()> + use<> {
    let delay = *delay;
    iced::stream::channel(1, async move |mut sender| {
        thread::spawn(move || {
            thread::sleep(delay);
            let _ = sender.try_send(());
        });
        future::pending::<()>().await;
    })
}

fn tick(period: &Duration) -> impl Stream<Item = ()> + use<> {
    let period = *period;
    iced::stream::channel(1, async move |mut sender| {
        thread::spawn(move || {
            while !sender.is_closed() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                thread::sleep(until_next(now, period));
                let _ = sender.try_send(());
            }
        });
        future::pending::<()>().await;
    })
}

pub fn until_next(now: Duration, period: Duration) -> Duration {
    let period = period.as_millis().max(1);
    let left = period - now.as_millis() % period;
    Duration::from_millis(u64::try_from(left).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_land_on_period_boundaries() {
        assert_eq!(
            until_next(Duration::from_secs(125), MINUTE),
            Duration::from_secs(55)
        );
        assert_eq!(until_next(Duration::from_secs(120), MINUTE), MINUTE);
        assert_eq!(
            until_next(Duration::from_millis(1_300), SECOND),
            Duration::from_millis(700)
        );
    }
}
