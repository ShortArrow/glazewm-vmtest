//! Entry point of the guest daemon: `vmtest-guest-daemon [listen-address]`, default `127.0.0.1:7041`.
//!
//! The SUT is expected beside the daemon's executable: glazewm at `glazewm\glazewm.exe`, its configuration at
//! `glazewm\config.yaml` and its client at `glazewm\glazewm-cli.exe`, and zebar at `zebar\zebar.exe` with its
//! configuration directory at `zebar\config`;
//! the Win32 test-window application at `vmtest-test-window-win32.exe` beside it, and the load generator
//! at `vmtest-load.exe`. Limits on requests
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
    use vmtest_guest_daemon::load::LoadProcesses;
    use vmtest_guest_daemon::process::{DeployedSut, ProcessGlazewmCli};
    use vmtest_guest_daemon::test_windows::Batch;
    use vmtest_guest_daemon::test_windows::LaunchedApp;
    use vmtest_guest_daemon::win32::{self, Win32Topology};
    use vmtest_guest_daemon::{
        CliOutcome, DisplayPower, GlazewmCli, LoadHost, Pointer, SutProcesses, TestWindowHost,
        TopologySource, listen_address, serve,
    };
    use vmtest_protocol::{DisplayDto, LoadKindDto, PresenceDto, SutProgram, TestWindowDto};

    struct LiveGuest {
        cli: ProcessGlazewmCli,
        glazewm: DeployedSut,
        zebar: DeployedSut,
        test_windows: LaunchedApp,
        loads: LoadProcesses,
    }

    impl LoadHost for LiveGuest {
        fn start_load(&self, kind: LoadKindDto, percent: u8) -> Result<(), String> {
            self.loads.start(kind, percent)
        }

        fn stop_load(&self, kind: LoadKindDto) -> Result<(), String> {
            self.loads.stop(kind)
        }
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

    impl LiveGuest {
        fn sut(&self, program: SutProgram) -> &DeployedSut {
            match program {
                SutProgram::Glazewm => &self.glazewm,
                SutProgram::Zebar => &self.zebar,
            }
        }
    }

    impl SutProcesses for LiveGuest {
        fn is_running(&self, program: SutProgram) -> Result<bool, String> {
            win32::image_is_running(&self.sut(program).executable())
        }

        fn start(&self, program: SutProgram) -> Result<(), String> {
            self.sut(program).start()
        }
    }

    impl Pointer for LiveGuest {
        fn click(&self, x: i32, y: i32) -> Result<(), String> {
            win32::click(x, y)
        }

        fn foreground_window(&self) -> Result<Option<i64>, String> {
            Ok(win32::foreground_window())
        }
    }

    impl DisplayPower for LiveGuest {
        fn set_display_power(&self, on: bool) -> Result<(), String> {
            win32::set_display_power(on)
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
    let address = listen_address(std::env::args().nth(1));
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
    let guest = Arc::new(LiveGuest {
        cli: ProcessGlazewmCli {
            executable: directory.join("glazewm").join("glazewm-cli.exe"),
        },
        glazewm: DeployedSut::new(&directory, SutProgram::Glazewm),
        zebar: DeployedSut::new(&directory, SutProgram::Zebar),
        test_windows: LaunchedApp::new(directory.join("vmtest-test-window-win32.exe"), limits),
        loads: LoadProcesses::new(directory.join("vmtest-load.exe")),
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
