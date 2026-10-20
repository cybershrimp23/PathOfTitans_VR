use std::ffi::c_void;
use std::path::{Path, PathBuf};

use windows::core::{s, w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, HANDLE, HMODULE, HWND, LPARAM,
};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows::Win32::System::ProcessStatus::{K32EnumProcessModules, K32GetModuleFileNameExW};
use windows::Win32::System::Threading::{
    CreateProcessW, CreateRemoteThread, GetExitCodeThread, OpenProcess, ResumeThread,
    WaitForSingleObject, CREATE_SUSPENDED, INFINITE, PROCESS_CREATE_THREAD,
    PROCESS_INFORMATION, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
    PROCESS_VM_WRITE, STARTUPINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::config::Config;
use crate::discover::wide;

pub const GAME_EXE_NAME: &str = "PathOfTitans-Win64-Shipping.exe";
pub const BACKEND_DLL_NAME: &str = "UEVRBackend.dll";

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn find_running_game() -> Option<u32> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut result = None;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name = utf16_to_string(&entry.szExeFile);
                if name.eq_ignore_ascii_case(GAME_EXE_NAME) {
                    result = Some(entry.th32ProcessID);
                    break;
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        CloseHandle(snap).ok();
        result
    }
}

/// Launches the game windowed at the VR-tuned resolution, suspended, so the
/// backend can be injected before the engine initializes.
#[allow(dead_code)]
pub fn launch_game_suspended(exe: &Path) -> Result<PROCESS_INFORMATION, String> {
    unsafe {
        let cmdline = format!("\"{}\" -Windowed -Res=1280x1024\0", exe.display());
        let mut wide: Vec<u16> = cmdline.encode_utf16().collect();
        let mut pi = PROCESS_INFORMATION::default();
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(wide.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED,
            None,
            PCWSTR::null(),
            &si,
            &mut pi,
        )
        .map_err(|e| format!("Failed to start the game: {e}"))?;
        Ok(pi)
    }
}

#[allow(dead_code)]
pub fn resume_main_thread(pi: &PROCESS_INFORMATION) {
    unsafe {
        ResumeThread(pi.hThread);
        CloseHandle(pi.hThread).ok();
    }
}

/// Launches the game windowed at the VR-tuned resolution and lets it run normally.
/// Returns the pid. Use `watch_process_exit` to be notified when it closes.
pub fn launch_game(exe: &Path) -> Result<u32, String> {
    unsafe {
        let cmdline = format!("\"{}\" -dx12 -BranchKey=\"production\"\0", exe.display());
        let mut wide: Vec<u16> = cmdline.encode_utf16().collect();
        let mut pi = PROCESS_INFORMATION::default();
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(wide.as_mut_ptr())),
            None,
            None,
            false,
            Default::default(),
            None,
            PCWSTR::null(),
            &si,
            &mut pi,
        )
        .map_err(|e| format!("Failed to start the game: {e}"))?;
        let pid = pi.dwProcessId;
        CloseHandle(pi.hThread).ok();
        CloseHandle(pi.hProcess).ok();
        Ok(pid)
    }
}

/// Opens a waitable handle to an existing process. Caller must CloseHandle it.
fn open_waitable_handle(pid: u32) -> Option<HANDLE> {
    use windows::Win32::System::Threading::{
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok() }
}

/// Blocks the calling thread until the given pid exits. If the process cannot be
/// opened for a waitable handle (elevated game, or already gone), falls back to
/// polling the process list.
pub fn wait_for_exit(pid: u32) {
    if pid == 0 {
        return;
    }
    match open_waitable_handle(pid) {
        Some(h) => unsafe {
            WaitForSingleObject(h, INFINITE);
            CloseHandle(h).ok();
        },
        None => loop {
            match find_running_game() {
                Some(p) if p == pid => std::thread::sleep(std::time::Duration::from_millis(500)),
                _ => break,
            }
        },
    }
}

/// Spawns a background thread that blocks until the given pid exits, then runs `on_exit`.
/// Returns Err if the process cannot be opened to watch.
pub fn watch_process_exit(
    pid: u32,
    on_exit: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let handle = open_waitable_handle(pid)
        .ok_or_else(|| "could not open the game process to watch for exit".to_string())?;
    // HANDLE is a raw-pointer wrapper (not Send). Carry it across as an isize owned solely
    // by this worker thread, which waits on it then closes it - sound by construction.
    let raw = handle.0 as isize;
    std::thread::spawn(move || {
        let h = HANDLE(raw as *mut core::ffi::c_void);
        unsafe {
            WaitForSingleObject(h, INFINITE);
            CloseHandle(h).ok();
        }
        on_exit();
    });
    Ok(())
}

#[allow(dead_code)]
struct WindowScan {
    pid: u32,
    found: bool,
}

#[allow(dead_code)]
unsafe extern "system" fn window_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let scan = &mut *(lparam.0 as *mut WindowScan);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == scan.pid && IsWindowVisible(hwnd).as_bool() {
            scan.found = true;
            return BOOL(0);
        }
        BOOL(1)
    }
}

#[allow(dead_code)]
pub fn game_window_exists(pid: u32) -> bool {
    let mut scan = WindowScan { pid, found: false };
    unsafe {
        EnumWindows(
            Some(window_cb),
            LPARAM(&mut scan as *mut WindowScan as isize),
        )
        .ok();
    }
    scan.found
}

#[allow(dead_code)]
pub fn wait_for_window(pid: u32, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if game_window_exists(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}

pub fn inject_dll(pid: u32, dll: &Path) -> Result<(), String> {
    unsafe {
        let access = PROCESS_CREATE_THREAD
            | PROCESS_QUERY_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_WRITE
            | PROCESS_VM_READ;
        let proc = match OpenProcess(access, false, pid) {
            Ok(h) => h,
            Err(_) => {
                if GetLastError() == ERROR_ACCESS_DENIED {
                    return Err("Access denied opening the game process — it is running at a higher privilege level than the launcher. Use File → Relaunch as Administrator.".to_string());
                }
                return Err(format!(
                    "OpenProcess failed for pid {pid} (Win32 error {:?})",
                    GetLastError()
                ));
            }
        };

        let dll_wide = wide(&dll.to_string_lossy());
        let len = dll_wide.len() * 2;
        let mut err = String::new();
        let remote = VirtualAllocEx(proc, None, len, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if remote.is_null() {
            err = format!("VirtualAllocEx failed (Win32 error {:?})", GetLastError());
        } else if WriteProcessMemory(proc, remote, dll_wide.as_ptr() as *const c_void, len, None)
            .is_err()
        {
            err = format!("WriteProcessMemory failed (Win32 error {:?})", GetLastError());
        } else {
            match GetModuleHandleW(w!("kernel32.dll"))
                .map_err(|e| format!("GetModuleHandleW(kernel32.dll): {e}"))
            {
                Ok(kernel32) => {
                    let Some(addr) = GetProcAddress(kernel32, s!("LoadLibraryW")) else {
                        err = "GetProcAddress(LoadLibraryW) returned null".to_string();
                        VirtualFreeEx(proc, remote, 0, MEM_RELEASE).ok();
                        CloseHandle(proc).ok();
                        return Err(err);
                    };
                    let start: unsafe extern "system" fn(*mut c_void) -> u32 =
                        std::mem::transmute_copy(&addr);
                    match CreateRemoteThread(proc, None, 0, Some(start), Some(remote), 0, None) {
                        Ok(thread) => {
                            WaitForSingleObject(thread, INFINITE);
                            let mut exit_code = 0u32;
                            GetExitCodeThread(thread, &mut exit_code).ok();
                            CloseHandle(thread).ok();
                            if exit_code == 0 {
                                err = "LoadLibraryW returned NULL inside the game — UEVRBackend.dll \
                                       could not be loaded (locked file, missing dependency, or typo in path)."
                                    .to_string();
                            }
                        }
                        Err(e) => err = format!("CreateRemoteThread failed: {e}"),
                    }
                }
                Err(e) => err = e,
            }
            VirtualFreeEx(proc, remote, 0, MEM_RELEASE).ok();
        }
        CloseHandle(proc).ok();
        if err.is_empty() {
            Ok(())
        } else {
            Err(err)
        }
    }
}

/// Returns the per-game UEVR persistent dir, mirroring Framework::get_persistent_dir():
/// %APPDATA%\UnrealVRMod\<game-exe-stem>.
pub fn game_uevr_dir(exe: &Path) -> Result<PathBuf, String> {
    let gamename = exe
        .file_stem()
        .ok_or_else(|| "game exe has no file stem".to_string())?
        .to_string_lossy()
        .to_string();
    let appdata = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    Ok(appdata.join("UnrealVRMod").join(gamename))
}

/// Deletes the entire per-game UEVR folder (config.txt, imgui.ini, scripts/, ...) so every
/// launch starts from a clean slate. Returns Ok even if the folder does not exist.
pub fn clear_game_uevr_dir(exe: &Path) -> Result<(), String> {
    let dir = game_uevr_dir(exe)?;
    match dir.try_exists() {
        Ok(true) => std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("could not delete {}: {e}", dir.display()))?,
        Ok(false) => {}
        Err(e) => return Err(format!("could not stat {}: {e}", dir.display())),
    }
    Ok(())
}

/// Writes the `Frontend_RequestedRuntime` key into the game's UEVR config.txt so the
/// backend uses OpenVR (openvr_api.dll) or OpenXR (openxr_loader.dll). Run before the
/// backend initializes (i.e. before injection) for it to take effect.
pub fn write_runtime_config(exe: &Path, use_openvr: bool) -> Result<(), String> {
    let gamename = exe
        .file_stem()
        .ok_or_else(|| "game exe has no file stem".to_string())?
        .to_string_lossy()
        .to_string();

    let appdata = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let cfg_path = appdata.join("UnrealVRMod").join(&gamename).join("config.txt");

    if let Some(parent) = cfg_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create config dir {:?}: {e}", cfg_path.parent()))?;
    }

    let existing = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let kept: Vec<String> = existing
        .lines()
        .filter(|l| l.split('=').next().unwrap_or("").trim() != "Frontend_RequestedRuntime")
        .map(|l| format!("{l}\n"))
        .collect();

    let value = if use_openvr { "openvr_api.dll" } else { "openxr_loader.dll" };
    let mut content = format!("Frontend_RequestedRuntime={value}\n");
    for line in kept {
        content.push_str(&line);
    }

    std::fs::write(&cfg_path, content).map_err(|e| format!("failed to write {cfg_path:?}: {e}"))?;
    Ok(())
}

/// Copies the selected VR runtime DLL (openvr_api.dll / openxr_loader.dll) from the UEVR build
/// dir next to the game EXE. The backend resolves the runtime relative to the game EXE's
/// directory, so the DLL has to physically live there (the injector itself does not copy it).
pub fn copy_runtime_dll_to_exe_dir(exe: &Path, use_openvr: bool) -> Result<(), String> {
    let cfg = Config {
        use_openvr,
        ..Default::default()
    };
    let dll = cfg.runtime_dll().ok_or_else(|| {
        format!(
            "Could not find the runtime DLL ({}) - build it with build.ps1 or set the path in \
             File > Settings.",
            cfg.requested_runtime()
        )
    })?;

    let dest = exe
        .parent()
        .ok_or_else(|| "game exe has no parent directory".to_string())?
        .join(dll.file_name().ok_or_else(|| "runtime DLL has no file name".to_string())?);

    std::fs::copy(&dll, &dest).map_err(|e| {
        format!("could not copy {} -> {}: {e}", dll.display(), dest.display())
    })?;
    Ok(())
}

pub fn module_loaded(pid: u32, module_name: &str) -> bool {
    unsafe {
        let proc = match OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let mut modules = [HMODULE::default(); 1024];
        let mut needed = 0u32;
        let mut found = false;
        if K32EnumProcessModules(
            proc,
            modules.as_mut_ptr(),
            (modules.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut needed,
        )
        .as_bool()
        {
            let count = (needed as usize / std::mem::size_of::<HMODULE>()).min(modules.len());
            let mut buf = [0u16; 4096];
            for m in &modules[..count] {
                let n = K32GetModuleFileNameExW(Some(proc), Some(*m), &mut buf);
                if n == 0 {
                    continue;
                }
                let path = utf16_to_string(&buf[..n as usize]);
                if path
                    .rsplit(['\\', '/'])
                    .next()
                    .is_some_and(|f| f.eq_ignore_ascii_case(module_name))
                {
                    found = true;
                    break;
                }
            }
        }
        CloseHandle(proc).ok();
        found
    }
}

pub fn relaunch_as_admin() -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_w = wide(&exe.to_string_lossy());
    let op_w = wide("runas");
    unsafe {
        let r = ShellExecuteW(
            None,
            PCWSTR(op_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if (r.0 as isize) <= 32 {
            return Err(format!("ShellExecuteW(runas) failed: {:?}", r.0));
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn write_runtime_config_sets_openvr_key_first() {
        use std::fs;
        let gamename = "POTWriteRuntimeCfgTest";
        let exe = PathBuf::from(gamename);
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        let cfg = std::path::Path::new(&appdata)
            .join("UnrealVRMod")
            .join(gamename)
            .join("config.txt");
        fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        fs::write(&cfg, "FrameworkConfig_LogLevel=2\nSomeOtherKey=1\n").unwrap();

        write_runtime_config(&exe, true).expect("should write config");
        let text = fs::read_to_string(&cfg).unwrap();
        assert!(
            text.starts_with("Frontend_RequestedRuntime=openvr_api.dll\n"),
            "openvr key must be first, was:\n{text}"
        );
        assert!(text.contains("FrameworkConfig_LogLevel=2"));

        write_runtime_config(&exe, false).expect("should overwrite key");
        let text = fs::read_to_string(&cfg).unwrap();
        assert!(text.starts_with("Frontend_RequestedRuntime=openxr_loader.dll\n"));

        fs::remove_file(&cfg).ok();
        let _ = fs::remove_dir_all(cfg.parent().unwrap());
    }

    #[test]
    fn copy_runtime_dll_to_exe_dir_places_dll_next_to_exe() {
        use std::fs;
        let runtime = match Config::default().runtime_dll() {
            Some(d) => d,
            None => {
                eprintln!("runtime DLL not found - skipping copy test");
                return;
            }
        };
        let exe = std::env::temp_dir()
            .join("POTCopyRuntimeTest")
            .join("PathOfTitans-Win64-Shipping.exe");
        if !exe.is_file() {
            fs::create_dir_all(exe.parent().unwrap()).unwrap();
            fs::write(&exe, b"fake exe").unwrap();
        }
        copy_runtime_dll_to_exe_dir(&exe, true).expect("should copy runtime DLL");
        let expected = exe.parent().unwrap().join(runtime.file_name().unwrap());
        assert!(expected.is_file(), "runtime DLL should be copied next to the exe");
        fs::remove_file(&exe).ok();
        fs::remove_file(&expected).ok();
        fs::remove_dir_all(exe.parent().unwrap()).ok();
    }

    #[test]
    fn inject_into_running_process() {
        let dll = std::env::temp_dir().join("dummy.dll");
        if !dll.is_file() {
            eprintln!("dummy.dll not present - build it first");
            return;
        }
        let mut child = std::process::Command::new("notepad.exe")
            .spawn()
            .expect("failed to start notepad");
        let pid = child.id();
        if !wait_for_window(pid, std::time::Duration::from_secs(5)) {
            child.kill().ok();
            eprintln!("notepad window did not appear in time");
            return;
        }
        match inject_dll(pid, &dll) {
            Ok(()) => {
                assert!(module_loaded(pid, "dummy.dll"), "dummy.dll should be loaded");
            }
            Err(e) => {
                child.kill().ok();
                panic!("inject_dll failed: {e}");
            }
        }
        child.kill().ok();
    }

    #[test]
    fn inject_real_backend_into_running_game() {
        use crate::config::Config;
        let dll = match std::env::var("POT_LAUNCHER_BACKEND_DLL") {
            Ok(p) => PathBuf::from(p),
            Err(_) => Config::default_backend_dll()
                .unwrap_or_else(|| std::env::temp_dir().join("UEVRBackend.dll")),
        };
        let pid = match find_running_game() {
            Some(p) => p,
            None => {
                eprintln!("game not running - launching it now");
                let exe = "C:\\ProgramData\\Alderon Games\\Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64\\PathOfTitans-Win64-Shipping.exe";
                let mut child = std::process::Command::new(exe)
                    .args(["-Windowed", "-Res=1280x1024"])
                    .spawn()
                    .expect("failed to launch game");
                let p = child.id();
                if !wait_for_window(p, std::time::Duration::from_secs(20)) {
                    child.kill().ok();
                    eprintln!("game did not come up");
                    return;
                }
                p
            }
        };
        eprintln!("Injecting real backend {dll:?} into game pid {pid}");
        match inject_dll(pid, &dll) {
            Ok(()) => {
                assert!(
                    module_loaded(pid, BACKEND_DLL_NAME),
                    "UEVRBackend.dll should be loaded into the game"
                );
            }
            Err(e) => panic!("inject of real DLL failed: {e}"),
        }
    }

    #[test]
    fn full_game_launch_suspend_real_backend_resume() {
        use crate::config::Config;
        let dll = match std::env::var("POT_LAUNCHER_BACKEND_DLL") {
            Ok(p) => PathBuf::from(p),
            Err(_) => Config::default_backend_dll()
                .unwrap_or_else(|| std::env::temp_dir().join("UEVRBackend.dll")),
        };
        if !dll.is_file() {
            eprintln!("backend {dll:?} not found - skipping end-to-end test");
            return;
        }
        let exe = "C:\\ProgramData\\Alderon Games\\Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64\\PathOfTitans-Win64-Shipping.exe";
        let mut child = match std::process::Command::new(exe)
            .args(["-Windowed", "-Res=1280x1024"])
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cannot launch game ({e}) - skipping end-to-end test");
                return;
            }
        };
        let pid = child.id();
        eprintln!("Launched game pid {pid}, injecting real backend {dll:?}");
        match inject_dll(pid, &dll) {
            Ok(()) => {
                assert!(
                    module_loaded(pid, BACKEND_DLL_NAME),
                    "UEVRBackend.dll should be loaded into the real game"
                );
            }
            Err(e) => {
                child.kill().ok();
                panic!("end-to-end inject failed: {e}");
            }
        }
        child.kill().ok();
    }

    #[test]
    fn full_launch_suspend_inject_resume_pipeline() {
        let dll = std::env::temp_dir().join("dummy.dll");
        if !dll.is_file() {
            eprintln!("dummy.dll not present - build it first");
            return;
        }
        // launch suspended like the launcher does
        let cmdline = format!("\"{}\" -Windowed -Res=1280x1024\0", "C:\\Windows\\System32\\notepad.exe");
        let mut wide: Vec<u16> = cmdline.encode_utf16().collect();
        wide.push(0);
        let pi = unsafe {
            let mut pi = PROCESS_INFORMATION::default();
            let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
            CreateProcessW(PCWSTR::null(), Some(PWSTR(wide.as_mut_ptr())), None, None, false, CREATE_SUSPENDED, None, PCWSTR::null(), &si, &mut pi).expect("createprocess");
            pi
        };
        let pid = pi.dwProcessId;
        // inject while suspended
        assert!(inject_dll(pid, &dll).is_ok(), "inject while suspended should succeed");
        assert!(module_loaded(pid, "dummy.dll"), "module should be loaded");
        // resume so the injected module's DllMain runs before the process proceeds
        resume_main_thread(&pi);
    }
}
