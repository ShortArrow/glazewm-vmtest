//! The load generator: keeps every logical processor, or the physical memory, busy until it is killed.

use std::time::{Duration, Instant};

/// What to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cpu,
    Memory,
}

/// A load to keep up, read from the command line `<cpu|memory> <percent>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadRequest {
    pub kind: Kind,
    /// From 1 to 100.
    pub percent: u8,
}

/// Reads the arguments after the program name.
pub fn parse_args(args: &[String]) -> Result<LoadRequest, String> {
    let [kind, percent] = args else {
        return Err("usage: vmtest-load <cpu|memory> <percent>".into());
    };
    let kind = match kind.as_str() {
        "cpu" => Kind::Cpu,
        "memory" => Kind::Memory,
        other => return Err(format!("unknown kind of load: {other}")),
    };
    let percent = percent
        .parse::<u8>()
        .ok()
        .filter(|percent| (1..=100).contains(percent))
        .ok_or_else(|| format!("the percent must be from 1 to 100, not {percent}"))?;
    Ok(LoadRequest { kind, percent })
}

/// How much of each `period` a processor spends busy and idle at `percent`.
pub fn duty_cycle(percent: u8, period: Duration) -> (Duration, Duration) {
    let busy = period * u32::from(percent) / 100;
    (busy, period - busy)
}

/// How many bytes to hold for `percent` of `total` bytes of physical memory.
pub fn bytes_to_hold(percent: u8, total: u64) -> u64 {
    total / 100 * u64::from(percent) + total % 100 * u64::from(percent) / 100
}

/// The period over which a CPU load alternates between busy and idle.
const PERIOD: Duration = Duration::from_millis(100);

/// Keeps one processor busy for `percent` of the time, forever.
pub fn keep_processor_busy(percent: u8) -> ! {
    let (busy, idle) = duty_cycle(percent, PERIOD);
    loop {
        let start = Instant::now();
        while start.elapsed() < busy {
            std::hint::spin_loop();
        }
        if !idle.is_zero() {
            std::thread::sleep(idle);
        }
    }
}

/// Holds `bytes` of memory and writes to every page of it each second, so that it stays in use, forever.
pub fn keep_memory_in_use(bytes: u64) -> Result<std::convert::Infallible, String> {
    const PAGE: usize = 4096;
    let length =
        usize::try_from(bytes).map_err(|_| format!("{bytes} bytes do not fit in memory"))?;
    let mut held = Vec::new();
    held.try_reserve_exact(length)
        .map_err(|error| format!("cannot allocate {bytes} bytes: {error}"))?;
    held.resize(length, 0u8);
    let mut round = 0u8;
    loop {
        round = round.wrapping_add(1);
        for page in held.chunks_mut(PAGE) {
            page[0] = round;
        }
        std::hint::black_box(&held);
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn reads_the_kind_and_percent_of_load() {
        assert_eq!(
            parse_args(&args(&["cpu", "100"])),
            Ok(LoadRequest {
                kind: Kind::Cpu,
                percent: 100
            })
        );
        assert_eq!(
            parse_args(&args(&["memory", "1"])),
            Ok(LoadRequest {
                kind: Kind::Memory,
                percent: 1
            })
        );
    }

    #[test]
    fn rejects_a_percent_outside_1_to_100_and_an_unknown_kind() {
        assert!(parse_args(&args(&["cpu", "0"])).is_err());
        assert!(parse_args(&args(&["cpu", "101"])).is_err());
        assert!(parse_args(&args(&["gpu", "50"])).is_err());
        assert!(parse_args(&args(&["cpu"])).is_err());
    }

    #[test]
    fn splits_a_period_into_busy_and_idle_time_by_percent() {
        let period = Duration::from_millis(100);

        assert_eq!(
            duty_cycle(30, period),
            (Duration::from_millis(30), Duration::from_millis(70))
        );
        assert_eq!(duty_cycle(100, period), (period, Duration::ZERO));
    }

    #[test]
    fn takes_the_percent_of_physical_memory() {
        assert_eq!(bytes_to_hold(70, 8 << 30), (8u64 << 30) * 70 / 100);
    }
}
