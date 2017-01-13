extern crate env_logger;
extern crate getopts;
extern crate libc;
#[macro_use]
extern crate log;

mod time;
mod wakeup_sources;

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process;
use std::thread::sleep;
use std::time::Duration;

use env_logger::LogBuilder;
use getopts::Options;
use log::LogLevelFilter;

use ::time::MonotonicTimeMS;

struct PowerState {
    state_file: Option<File>,
    state: String,
}

impl PowerState {
    fn new(dry_run: bool, state: Option<String>) -> Self {
        let mut file = File::open("/sys/power/state").expect("Failed to open state file");
        let mut states = String::new();
        file.read_to_string(&mut states).expect("Failed to read states");

        let state = if let Some(state) = state {
            if states.split_whitespace().any(|kstate| kstate == state) {
                // We requested a state and it is supported
                state
            } else {
                panic!("Requested state {} not supported by the kernel. Have {}.",
                       state,
                       states)
            }
        } else if let Some(state) = states.split_whitespace()
            .find(|kstate| kstate == &"mem") {
            // Prefer mem
            state.to_string()
        } else if let Some(state) = states.split_whitespace()
            .find(|kstate| kstate == &"freeze") {
            // Then prefer freeze
            state.to_string()
        } else if let Some(state) = states.split_whitespace().next() {
            // Else take the first available option
            state.to_string()
        } else {
            panic!("No suspend states supported by the kernel")
        };
        info!("Using '{}' suspend mode", state);

        PowerState {
            state_file: if !dry_run { Some(file) } else { None },
            state: state,
        }
    }

    fn sleep(&mut self) -> bool {
        if let Some(ref mut file) = self.state_file {
            if let Err(e) = file.write_all(self.state.as_bytes()) {
                match e.raw_os_error() {
                    Some(libc::EBUSY) => {
                        // EBUSY is acceptable
                        debug!("Suspend write failed with EBUSY");
                        false
                    }
                    _ => Err(e).expect("state write failed"),
                }
            } else {
                true
            }
        } else {
            // No file. Dry-run mode
            println!("Would have attempted sleep");
            true
        }
    }
}

struct WakeupCount {
    file: File,
    count: String,
}

impl WakeupCount {
    /// Open wakeup_count and read the count
    fn get() -> Self {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/sys/power/wakeup_count")
            .expect("Failed to open wakeup_count");
        // More than enough to hold the kernel's %u format string
        let mut count = String::with_capacity(32);
        file.read_to_string(&mut count).expect("Failed to read wakeup_count");
        WakeupCount {
            file: file,
            count: count,
        }
    }

    /// Write the previous count and close the file
    ///
    /// Returns true if successfully written, meaning there were no new wakeup
    /// sources since get().
    fn put(mut self) -> bool {
        if let Err(e) = self.file.write_all(self.count.as_bytes()) {
            match e.raw_os_error() {
                Some(libc::EINVAL) => {
                    debug!("Wakeup occurred source since read");
                    false
                }
                Some(libc::EBUSY) => {
                    error!("Kernel autosleep enabled");
                    false
                }
                _ => Err(e).expect("wakeup_count write failed"),
            }
        } else {
            true
        }
    }
}

fn apply_hysteresis(hysteresis: Duration) {
    let event = wakeup_sources::get_most_recent_event();

    if let Some(event) = event {
        let last = event.last_change;
        let now = MonotonicTimeMS::now();
        info!("Last wakeup event at {} ({})",
              event.last_change,
              event.name);
        info!("Current time         {}", now);
        assert!(now >= last);
        let delta = now - last;
        info!("Delta                {:?}", delta);

        if delta < hysteresis {
            let delay = hysteresis - delta;
            info!("Delaying for         {:?}", delay);
            sleep(delay);
        }
    } else {
        info!("No wakeup sources");
    }
}

fn usage(opts: Options) -> ! {
    print!("{}", opts.usage("insomniad [OPTION...]"));
    process::exit(1);
}

fn main() {
    let mut log_builder = LogBuilder::new();

    let mut opts = Options::new();
    opts.optflag("h", "help", "print this help");
    opts.optflag("n",
                 "dry-run",
                 "dry run. Emit message instead of suspending");
    opts.optflag("v", "verbose", "Enable verbose messages");
    opts.optflag("d", "debug", "Enable debug messages (implies -v)");
    opts.optopt("s",
                "state",
                "Suspend mode. 'mem' (default), 'freeze', etc...",
                "STATE");
    opts.optopt("t",
                "timeout",
                concat!("hysteresis time in ms. Defaults to 10000 (10 seconds).\n",
                        "Will not sleep until at least this much time has elapsed since the \
                         last wakeup event"),
                "TIMEOUT_MS");
    let matches = if let Ok(matches) = opts.parse(env::args().skip(1)) {
        matches
    } else {
        usage(opts);
    };
    if matches.opt_present("h") {
        usage(opts);
    }

    if matches.opt_present("d") {
        log_builder.filter(None, LogLevelFilter::Debug);
    } else if matches.opt_present("v") {
        log_builder.filter(None, LogLevelFilter::Info);
    } else if let Ok(log) = env::var("RUST_LOG") {
        log_builder.parse(&log);
    } else {
        log_builder.filter(None, LogLevelFilter::Error);
    }
    log_builder.init().unwrap();

    let timeout = if let Some(timeout) = matches.opt_str("t") {
        timeout.parse().unwrap_or_else(|e| {
            error!("Failed to parse timeout as integer. {}", e);
            usage(opts)
        })
    } else {
        10000
    };
    info!("Using hysteresis time of {}ms", timeout);
    let timeout = Duration::from_millis(timeout);

    let mut state = PowerState::new(matches.opt_present("n"), matches.opt_str("s"));
    loop {
        let count = WakeupCount::get();

        apply_hysteresis(timeout);

        if !count.put() {
            // Wake events occurred since get(). Restart.
            continue;
        }

        info!("Going to sleep");
        if !state.sleep() {
            info!("Non-fatal error writing to suspend state");
            // Rate-limit suspend attempts to avoid thrashing aborted suspends
            sleep(Duration::from_secs(1));
            continue;
        }
        info!("Exited sleep");

        // Sleep for at least the requested timeout after wake, emulating a
        // wakeup source on resume. Normally this would be taken care of by the
        // kernel generating a wakeup event on resume and we would make sure
        // not to sleep until it is released + our timeout. This makes sure our
        // timeout is applied to spurious wakeups as well. There is no point in
        // polling after resume until at least the timeout has elapsed.

        sleep(timeout);
    }
}
