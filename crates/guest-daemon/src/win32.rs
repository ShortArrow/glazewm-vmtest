//! Reads the guest's displays, processes and windows through the Win32 API.

use std::path::Path;

use vmtest_protocol::{DisplayDto, PresenceDto, RectDto};
use windows::Win32::Foundation::WPARAM;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    MONITORINFOEXW,
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
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongPtrW, HWND_BROADCAST, IsWindow, PostMessageW, SC_MONITORPOWER,
    WM_SYSCOMMAND, WS_CHILD,
};
use windows::core::{BOOL, PWSTR};

use crate::TopologySource;

/// Turns the displays off by asking every top-level window to, as the system does on idle; wakes them by
/// moving the mouse there and back, since the OS no longer turns displays on for `SC_MONITORPOWER` alone.
pub fn set_display_power(on: bool) -> Result<(), String> {
    if on {
        let nudge = |dx: i32| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dwFlags: MOUSEEVENTF_MOVE,
                    ..Default::default()
                },
            },
        };
        let inputs = [nudge(1), nudge(-1)];
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err(format!(
                "SendInput sent {sent} of {} inputs: {}",
                inputs.len(),
                windows::core::Error::from_thread()
            ))
        }
    } else {
        const MONITOR_OFF: isize = 2;
        unsafe {
            PostMessageW(
                Some(HWND_BROADCAST),
                WM_SYSCOMMAND,
                WPARAM(SC_MONITORPOWER as usize),
                LPARAM(MONITOR_OFF),
            )
        }
        .map_err(|error| format!("cannot ask the windows to turn the displays off: {error}"))
    }
}

/// The `MONITORINFO.dwFlags` bit marking the primary monitor.
const MONITORINFOF_PRIMARY: u32 = 1;

/// Makes the process per-monitor DPI aware (v2), so that coordinates are physical pixels.
pub fn make_dpi_aware() -> Result<(), String> {
    // SAFETY: Called once at start-up, before any window or monitor handle is used.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        .map_err(|error| format!("SetProcessDpiAwarenessContext failed: {error}"))
}

/// Reads displays with `EnumDisplayMonitors` from the session the process runs in.
///
/// The enumeration fails now and then while the OS is changing its displays, so a failure is retried a few times
/// before it is reported.
pub struct Win32Topology;

/// How many times, and how far apart, a failed display enumeration is tried again.
const ENUMERATION_RETRIES: u32 = 10;
const ENUMERATION_RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(100);

impl TopologySource for Win32Topology {
    fn displays(&self) -> Result<Vec<DisplayDto>, String> {
        let mut last_error = String::new();
        for _ in 0..=ENUMERATION_RETRIES {
            match monitors() {
                Ok(monitors) => return monitors.into_iter().map(describe).collect(),
                Err(error) => last_error = error,
            }
            std::thread::sleep(ENUMERATION_RETRY_PAUSE);
        }
        Err(last_error)
    }
}

fn monitors() -> Result<Vec<HMONITOR>, String> {
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
        return Err(format!(
            "EnumDisplayMonitors failed: {}",
            windows::core::Error::from_thread()
        ));
    }
    Ok(monitors)
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
    let device_name = utf16_until_nul(&info.szDevice);
    Ok(DisplayDto {
        adapter: adapter_of(&device_name)?,
        device_name,
        bounds: rect(info.monitorInfo.rcMonitor),
        working_area: rect(info.monitorInfo.rcWork),
        is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

/// The description of the display adapter whose output is `device_name`, such as `\\.\DISPLAY1`.
fn adapter_of(device_name: &str) -> Result<String, String> {
    let mut index = 0;
    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `device` is a correctly sized `DISPLAY_DEVICEW`, as `cb` declares.
        let found = unsafe { EnumDisplayDevicesW(None, index, &mut device, 0) };
        if !found.as_bool() {
            return Err(format!("no display adapter has the output {device_name}"));
        }
        if utf16_until_nul(&device.DeviceName) == device_name {
            return Ok(utf16_until_nul(&device.DeviceString));
        }
        index += 1;
    }
}

fn rect(r: RECT) -> RectDto {
    RectDto {
        x: r.left,
        y: r.top,
        width: (r.right - r.left).unsigned_abs(),
        height: (r.bottom - r.top).unsigned_abs(),
    }
}

/// Whether window `handle` exists and, if so, whether it is a top-level window rather than a child.
pub fn window_presence(handle: i64) -> PresenceDto {
    let window = HWND(handle as *mut std::ffi::c_void);
    // SAFETY: Both calls accept any value and report a window that does not exist.
    unsafe {
        if !IsWindow(Some(window)).as_bool() {
            return PresenceDto::Gone;
        }
        if GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_CHILD.0 != 0 {
            PresenceDto::Child
        } else {
            PresenceDto::TopLevel
        }
    }
}

/// The visible frame of window `handle`, without the invisible resize borders, or `None` when it does not exist.
pub fn window_frame(handle: i64) -> Option<RectDto> {
    let window = HWND(handle as *mut std::ffi::c_void);
    // SAFETY: `IsWindow` accepts any value; `frame` is a `RECT`, the size passed.
    unsafe {
        if !IsWindow(Some(window)).as_bool() {
            return None;
        }
        let mut frame = RECT::default();
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut RECT as *mut std::ffi::c_void,
            size_of::<RECT>() as u32,
        )
        .ok()?;
        Some(rect(frame))
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
    fn says_a_window_that_does_not_exist_is_gone() {
        assert_eq!(window_presence(0), PresenceDto::Gone);
    }

    #[test]
    fn says_the_desktop_window_is_top_level() {
        // SAFETY: `GetDesktopWindow` has no preconditions.
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };

        assert_eq!(window_presence(desktop.0 as i64), PresenceDto::TopLevel);
    }

    #[test]
    fn has_no_frame_for_a_window_that_does_not_exist() {
        assert_eq!(window_frame(0), None);
    }

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
