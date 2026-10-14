//! The load generator processes the guest daemon starts and stops.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use vmtest_protocol::LoadKindDto;

/// How long a started load generator must keep running before it counts as started.
const START_CHECK: Duration = Duration::from_millis(300);

/// At most one running load generator per kind of load.
pub struct LoadProcesses {
    executable: PathBuf,
    running: Mutex<HashMap<LoadKindDto, Child>>,
}

impl LoadProcesses {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Starts a load of `kind` at `percent`, stopping a running one of the same kind first.
    ///
    /// Returns once the generator has kept running for `START_CHECK`, so that one that rejects its
    /// arguments or cannot allocate is reported here rather than taken for a running load.
    pub fn start(&self, kind: LoadKindDto, percent: u8) -> Result<(), String> {
        if !(1..=100).contains(&percent) {
            return Err(format!(
                "a load must be from 1 to 100 percent, not {percent}"
            ));
        }
        let mut running = self
            .running
            .lock()
            .map_err(|_| "a previous request panicked")?;
        if let Some(previous) = running.remove(&kind) {
            end(previous)?;
        }
        let mut child = Command::new(&self.executable)
            .arg(match kind {
                LoadKindDto::Cpu => "cpu",
                LoadKindDto::Memory => "memory",
            })
            .arg(percent.to_string())
            .spawn()
            .map_err(|error| format!("cannot start the load generator: {error}"))?;
        std::thread::sleep(START_CHECK);
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot watch the load generator: {error}"))?
        {
            return Err(format!("the load generator exited at once with {status}"));
        }
        running.insert(kind, child);
        Ok(())
    }

    /// Stops the running load of `kind`, if any, and waits for its process to end.
    pub fn stop(&self, kind: LoadKindDto) -> Result<(), String> {
        let mut running = self
            .running
            .lock()
            .map_err(|_| "a previous request panicked")?;
        match running.remove(&kind) {
            Some(child) => end(child),
            None => Ok(()),
        }
    }
}

fn end(mut child: Child) -> Result<(), String> {
    let _ = child.kill();
    child
        .wait()
        .map(|_| ())
        .map_err(|error| format!("cannot wait for the load generator to end: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_a_percent_outside_1_to_100_without_starting_anything() {
        let loads = LoadProcesses::new("vmtest-no-such-load.exe".into());

        let error = loads.start(LoadKindDto::Cpu, 0).unwrap_err();

        assert!(error.contains("from 1 to 100"), "{error}");
    }

    #[test]
    fn reports_a_load_generator_that_cannot_be_started() {
        let loads = LoadProcesses::new("vmtest-no-such-load.exe".into());

        let error = loads.start(LoadKindDto::Memory, 50).unwrap_err();

        assert!(
            error.starts_with("cannot start the load generator"),
            "{error}"
        );
    }

    #[test]
    fn stopping_a_load_that_is_not_running_changes_nothing() {
        let loads = LoadProcesses::new("vmtest-no-such-load.exe".into());

        assert_eq!(loads.stop(LoadKindDto::Cpu), Ok(()));
    }
}
