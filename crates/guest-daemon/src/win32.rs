//! Reads the guest's displays and processes through the Win32 API.

use std::path::Path;

use vmtest_protocol::{DisplayDto, RectDto};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::core::{BOOL, PWSTR};

use crate::TopologySource;

/// The `MONITORINFO.dwFlags` bit marking the primary monitor.
const MONITORINFOF_PRIMARY: u32 = 1;

/// Makes the process per-monitor DPI aware (v2), so that coordinates are physical pixels.
pub fn make_dpi_aware() -> Result<(), String> {
    // SAFETY: Called once at start-up, before any window or monitor handle is used.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        .map_err(|error| format!("SetProcessDpiAwarenessContext failed: {error}"))
}

/// Reads displays with `EnumDisplayMonitors` from the session the process runs in.
pub struct Win32Topology;

impl TopologySource for Win32Topology {
    fn displays(&self) -> Result<Vec<DisplayDto>, String> {
        let mut monitors: Vec<HMONITOR> = Vec::new();
        // SAFETY: `collect_monitor` only casts `data` back to the `Vec` passed here, which outlives the call.
        let enumerated = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(collect_monitor),
                LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
            )
        };
        if !enumerated.as_bool() {
            return Err("EnumDisplayMonitors failed".into());
        }
        monitors.into_iter().map(describe).collect()
    }
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `Vec<HMONITOR>` passed by `Win32Topology::displays`, alive for the enumeration.
    let monitors = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    monitors.push(monitor);
    true.into()
}

fn describe(monitor: HMONITOR) -> Result<DisplayDto, String> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a correctly sized `MONITORINFOEXW`, as `cbSize` declares.
    let read = unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) };
    if !read.as_bool() {
        return Err("GetMonitorInfoW failed".into());
    }
    Ok(DisplayDto {
        device_name: utf16_until_nul(&info.szDevice),
        bounds: rect(info.monitorInfo.rcMonitor),
        working_area: rect(info.monitorInfo.rcWork),
        is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

fn rect(r: RECT) -> RectDto {
    RectDto {
        x: r.left,
        y: r.top,
        width: (r.right - r.left).unsigned_abs(),
        height: (r.bottom - r.top).unsigned_abs(),
    }
}

/// Says whether a process whose executable is `image` is running, comparing full paths without regard to case.
///
/// Matching the path rather than the file name keeps a glazewm the guest may have installed itself from
/// being taken for the one the harness deployed. Processes this user may not query are not that one.
pub fn image_is_running(image: &Path) -> Result<bool, String> {
    let wanted = image.to_string_lossy().to_lowercase();
    let Some(file_name) = image
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
    else {
        return Err(format!("{} names no executable", image.display()));
    };
    for (pid, exe_file) in processes()? {
        if exe_file.to_lowercase() == file_name
            && image_path(pid).is_some_and(|path| path.to_lowercase() == wanted)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Lists every process as its id and executable file name.
fn processes() -> Result<Vec<(u32, String)>, String> {
    // SAFETY: The snapshot handle is closed below and not used after.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| format!("CreateToolhelp32Snapshot failed: {error}"))?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = Vec::new();
    // SAFETY: `entry` is a correctly sized `PROCESSENTRY32W`, as `dwSize` declares.
    let mut next = unsafe { Process32FirstW(snapshot, &mut entry) };
    while next.is_ok() {
        found.push((entry.th32ProcessID, utf16_until_nul(&entry.szExeFile)));
        // SAFETY: As for `Process32FirstW`.
        next = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    close(snapshot);
    Ok(found)
}

/// The full path of the executable of process `pid`, or `None` when it cannot be queried.
fn image_path(pid: u32) -> Option<String> {
    // SAFETY: The process handle is closed below and not used after.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: `length` holds the capacity of `buffer`, which outlives the call.
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    close(process);
    queried
        .ok()
        .map(|()| String::from_utf16_lossy(&buffer[..length as usize]))
}

fn close(handle: HANDLE) {
    // SAFETY: `handle` was opened by this module and is closed once.
    let _ = unsafe { CloseHandle(handle) };
}

fn utf16_until_nul(text: &[u16]) -> String {
    let length = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..length])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_running_test_executable_by_its_path() {
        let path = std::env::current_exe().unwrap();

        assert_eq!(image_is_running(&path), Ok(true));
    }

    #[test]
    fn finds_no_process_for_an_executable_that_is_not_running() {
        let path = std::env::current_exe()
            .unwrap()
            .with_file_name("vmtest-not-running.exe");

        assert_eq!(image_is_running(&path), Ok(false));
    }

    #[test]
    #[ignore = "needs an interactive desktop session"]
    fn reads_at_least_one_display_and_exactly_one_primary() {
        let displays = Win32Topology.displays().unwrap();

        assert!(!displays.is_empty());
        assert_eq!(displays.iter().filter(|d| d.is_primary).count(), 1);
    }
}
