//! Plain Win32 windows, each on its own thread with its own message loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GetForegroundWindow, GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_DESTROY, WNDCLASSW,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::WindowHost;
use crate::gate::Gate;

const CLASS_NAME: PCWSTR = w!("VmtestTestWindow");

/// The message that sends a window's thread into its gate.
const WM_HANG: u32 = WM_APP + 1;

/// How long a window's thread may take to reach its gate after `WM_HANG` is posted.
const HANG_LIMIT: Duration = Duration::from_secs(2);

thread_local! {
    /// The gate of the window that owns this thread.
    static GATE: RefCell<Option<Arc<Gate>>> = const { RefCell::new(None) };
}

/// The windows this application has opened, each with the gate that can stop it.
#[derive(Default)]
pub struct Win32Windows {
    opened: HashMap<u32, (HWND, Arc<Gate>)>,
}

impl WindowHost for Win32Windows {
    fn open(&mut self, id: u32) -> Result<i64, String> {
        let gate = Arc::new(Gate::default());
        let handle = open(id, Arc::clone(&gate))?;
        self.opened
            .insert(id, (HWND(handle as *mut std::ffi::c_void), gate));
        Ok(handle)
    }

    /// Posts `WM_HANG`, so that the window stops while handling a message, as a busy application does.
    fn hang(&mut self, id: u32) -> Result<(), String> {
        let (window, gate) = self.window(id)?;
        gate.close();
        // SAFETY: `window` is a window this application created; posting to a destroyed one fails harmlessly.
        unsafe { PostMessageW(Some(window), WM_HANG, WPARAM(0), LPARAM(0)) }
            .map_err(|error| format!("PostMessageW to window {id} failed: {error}"))?;
        if gate.wait_until_holding(HANG_LIMIT) {
            Ok(())
        } else {
            gate.open();
            Err(format!("window {id} did not stop within {HANG_LIMIT:?}"))
        }
    }

    fn resume(&mut self, id: u32) -> Result<(), String> {
        self.window(id)?.1.open();
        Ok(())
    }
}

impl Win32Windows {
    fn window(&self, id: u32) -> Result<(HWND, Arc<Gate>), String> {
        self.opened
            .get(&id)
            .map(|(window, gate)| (*window, Arc::clone(gate)))
            .ok_or_else(|| format!("no window {id} has been opened"))
    }
}

/// How long a new window may take to become the foreground window before `open` returns anyway.
const FOREGROUND_LIMIT: Duration = Duration::from_secs(2);

/// Opens a normal top-level window titled `vmtest test window <id>` and returns its handle once it exists
/// and has become the foreground window, or after `FOREGROUND_LIMIT`.
///
/// Waiting for the foreground makes windows opened one after another land in the same places on every run:
/// a window manager places a new window beside the focused one. The window gets a thread of its own, so
/// that later it can stop processing messages without affecting the others.
fn open(id: u32, gate: Arc<Gate>) -> Result<i64, String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        GATE.with(|own| *own.borrow_mut() = Some(gate));
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
        if message == WM_HANG {
            if let Some(gate) = GATE.with(|own| own.borrow().clone()) {
                gate.hold_while_closed();
            }
            return LRESULT(0);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}
