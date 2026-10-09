//! Entry point of the guest daemon: `vmtest-guest-daemon [listen-address]`, default `0.0.0.0:7041`.

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::io::BufReader;
    use std::net::TcpListener;
    use vmtest_guest_daemon::{serve, win32};

    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:7041".into());
    if let Err(error) = win32::make_dpi_aware() {
        eprintln!("{error}");
    }
    let listener = TcpListener::bind(&address)?;
    eprintln!("listening on {address}");
    for stream in listener.incoming() {
        let stream = stream?;
        std::thread::spawn(move || {
            let result = stream
                .try_clone()
                .and_then(|reader| serve(BufReader::new(reader), stream, &win32::Win32Topology));
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
