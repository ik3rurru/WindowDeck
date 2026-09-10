//! Bounded FIFO for encoded data. Overflow expires the session; it never drops a
//! dependent H.264 fragment and continues feeding a corrupted bitstream.
use std::io;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

pub const MAX_AGE: Duration = Duration::from_millis(250);
pub const CAPACITY: usize = 2;

pub struct Queued<T> {
    pub produced: Instant,
    pub value: T,
}

impl<T> Queued<T> {
    pub fn into_fresh(self) -> io::Result<T> {
        self.into_fresh_within(MAX_AGE)
    }

    pub fn into_fresh_within(self, max_age: Duration) -> io::Result<T> {
        if self.produced.elapsed() > max_age {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "encoded queue expired; restart the session",
            ));
        }
        Ok(self.value)
    }
}

pub fn channel<T>() -> (SyncSender<Queued<T>>, Receiver<Queued<T>>) {
    mpsc::sync_channel(CAPACITY)
}

pub fn send<T>(sender: &SyncSender<Queued<T>>, value: T) -> io::Result<()> {
    send_with_timeout(sender, value, MAX_AGE)
}

pub fn send_with_timeout<T>(
    sender: &SyncSender<Queued<T>>,
    value: T,
    timeout: Duration,
) -> io::Result<()> {
    let mut pending = Queued {
        produced: Instant::now(),
        value,
    };
    loop {
        match sender.try_send(pending) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Disconnected(_)) => return Err(io::ErrorKind::BrokenPipe.into()),
            Err(TrySendError::Full(item)) => pending = item,
        }
        if pending.produced.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "encoded consumer stalled; restart the session",
            ));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn congestion_never_discards_encoded_bytes() {
        let (tx, rx) = channel();
        send(&tx, 1).unwrap();
        send(&tx, 2).unwrap();
        assert_eq!(send(&tx, 3).unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(rx.recv().unwrap().value, 1);
        assert_eq!(rx.recv().unwrap().value, 2);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn old_data_expires_even_when_the_queue_has_space() {
        let item = Queued {
            produced: Instant::now() - MAX_AGE - Duration::from_millis(1),
            value: 1,
        };
        assert_eq!(
            item.into_fresh().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
}
