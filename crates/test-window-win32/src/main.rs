//! Entry point of the Win32 test-window application: `vmtest-test-window-win32 <daemon-address>`.
//!
//! Connects to the guest daemon at the address, carries out its commands, and exits when the connection
//! closes, taking its windows with it. Built without a console, so that only the test windows appear.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::io::BufReader;
    use std::net::TcpStream;
    use vmtest_test_window_win32::serve;
    use vmtest_test_window_win32::window::Win32Windows;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };

    let address = std::env::args()
        .nth(1)
        .ok_or_else(|| std::io::Error::other("usage: vmtest-test-window-win32 <daemon-address>"))?;
    // SAFETY: Called once at start-up, before any window exists.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let stream = TcpStream::connect(address)?;
    serve(
        BufReader::new(stream.try_clone()?),
        stream,
        &mut Win32Windows::default(),
    )
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 test-window application runs only on Windows");
    std::process::exit(1);
}
