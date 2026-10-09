//! Entry point of the guest daemon: `vmtest-guest-daemon [listen-address]`, default `0.0.0.0:7041`.
//!
//! The SUT's client is expected at `glazewm\glazewm-cli.exe` beside the daemon's executable.

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::io::BufReader;
    use std::net::TcpListener;
    use std::sync::Arc;
    use vmtest_guest_daemon::process::ProcessGlazewmCli;
    use vmtest_guest_daemon::win32::{self, Win32Topology};
    use vmtest_guest_daemon::{CommandOutput, GlazewmCli, TopologySource, serve};
    use vmtest_protocol::DisplayDto;

    struct LiveGuest {
        cli: ProcessGlazewmCli,
    }

    impl TopologySource for LiveGuest {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            Win32Topology.displays()
        }
    }

    impl GlazewmCli for LiveGuest {
        fn run(&self, args: &[String]) -> Result<CommandOutput, String> {
            self.cli.run(args)
        }
    }

    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:7041".into());
    if let Err(error) = win32::make_dpi_aware() {
        eprintln!("{error}");
    }
    let executable = std::env::current_exe()?
        .parent()
        .map(|dir| dir.join("glazewm").join("glazewm-cli.exe"))
        .ok_or_else(|| std::io::Error::other("the daemon's executable has no parent directory"))?;
    let guest = Arc::new(LiveGuest {
        cli: ProcessGlazewmCli { executable },
    });
    let listener = TcpListener::bind(&address)?;
    eprintln!("listening on {address}");
    for stream in listener.incoming() {
        let stream = stream?;
        let guest = Arc::clone(&guest);
        std::thread::spawn(move || {
            let result = stream
                .try_clone()
                .and_then(|reader| serve(BufReader::new(reader), stream, guest.as_ref()));
            if let Err(error) = result {
                eprintln!("connection ended: {error}");
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
