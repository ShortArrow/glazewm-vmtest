//! Entry point of the guest daemon: `vmtest-guest-daemon [listen-address]`, default `0.0.0.0:7041`.
//!
//! The SUT is expected at `glazewm\glazewm.exe` and its client at `glazewm\glazewm-cli.exe` beside the daemon's executable,
//! and the Win32 test-window application at `vmtest-test-window-win32.exe` beside it. Limits on requests
//! are read from `guest-daemon.json` beside it if that file exists; the daemon does not start if it is invalid.
//! Built without a console, so that no window of its own appears on the desktop under test;
//! it logs to `guest-daemon.log` beside its executable.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::io::BufReader;
    use std::net::TcpListener;
    use std::sync::Arc;
    use vmtest_guest_daemon::limits::Limits;
    use vmtest_guest_daemon::process::ProcessGlazewmCli;
    use vmtest_guest_daemon::test_windows::Batch;
    use vmtest_guest_daemon::test_windows::LaunchedApp;
    use vmtest_guest_daemon::win32::{self, Win32Topology};
    use vmtest_guest_daemon::{
        CliOutcome, GlazewmCli, SutProcesses, TestWindowHost, TopologySource, serve,
    };
    use vmtest_protocol::{DisplayDto, PresenceDto, SutProgram, TestWindowDto};

    struct LiveGuest {
        cli: ProcessGlazewmCli,
        glazewm: std::path::PathBuf,
        test_windows: LaunchedApp,
    }

    impl TestWindowHost for LiveGuest {
        fn open(&self, batch: &Batch) -> Result<Vec<u32>, String> {
            self.test_windows.open(batch)
        }

        fn presence(&self, handles: &[i64]) -> Vec<PresenceDto> {
            handles
                .iter()
                .map(|&handle| win32::window_presence(handle))
                .collect()
        }

        fn windows(&self) -> Result<Vec<TestWindowDto>, String> {
            Ok(self
                .test_windows
                .windows()?
                .into_iter()
                .map(|window| TestWindowDto {
                    id: window.id,
                    handle: window.handle,
                    frame: win32::window_frame(window.handle),
                    hung: window.hung,
                })
                .collect())
        }

        fn hang(&self, id: u32) -> Result<(), String> {
            self.test_windows.hang(id)
        }

        fn resume(&self, id: u32) -> Result<(), String> {
            self.test_windows.resume(id)
        }
    }

    impl SutProcesses for LiveGuest {
        fn is_running(&self, program: SutProgram) -> Result<bool, String> {
            match program {
                SutProgram::Glazewm => win32::image_is_running(&self.glazewm),
            }
        }
    }

    impl TopologySource for LiveGuest {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            Win32Topology.displays()
        }
    }

    impl GlazewmCli for LiveGuest {
        fn run(&self, args: &[String], limit: std::time::Duration) -> Result<CliOutcome, String> {
            self.cli.run(args, limit)
        }
    }

    let directory = std::env::current_exe()?
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| std::io::Error::other("the daemon's executable has no parent directory"))?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("guest-daemon.log"))?;
    let log = std::sync::Mutex::new(log_file);
    let log = Arc::new(move |message: String| {
        use std::io::Write;
        if let Ok(mut file) = log.lock() {
            let _ = writeln!(file, "{message}");
        }
    });
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:7041".into());
    if let Err(error) = win32::make_dpi_aware() {
        log(error);
    }
    let limits = match std::fs::read_to_string(directory.join("guest-daemon.json")) {
        Ok(text) => Limits::from_json(&text).map_err(|error| {
            log(error.clone());
            std::io::Error::other(error)
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Limits::default(),
        Err(error) => return Err(error),
    };
    let sut = directory.join("glazewm");
    let guest = Arc::new(LiveGuest {
        cli: ProcessGlazewmCli {
            executable: sut.join("glazewm-cli.exe"),
        },
        glazewm: sut.join("glazewm.exe"),
        test_windows: LaunchedApp::new(directory.join("vmtest-test-window-win32.exe"), limits),
    });
    let listener = TcpListener::bind(&address)?;
    log(format!("listening on {address}"));
    for stream in listener.incoming() {
        let stream = stream?;
        let guest = Arc::clone(&guest);
        let log = Arc::clone(&log);
        std::thread::spawn(move || {
            let result = stream
                .try_clone()
                .and_then(|reader| serve(BufReader::new(reader), stream, guest.as_ref()));
            if let Err(error) = result {
                log(format!("connection ended: {error}"));
            }
        });
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the guest daemon runs only on Windows guests");
    std::process::exit(1);
}
