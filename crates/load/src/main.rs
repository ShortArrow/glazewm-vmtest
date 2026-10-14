//! Entry point of the load generator: `vmtest-load <cpu|memory> <percent>`, running until it is killed.
//!
//! Built without a console, so that no window of its own appears on the desktop under test.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), String> {
    use vmtest_load::{Kind, bytes_to_hold, keep_memory_in_use, keep_processor_busy, parse_args};
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let request = parse_args(&args)?;
    match request.kind {
        Kind::Cpu => {
            let processors = std::thread::available_parallelism().map_or(1, usize::from);
            let threads: Vec<_> = (0..processors)
                .map(|_| std::thread::spawn(move || keep_processor_busy(request.percent)))
                .collect();
            for thread in threads {
                let _ = thread.join();
            }
            Ok(())
        }
        Kind::Memory => {
            let mut status = MEMORYSTATUSEX {
                dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            // SAFETY: `status` is a correctly sized `MEMORYSTATUSEX`, as `dwLength` declares.
            unsafe { GlobalMemoryStatusEx(&mut status) }
                .map_err(|error| format!("GlobalMemoryStatusEx failed: {error}"))?;
            keep_memory_in_use(bytes_to_hold(request.percent, status.ullTotalPhys)).map(|_| ())
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the load generator runs only on Windows guests");
    std::process::exit(1);
}
