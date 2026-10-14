//! Plain Win32 windows, each on its own thread with its own message loop.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GetForegroundWindow, GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassW,
    TranslateMessage, WINDOW_EX_STYLE, WM_DESTROY, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::{HSTRING, PCWSTR, w};

const CLASS_NAME: PCWSTR = w!("VmtestTestWindow");

/// How long a new window may take to become the foreground window before `open` returns anyway.
const FOREGROUND_LIMIT: Duration = Duration::from_secs(2);

/// Opens a normal top-level window titled `vmtest test window <id>` and returns its handle once it exists
/// and has become the foreground window, or after `FOREGROUND_LIMIT`.
///
/// Waiting for the foreground makes windows opened one after another land in the same places on every run:
/// a window manager places a new window beside the focused one. The window gets a thread of its own, so
/// that later it can stop processing messages without affecting the others.
pub fn open(id: u32) -> Result<i64, String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let created = create(id);
        let succeeded = created.is_ok();
        let _ = sender.send(created);
        if succeeded {
            pump_messages();
        }
    });
    let handle = receiver
        .recv()
        .map_err(|_| format!("the thread of window {id} ended before creating it"))??;
    wait_for_foreground(handle);
    Ok(handle)
}

fn wait_for_foreground(handle: i64) {
    let start = Instant::now();
    // SAFETY: `GetForegroundWindow` has no preconditions.
    while unsafe { GetForegroundWindow() }.0 as i64 != handle && start.elapsed() < FOREGROUND_LIMIT
    {
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn create(id: u32) -> Result<i64, String> {
    // SAFETY: Registering the same class again fails harmlessly; the class outlives every window of it.
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|error| error.to_string())?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_procedure),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class);
        let title = HSTRING::from(format!("vmtest test window {id}"));
        let window = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            &title,
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            640,
            480,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|error| format!("CreateWindowExW failed: {error}"))?;
        Ok(window.0 as i64)
    }
}

fn pump_messages() {
    let mut message = MSG::default();
    // SAFETY: `message` is a valid `MSG` for the whole loop.
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

extern "system" fn window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: The arguments are the ones Windows passed to this window procedure.
    unsafe {
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}
