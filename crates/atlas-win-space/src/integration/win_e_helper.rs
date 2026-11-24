//! Packaged (MSIX) Win+E integration using `windows.startupTask` + a lightweight
//! `WH_KEYBOARD_LL` shortcut helper. Writes zero registry keys and requires no
//! `unvirtualizedResources` capability; uninstalling the MSIX package automatically
//! terminates the helper and removes the startup task, restoring Windows Explorer's
//! default Win+E behavior immediately.
use std::{
    ffi::OsStr,
    io,
    mem::{size_of, zeroed},
    os::windows::{ffi::OsStrExt, process::CommandExt},
    process::Command,
    ptr::null,
    sync::{
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc, OnceLock,
    },
    time::{Duration, Instant},
};
use windows::{
    core::HSTRING,
    ApplicationModel::{StartupTask, StartupTaskState},
    Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{CreateMutexW, CREATE_NO_WINDOW},
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
            KEYEVENTF_KEYUP, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            AllowSetForegroundWindow, CallNextHookEx, CreateWindowExW, DefWindowProcW,
            DestroyWindow, DispatchMessageW, FindWindowExW, GetMessageW, PostMessageW,
            PostQuitMessage, RegisterClassExW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, UnregisterClassW, ASFW_ANY, HC_ACTION, HHOOK, HWND_MESSAGE,
            KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_APP, WM_CLOSE, WM_DESTROY,
            WM_ENDSESSION, WM_KEYDOWN, WM_KEYUP, WM_QUERYENDSESSION, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WNDCLASSEXW,
        },
    },
};

pub(super) const HELPER_ARG: &str = "--atlas-win-e-helper";
const STARTUP_TASK_ID: &str = "AtlasWinEHelper";
const MUTEX_NAME: &str = r"Local\AtlasWinEHelperMutex_CuberLabs";
const WINDOW_CLASS: &str = "AtlasWinEHelperWindow_CuberLabs";
const WM_LAUNCH_ATLAS: u32 = WM_APP + 1;
const VK_E: u32 = b'E' as u32;
/// Unassigned virtual key (same mask-key convention as AutoHotkey/PowerToys)
/// injected while Win is held so releasing Win does not trigger the Start menu.
const VK_WIN_MASK: u16 = 0xE8;
const ATLAS_MASK_EXTRA_INFO: usize = 0x4154_4C53_5745; // "ATLSWE"
const LAUNCH_DEBOUNCE: Duration = Duration::from_millis(250);
const HELPER_SYNC_TIMEOUT: Duration = Duration::from_millis(1500);

static HELPER_HWND: AtomicIsize = AtomicIsize::new(0);
static E_SUPPRESSED: AtomicBool = AtomicBool::new(false);
static LAUNCH_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

struct OwnedHook(HHOOK);
impl Drop for OwnedHook {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                UnhookWindowsHookEx(self.0);
            }
        }
    }
}

struct MtaScope(bool);
impl Drop for MtaScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Run WinRT `StartupTask` operations on a dedicated MTA thread so calling
/// `IAsyncOperation::get()` never blocks or deadlocks an STA UI thread.
fn run_on_mta<T: Send + 'static>(
    action: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    std::thread::spawn(move || {
        let _mta = MtaScope(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok());
        action()
    })
    .join()
    .map_err(|_| io::Error::other("Win+E startup task worker panicked"))?
}

fn winrt_err(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}

fn query_startup_task_state() -> io::Result<StartupTaskState> {
    run_on_mta(|| {
        let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
            .map_err(winrt_err)?
            .get()
            .map_err(winrt_err)?;
        task.State().map_err(winrt_err)
    })
}

fn is_state_enabled(state: StartupTaskState) -> bool {
    state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy
}

fn find_helper_window() -> HWND {
    let class_name = wide(WINDOW_CLASS);
    unsafe { FindWindowExW(HWND_MESSAGE, 0, class_name.as_ptr(), null()) }
}

fn spawn_helper_process() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg(HELPER_ARG)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}

fn wait_for_helper_state(want_running: bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let running = find_helper_window() != 0;
        if running == want_running {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn stop_helper_process() {
    let hwnd = find_helper_window();
    if hwnd != 0 {
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
        let _ = wait_for_helper_state(false, HELPER_SYNC_TIMEOUT);
    }
}

/// Read packaged Win+E status from the MSIX `StartupTask` and keep the
/// background helper synchronized (e.g. restarting it after a Store update or
/// stopping it if the user disabled the startup task in Windows Settings).
pub(super) fn packaged_win_e_enabled() -> io::Result<bool> {
    let enabled = is_state_enabled(query_startup_task_state()?);
    let running = find_helper_window() != 0;
    if enabled && !running {
        let _ = spawn_helper_process();
    } else if !enabled && running {
        let hwnd = find_helper_window();
        if hwnd != 0 {
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    }
    Ok(enabled)
}

/// Enable or disable the packaged Win+E shortcut helper via `StartupTask`.
pub(super) fn set_packaged_win_e_enabled(enabled: bool) -> io::Result<()> {
    if enabled {
        let state = run_on_mta(|| {
            let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
                .map_err(winrt_err)?
                .get()
                .map_err(winrt_err)?;
            let current = task.State().map_err(winrt_err)?;
            if current == StartupTaskState::Disabled {
                task.RequestEnableAsync()
                    .map_err(winrt_err)?
                    .get()
                    .map_err(winrt_err)
            } else {
                Ok(current)
            }
        })?;
        match state {
            StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => {}
            StartupTaskState::DisabledByUser => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Atlas Win+E startup task is disabled in Windows Settings -> Apps -> Startup",
                ));
            }
            StartupTaskState::DisabledByPolicy => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Atlas Win+E startup task is disabled by Windows policy",
                ));
            }
            _ => {
                return Err(io::Error::other(
                    "Could not enable the Atlas Win+E startup task",
                ));
            }
        }
        if find_helper_window() == 0 {
            if let Err(error) = spawn_helper_process() {
                let _ = disable_startup_task();
                return Err(error);
            }
            if !wait_for_helper_state(true, HELPER_SYNC_TIMEOUT) {
                let _ = disable_startup_task();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Atlas Win+E shortcut helper did not start in time",
                ));
            }
        }
        Ok(())
    } else {
        disable_startup_task()?;
        stop_helper_process();
        Ok(())
    }
}

fn disable_startup_task() -> io::Result<()> {
    run_on_mta(|| {
        let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
            .map_err(winrt_err)?
            .get()
            .map_err(winrt_err)?;
        task.Disable().map_err(winrt_err)
    })
}

/// Pure decision state for the low-level Win+E keyboard hook.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Modifiers {
    win: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookAction {
    PassThrough,
    SwallowOnly,
    SwallowAndLaunch,
}

#[derive(Debug, Default)]
struct WinEStateMachine {
    e_suppressed: bool,
}

impl WinEStateMachine {
    fn on_key(
        &mut self,
        msg: u32,
        vk_code: u32,
        injected: bool,
        modifiers: Modifiers,
    ) -> HookAction {
        if injected || vk_code != VK_E {
            return HookAction::PassThrough;
        }
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if self.e_suppressed {
                    return HookAction::SwallowOnly;
                }
                if modifiers.win && !modifiers.ctrl && !modifiers.alt && !modifiers.shift {
                    self.e_suppressed = true;
                    HookAction::SwallowAndLaunch
                } else {
                    HookAction::PassThrough
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if self.e_suppressed {
                    self.e_suppressed = false;
                    HookAction::SwallowOnly
                } else {
                    HookAction::PassThrough
                }
            }
            _ => HookAction::PassThrough,
        }
    }
}

fn current_modifiers() -> Modifiers {
    unsafe {
        Modifiers {
            win: GetAsyncKeyState(i32::from(VK_LWIN)) < 0
                || GetAsyncKeyState(i32::from(VK_RWIN)) < 0,
            ctrl: GetAsyncKeyState(i32::from(VK_CONTROL)) < 0,
            alt: GetAsyncKeyState(i32::from(VK_MENU)) < 0,
            shift: GetAsyncKeyState(i32::from(VK_SHIFT)) < 0,
        }
    }
}

/// Inject an unassigned keystroke (down + up) while Win is still physically held
/// so releasing Win does not open the Start menu.
fn send_win_mask_key() {
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_WIN_MASK,
                    wScan: 0,
                    dwFlags: 0,
                    time: 0,
                    dwExtraInfo: ATLAS_MASK_EXTRA_INFO,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_WIN_MASK,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: ATLAS_MASK_EXTRA_INFO,
                },
            },
        },
    ];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        );
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 && lparam != 0 {
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let injected =
            (kb.flags & LLKHF_INJECTED) != 0 || kb.dwExtraInfo == ATLAS_MASK_EXTRA_INFO;
        let mut machine = WinEStateMachine {
            e_suppressed: E_SUPPRESSED.load(Ordering::Relaxed),
        };
        let action = machine.on_key(wparam as u32, kb.vkCode, injected, current_modifiers());
        // Persist updated state BEFORE SendInput so synchronous re-entry sees it cleanly.
        E_SUPPRESSED.store(machine.e_suppressed, Ordering::Relaxed);
        match action {
            HookAction::PassThrough => {}
            HookAction::SwallowOnly => return 1,
            HookAction::SwallowAndLaunch => {
                send_win_mask_key();
                let hwnd = HELPER_HWND.load(Ordering::Relaxed);
                if hwnd != 0 {
                    unsafe {
                        PostMessageW(hwnd, WM_LAUNCH_ATLAS, 0, 0);
                    }
                }
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

unsafe extern "system" fn helper_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_LAUNCH_ATLAS => {
            if let Some(tx) = LAUNCH_TX.get() {
                let _ = tx.send(());
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION => {
            if wparam != 0 {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_DESTROY => {
            HELPER_HWND.store(0, Ordering::Relaxed);
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn launch_atlas_window() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    unsafe {
        AllowSetForegroundWindow(ASFW_ANY);
    }
    if let Ok(child) = Command::new(exe).spawn() {
        unsafe {
            AllowSetForegroundWindow(child.id());
        }
    }
}

/// Entry point for `atlas.exe --atlas-win-e-helper`, invoked either by Windows
/// at logon via `windows.startupTask` or by Atlas when the user enables Win+E.
pub(super) fn run_helper() {
    if !super::is_packaged() {
        return;
    }
    // Respect Windows Settings / Task Manager if the user disabled the task.
    if !query_startup_task_state().is_ok_and(is_state_enabled) {
        return;
    }
    let mutex_name = wide(MUTEX_NAME);
    let mutex = unsafe { CreateMutexW(null(), 0, mutex_name.as_ptr()) };
    if mutex == 0 {
        return;
    }
    let _mutex = OwnedHandle(mutex);
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }

    let (tx, rx) = mpsc::channel::<()>();
    let _ = LAUNCH_TX.set(tx);
    let launcher = std::thread::spawn(move || {
        let mut last_launch: Option<Instant> = None;
        while rx.recv().is_ok() {
            // Drain queued duplicates if any arrived together.
            while rx.try_recv().is_ok() {}
            let now = Instant::now();
            if last_launch.is_some_and(|prev| now.duration_since(prev) < LAUNCH_DEBOUNCE) {
                continue;
            }
            last_launch = Some(now);
            launch_atlas_window();
        }
    });

    let hinstance = unsafe { GetModuleHandleW(null()) };
    let class_name = wide(WINDOW_CLASS);
    let mut wnd_class: WNDCLASSEXW = unsafe { zeroed() };
    wnd_class.cbSize = size_of::<WNDCLASSEXW>() as u32;
    wnd_class.lpfnWndProc = Some(helper_wnd_proc);
    wnd_class.hInstance = hinstance;
    wnd_class.lpszClassName = class_name.as_ptr();

    if unsafe { RegisterClassExW(&wnd_class) } == 0 {
        return;
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            0,
            hinstance,
            null(),
        )
    };
    if hwnd == 0 {
        unsafe {
            UnregisterClassW(class_name.as_ptr(), hinstance);
        }
        return;
    }
    HELPER_HWND.store(hwnd, Ordering::Relaxed);

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), hinstance, 0) };
    if hook == 0 {
        unsafe {
            DestroyWindow(hwnd);
            UnregisterClassW(class_name.as_ptr(), hinstance);
        }
        HELPER_HWND.store(0, Ordering::Relaxed);
        return;
    }
    let _hook = OwnedHook(hook);

    let mut msg: MSG = unsafe { zeroed() };
    while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    drop(_hook);
    HELPER_HWND.store(0, Ordering::Relaxed);
    unsafe {
        UnregisterClassW(class_name.as_ptr(), hinstance);
    }
    // Note: LAUNCH_TX lives in OnceLock, so the helper process exits right after return.
    let _ = launcher;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win_only() -> Modifiers {
        Modifiers {
            win: true,
            ..Default::default()
        }
    }

    #[test]
    fn win_e_press_repeat_and_release_launches_once_and_swallows_cleanly() {
        let mut sm = WinEStateMachine::default();
        assert_eq!(
            sm.on_key(WM_KEYDOWN, VK_E, false, win_only()),
            HookAction::SwallowAndLaunch
        );
        // Key auto-repeat while held must be swallowed without relaunching.
        assert_eq!(
            sm.on_key(WM_KEYDOWN, VK_E, false, win_only()),
            HookAction::SwallowOnly
        );
        // Matching E release is swallowed so the focused app sees no stray key-up.
        assert_eq!(
            sm.on_key(WM_KEYUP, VK_E, false, win_only()),
            HookAction::SwallowOnly
        );
        // Pressing E again while still holding Win launches again.
        assert_eq!(
            sm.on_key(WM_KEYDOWN, VK_E, false, win_only()),
            HookAction::SwallowAndLaunch
        );
        // Releasing Win before E still swallows the orphaned E key-up cleanly.
        assert_eq!(
            sm.on_key(WM_KEYUP, VK_E, false, Modifiers::default()),
            HookAction::SwallowOnly
        );
        // Subsequent plain E passes through untouched.
        assert_eq!(
            sm.on_key(WM_KEYDOWN, VK_E, false, Modifiers::default()),
            HookAction::PassThrough
        );
    }

    #[test]
    fn other_modifiers_other_keys_and_injected_events_pass_through() {
        let mut sm = WinEStateMachine::default();
        // Win+Shift+E, Win+Ctrl+E, Win+Alt+E are untouched.
        assert_eq!(
            sm.on_key(
                WM_KEYDOWN,
                VK_E,
                false,
                Modifiers {
                    win: true,
                    shift: true,
                    ..Default::default()
                }
            ),
            HookAction::PassThrough
        );
        assert_eq!(
            sm.on_key(
                WM_KEYDOWN,
                VK_E,
                false,
                Modifiers {
                    win: true,
                    ctrl: true,
                    ..Default::default()
                }
            ),
            HookAction::PassThrough
        );
        assert_eq!(
            sm.on_key(
                WM_KEYDOWN,
                VK_E,
                false,
                Modifiers {
                    win: true,
                    alt: true,
                    ..Default::default()
                }
            ),
            HookAction::PassThrough
        );
        // Other Win shortcuts (e.g. Win+D) and Win key itself pass through.
        assert_eq!(
            sm.on_key(WM_KEYDOWN, b'D' as u32, false, win_only()),
            HookAction::PassThrough
        );
        assert_eq!(
            sm.on_key(WM_KEYDOWN, u32::from(VK_LWIN), false, win_only()),
            HookAction::PassThrough
        );
        // Injected events are never intercepted.
        assert_eq!(
            sm.on_key(WM_KEYDOWN, VK_E, true, win_only()),
            HookAction::PassThrough
        );
        // Plain E key-up when not suppressed passes through.
        assert_eq!(
            sm.on_key(WM_KEYUP, VK_E, false, Modifiers::default()),
            HookAction::PassThrough
        );
    }
}
