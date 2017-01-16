use std::fs::File;
use std::io::{BufRead, BufReader, Read};

use ::time::MonotonicTimeMS;

#[derive(Debug, PartialEq)]
pub struct WakeupSource {
    pub name: String,
    pub active_count: u64,
    pub event_count: u64,
    pub wakeup_count: u64,
    pub expire_count: u64,
    pub active_since: MonotonicTimeMS,
    pub total_time: MonotonicTimeMS,
    pub max_time: MonotonicTimeMS,
    pub last_change: MonotonicTimeMS,
    pub prevent_suspend_time: MonotonicTimeMS,
}

macro_rules! field {
    ($fields:ident, $name:ident) => {
        $fields.next().expect(concat!("Missing field: ", stringify!($name)))
            .trim_right()
            .parse().expect(concat!("Parsing field ", stringify!($name), " failed"))
    }
}

fn parse_wakeup_source(line: &str) -> WakeupSource {
    let mut fields = line.split('\t').filter(|field| field.len() > 0);
    WakeupSource {
        name: field!(fields, name),
        active_count: field!(fields, active_count),
        event_count: field!(fields, event_count),
        wakeup_count: field!(fields, wakeup_count),
        expire_count: field!(fields, expire_count),
        active_since: field!(fields, active_since),
        total_time: field!(fields, total_time),
        max_time: field!(fields, max_time),
        last_change: field!(fields, last_change),
        prevent_suspend_time: field!(fields, prevent_suspend_time),
    }
}

#[test]
fn test_parse_wakeup_source() {
    let ws = parse_wakeup_source("source\t0\t0\t0\t0\t0\t0\t0\t500\t0\n");
    assert_eq!(ws,
               WakeupSource {
                   name: "source".to_string(),
                   active_count: 0u64.into(),
                   event_count: 0u64.into(),
                   wakeup_count: 0u64.into(),
                   expire_count: 0u64.into(),
                   active_since: 0u64.into(),
                   total_time: 0u64.into(),
                   max_time: 0u64.into(),
                   last_change: 500u64.into(),
                   prevent_suspend_time: 0u64.into(),
               });
}

fn most_recent_event<R: Read>(file: R) -> Option<WakeupSource> {
    const HEADER: &'static str = concat!("name\t\t",
                                         "active_count\tevent_count\twakeup_count\t",
                                         "expire_count\tactive_since\ttotal_time\t",
                                         "max_time\tlast_change\tprevent_suspend_time");

    let mut lines = BufReader::new(file).lines();

    let header = lines.next().expect("No header").expect("Read header failed");
    // Make sure we're reading the file we expect
    assert_eq!(header, HEADER);

    // Return the most recent
    lines.map(|line| parse_wakeup_source(&line.expect("Read sources line failed")))
        .max_by_key(|source| source.last_change)
}

#[test]
fn test_most_recent() {
    use std::io::Cursor;

    let cursor = Cursor::new(&include_bytes!("wakeup_sources")[..]);
    let ws = most_recent_event(cursor).unwrap();
    assert_eq!(&ws.name, "foo");
    assert_eq!(ws.last_change, 2229000u64.into());
}

pub fn get_most_recent_event() -> Option<WakeupSource> {
    let f = File::open("/sys/kernel/debug/wakeup_sources").expect("Failed to open wakeup_sources");
    most_recent_event(f)
}
