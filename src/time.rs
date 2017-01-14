use std::fmt::{self, Display, Formatter};
use std::io;
use std::num::ParseIntError;
use std::ops::Sub;
use std::str::FromStr;
use std::time::Duration;

use libc;

fn get_monotonic() -> io::Result<libc::timespec> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ts)
    }
}

/// Like std::time::Instant but with millisecond resolution and allowing
/// instantiation from a numeric value (via string parsing).
///
/// Instant stores monotonic time internally, but is too restrictive. We cannot
/// compare a wakeup source timestamp to it since its value is opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonotonicTimeMS(u64);

impl MonotonicTimeMS {
    pub fn now() -> Self {
        let ts = get_monotonic().expect("clock_gettime failed");
        let mut ms = ts.tv_sec as i64;
        ms *= 1000;
        ms += ts.tv_nsec / 1000 / 1000;
        MonotonicTimeMS(ms as u64)
    }
}

impl Display for MonotonicTimeMS {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}ms", self.0)
    }
}

impl FromStr for MonotonicTimeMS {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MonotonicTimeMS(try!(s.parse())))
    }
}

impl From<u64> for MonotonicTimeMS {
    fn from(val: u64) -> Self {
        MonotonicTimeMS(val)
    }
}

impl Sub for MonotonicTimeMS {
    type Output = Duration;

    fn sub(self, other: Self) -> Duration {
        Duration::from_millis(self.0 - other.0)
    }
}
