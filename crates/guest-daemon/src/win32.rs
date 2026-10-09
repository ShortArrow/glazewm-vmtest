//! Reads the guest's displays through the Win32 API.

use vmtest_protocol::{DisplayDto, RectDto};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::core::BOOL;

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
    let name_length = info
        .szDevice
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(info.szDevice.len());
    Ok(DisplayDto {
        device_name: String::from_utf16_lossy(&info.szDevice[..name_length]),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs an interactive desktop session"]
    fn reads_at_least_one_display_and_exactly_one_primary() {
        let displays = Win32Topology.displays().unwrap();

        assert!(!displays.is_empty());
        assert_eq!(displays.iter().filter(|d| d.is_primary).count(), 1);
    }
}
