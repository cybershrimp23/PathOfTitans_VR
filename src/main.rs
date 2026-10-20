#![cfg_attr(not(test), windows_subsystem = "windows")] // GUI app: no console window when run

mod config;
mod discover;
mod game_settings;
mod inject;
mod steamvr;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use config::Config;

/// VR Notes rendered as CommonMark. Stored as a compiled-in string.
const VR_NOTES_MD: &str = include_str!("../assets/vr_notes.md");

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    Ready = 0,
    Launching = 1,
    Injecting = 2,
    Running = 4,
    Failed = 5,
}

impl State {
    fn from_u8(v: u8) -> State {
        match v {
            1 => State::Launching,
            2 => State::Injecting,
            4 => State::Running,
            5 => State::Failed,
            _ => State::Ready,
        }
    }

    fn label(self) -> &'static str {
        match self {
            State::Ready => "Ready",
            State::Launching => "Launching game…",
            State::Injecting => "Injecting UEVR backend…",
            State::Running => "VR active — put on your headset",
            State::Failed => "Failed — open File > Log",
        }
    }
}

struct Shared {
    log: Mutex<Vec<String>>,
    /// Optional mirror of every log line to this file (used by the headless
    /// --launch mode invoked from SteamVR Home, where there is no GUI to read).
    log_path: Option<PathBuf>,
    state: AtomicU8,
    busy: AtomicBool,
    detected: Mutex<Option<PathBuf>>,
    pid: AtomicU32,
}

impl Shared {
    fn new(log_path: Option<PathBuf>) -> Self {
        Shared {
            log: Mutex::new(Vec::new()),
            log_path,
            state: AtomicU8::new(State::Ready as u8),
            busy: AtomicBool::new(false),
            detected: Mutex::new(None),
            pid: AtomicU32::new(0),
        }
    }

    fn log(&self, msg: &str) {
        let line = format!("[{}] {msg}", timestamp());
        self.log.lock().unwrap().push(line.clone());
        if let Some(p) = &self.log_path {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    fn set_state(&self, s: State) {
        self.state.store(s as u8, Ordering::Relaxed);
    }

    fn done(&self) {
        self.busy.store(false, Ordering::Relaxed);
    }
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{:02}:{:02}:{:02}Z", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

struct LauncherApp {
    config: Config,
    shared: Arc<Shared>,
    show_settings: bool,
    show_log: bool,
    show_notes: bool,
    edit_game: String,
    edit_backend: String,
    md_cache: CommonMarkCache,
}

impl LauncherApp {
    fn new() -> Self {
        let mut config = Config::load();
        if config.game_exe.as_ref().is_none_or(|p| !p.is_file()) {
            config.game_exe = discover::quick_find();
        }
        if config.backend_dll.as_ref().is_none_or(|p| !p.is_file()) {
            config.backend_dll = Config::default_backend_dll().filter(|p| p.is_file());
        }
        let shared = Arc::new(Shared::new(None));
        if config.game_exe.is_none() {
            shared.log("Path of Titans not found automatically - open File > Settings to browse for it.");
        }
        if config.backend_dll.is_none() {
            shared.log("UEVRBackend.dll not found - build it with build.ps1 or set the path in File > Settings.");
        }
        // Auto-heal a pending desktop-settings backup left by a prior session (e.g. the
        // launcher was restarted/killed before the game exited). If the game is still
        // running we can't safely rewrite the ini, so re-arm a watcher to restore on exit.
        if game_settings::restore_pending() {
            match inject::find_running_game() {
                Some(pid) => {
                    shared.log("Found a pending desktop-settings backup from a prior session; will restore when the game exits.");
                    let s = Arc::clone(&shared);
                    if let Err(e) = inject::watch_process_exit(pid, move || {
                        match game_settings::restore_desktop() {
                            Ok(true) => s.log("Restored desktop settings after the game exited (recovered from a prior session)."),
                            Ok(false) => {}
                            Err(e) => s.log(&format!("WARN: could not restore desktop settings: {e}")),
                        }
                    }) {
                        shared.log(&format!("WARN: could not watch the running game to restore settings: {e}"));
                    }
                }
                None => match game_settings::restore_desktop() {
                    Ok(true) => shared.log("Restored desktop display settings from a prior session."),
                    Ok(false) => {}
                    Err(e) => shared.log(&format!("WARN: {e}")),
                },
            }
        }
        let mut app = Self {
            config,
            shared,
            show_settings: false,
            show_log: false,
            show_notes: false,
            edit_game: String::new(),
            edit_backend: String::new(),
            md_cache: CommonMarkCache::default(),
        };
        app.sync_edit_fields();
        app
    }

    fn launch(&self) {
        let Some(exe) = self.config.game_exe.clone().filter(|p| p.is_file()) else {
            self.shared
                .log("Game path is not set or invalid - fix it in File > Settings.");
            return;
        };
        let Some(dll) = self.config.backend_dll_path() else {
            self.shared
                .log("UEVRBackend.dll not found - build it or fix the path in File > Settings.");
            return;
        };
        if self.shared.busy.load(Ordering::Relaxed) {
            return;
        }
        self.shared.busy.store(true, Ordering::Relaxed);
        self.shared.set_state(State::Launching);
        let shared = Arc::clone(&self.shared);
        let use_openvr = self.config.use_openvr;
        let nullify_vr_plugins = self.config.nullify_vr_plugins;
        let inject_delay_secs = self.config.inject_delay_secs;
        let wipe_uevr_config = self.config.wipe_uevr_config_on_launch;
        let protect_desktop = self.config.protect_desktop_settings;
        std::thread::spawn(move || {
            run_launch_flow(shared, exe, dll, use_openvr, nullify_vr_plugins, inject_delay_secs, wipe_uevr_config, protect_desktop, true)
        });
    }

    fn auto_detect(&self) {
        if self.shared.busy.load(Ordering::Relaxed) {
            return;
        }
        self.shared.busy.store(true, Ordering::Relaxed);
        let shared = Arc::clone(&self.shared);
        std::thread::spawn(move || {
            shared.log("Searching fixed drives (known install layouts)…");
            if let Some(found) = discover::quick_find() {
                shared.log(&format!("Found game at {}", found.display()));
                *shared.detected.lock().unwrap() = Some(found);
            } else {
                shared.log("Known layouts missed - running deep drive scan (can take a few minutes)…");
                match discover::deep_find() {
                    Some(found) => {
                        shared.log(&format!("Found game at {}", found.display()));
                        *shared.detected.lock().unwrap() = Some(found);
                    }
                    None => shared.log(
                        "Path of Titans was not found on any fixed drive. Use Browse… in Settings.",
                    ),
                }
            }
            shared.done();
        });
    }

    fn chip(ui: &mut egui::Ui, ok: bool, text: &str) {
        let color = if ok {
            egui::Color32::from_rgb(80, 200, 120)
        } else {
            egui::Color32::from_rgb(230, 90, 90)
        };
        ui.horizontal(|ui| {
            ui.colored_label(color, if ok { "●" } else { "✖" });
            ui.label(text);
        });
    }

    fn take_detected(&mut self) {
        let found = self.shared.detected.lock().unwrap().take();
        if let Some(p) = found {
            self.config.game_exe = Some(p);
            self.config.save().ok();
        }
    }

    /// Re-read the editable path strings from config. Called when the Settings
    /// window first opens so the fields show the latest values.
    fn sync_edit_fields(&mut self) {
        self.edit_game = self
            .config
            .game_exe
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.edit_backend = self
            .config
            .backend_dll
            .clone()
            .or_else(Config::default_backend_dll)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
    }
}

fn run_launch_flow(
    shared: Arc<Shared>,
    exe: PathBuf,
    dll: PathBuf,
    use_openvr: bool,
    nullify_vr_plugins: bool,
    inject_delay_secs: u32,
    wipe_uevr_config: bool,
    protect_desktop: bool,
    watch: bool,
) {
    shared.log(&format!("Game:   {}", exe.display()));
    shared.log(&format!("Backend: {}", dll.display()));

    if wipe_uevr_config {
        match inject::clear_game_uevr_dir(&exe) {
            Ok(()) => shared.log(&format!(
                "Wiped per-game UEVR folder: {}",
                inject::game_uevr_dir(&exe).unwrap_or_default().display()
            )),
            Err(e) => shared.log(&format!("WARN: could not wipe UEVR folder: {e}")),
        }
    }

    let runtime = if use_openvr {
        "OpenVR"
    } else {
        "OpenXR"
    };
    let requested_runtime = Config {
        use_openvr,
        ..Default::default()
    }.requested_runtime();
    match inject::write_runtime_config(&exe, use_openvr) {
        Ok(()) => {
            shared.log(&format!(
                "VR runtime requested: {runtime} (Frontend_RequestedRuntime={} in config).",
                requested_runtime
            ));
        }
        Err(e) => {
            shared.log(&format!("WARN: could not set VR runtime: {e}"));
        }
    }

    if let Err(e) = inject::copy_runtime_dll_to_exe_dir(&exe, use_openvr) {
        shared.log(&format!("WARN: {e}"));
    } else {
        shared.log(&format!(
            "Copied {} next to the game EXE so the backend can find it.",
            requested_runtime
        ));
    }

    if protect_desktop {
        match game_settings::snapshot_desktop() {
            Ok(()) => shared.log("Saved your desktop display settings (will restore when the game exits)."),
            Err(e) => shared.log(&format!("WARN: could not back up desktop settings: {e}")),
        }
    }

    let inject_delay = std::time::Duration::from_secs(inject_delay_secs as u64);
    let pid: u32;
    let mut launched_new = false;

    if let Some(existing) = inject::find_running_game() {
        shared.log(&format!(
            "Game already running (pid {existing}) - injecting without restarting."
        ));
        pid = existing;
    } else {
        shared.set_state(State::Launching);
        // PoT applies resolution from its own ini and ignores -Res CLI, so write the VR
        // values in now (desktop values were already snapshotted above).
        match game_settings::apply_vr_resolution(1280, 1024) {
            Ok(()) => shared.log("Forced VR resolution into PoT's GameUserSettings.ini (1280x1024 windowed)."),
            Err(e) => shared.log(&format!("WARN: could not set VR resolution: {e}")),
        }
        shared.log("Starting Path of Titans…");
        match inject::launch_game(&exe) {
            Ok(p) => {
                pid = p;
                launched_new = true;
            }
            Err(e) => {
                shared.log(&format!("ERROR: {e}"));
                shared.set_state(State::Failed);
                shared.done();
                return;
            }
        }
        shared.log(&format!(
            "Waiting {inject_delay_secs}s for the engine to initialize before injecting…"
        ));
        std::thread::sleep(inject_delay);
    }
    shared.pid.store(pid, Ordering::Relaxed);

    shared.set_state(State::Injecting);
    if nullify_vr_plugins {
        if let Some(nullifier) = Config::default_nullifier_dll() {
            shared.log(&format!("Injecting plugin nullifier ({})…", nullifier.display()));
            if let Err(e) = inject::inject_dll(pid, &nullifier) {
                shared.log(&format!("WARN: could not inject plugin nullifier: {e}"));
            } else {
                shared.log("Plugin nullifier injected (other VR modules will be disabled).");
            }
        } else {
            shared.log(
                "WARN: plugin nullifier not found - cannot disable other VR modules. \
                 It ships alongside UEVRBackend.dll (~/UEVR/build/bin/uevr/).",
            );
        }
    }
    shared.log("Injecting UEVRBackend.dll…");
    if let Err(e) = inject::inject_dll(pid, &dll) {
        shared.log(&format!("ERROR: {e}"));
        shared.set_state(State::Failed);
        shared.done();
        return;
    }

    if launched_new {
        shared.log("Injection call succeeded; game window should already be up.");
    } else {
        shared.log("Injection call succeeded.");
    }

    if inject::module_loaded(pid, inject::BACKEND_DLL_NAME) {
        shared.log("UEVRBackend.dll verified inside the game process.");
    } else {
        shared.log("Module not verified yet - UEVR may still be initializing.");
    }

    if protect_desktop && watch {
        let watcher = Arc::clone(&shared);
        if let Err(e) = inject::watch_process_exit(pid, move || {
            match game_settings::restore_desktop() {
                Ok(true) => watcher.log("Game exited - desktop display settings restored."),
                Ok(false) => {}
                Err(e) => watcher.log(&format!("WARN: could not restore desktop settings: {e}")),
            }
        }) {
            shared.log(&format!("WARN: {e} - desktop settings will NOT auto-restore."));
        }
    }

    shared.log("VR should come up within ~30s. First view is often wrong - see Help > VR Notes.");
    shared.set_state(State::Running);
    shared.done();
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.take_detected();

        let state = State::from_u8(self.shared.state.load(Ordering::Relaxed));
        let busy = self.shared.busy.load(Ordering::Relaxed);

        egui::Panel::top("menubar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Settings…").clicked() {
                        self.show_settings = true;
                        self.sync_edit_fields();
                        ui.close();
                    }
                    if ui.button("Log…").clicked() {
                        self.show_log = true;
                        ui.close();
                    }
                    if ui.button("Relaunch as Administrator").clicked() {
                        if let Err(e) = inject::relaunch_as_admin() {
                            self.shared.log(&format!("Relaunch as admin failed: {e}"));
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("VR Notes").clicked() {
                        self.show_notes = true;
                        ui.close();
                    }
                });
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let color = match state {
                    State::Running => egui::Color32::from_rgb(80, 200, 120),
                    State::Failed => egui::Color32::from_rgb(230, 90, 90),
                    _ => egui::Color32::from_rgb(220, 200, 90),
                };
                ui.colored_label(color, state.label());
                if busy {
                    ui.spinner();
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(12.0);
            ui.heading("Path of Titans VR");
            ui.add_space(8.0);

            let game_ok = self.config.game_valid();
            let dll_ok = self.config.backend_dll_path().is_some();
            Self::chip(
                ui,
                game_ok,
                &format!(
                    "Game: {}",
                    self.config
                        .game_exe
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "NOT FOUND - set in File > Settings".into())
                ),
            );
            Self::chip(
                ui,
                dll_ok,
                &format!(
                    "UEVR backend: {}",
                    self.config
                        .backend_dll_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "NOT FOUND - run build.ps1 or set in File > Settings".into())
                ),
            );

            ui.add_space(24.0);
            let label = if busy { "Working…" } else { "  Launch VR" };
            let icon = egui::Image::new(egui::include_image!("../assets/icon.png"))
                .max_size(egui::vec2(44.0, 44.0));
            let text = egui::RichText::new(label).size(28.0);
            let button = egui::Button::new((icon, text)).min_size(egui::vec2(280.0, 64.0));
            if ui.add_enabled(!busy, button).clicked() {
                self.launch();
            }
            ui.add_space(8.0);
            ui.label("Launches the game windowed at 1280x1024 (VR-tuned) and injects UEVR automatically.");
            ui.label("If the game is already running, Launch VR just injects. Watch progress in File > Log.");
            ui.add_space(10.0);
            ui.colored_label(
                egui::Color32::from_rgb(220, 200, 90),
                egui::RichText::new(
                    "⚠ Use the mod at your own risk: anti-cheat is disabled (limits you to single-player \
                     and mod-allowed servers). Not affiliated with Alderon Games. This launcher collects \
                     no user data — see Help → VR Notes.",
                )
                .size(12.0),
            );
        });

        if self.show_settings {
            self.settings_ui(ui);
        }
        if self.show_log {
            self.log_ui(ui);
        }
        if self.show_notes {
            self.notes_ui(ui);
        }

        if busy {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn on_exit(&mut self) {
        self.config.save().ok();
    }
}

impl LauncherApp {
    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let mut open = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .show(ui, |ui| {
                let mut game = std::mem::take(&mut self.edit_game);
                let mut backend = std::mem::take(&mut self.edit_backend);

                ui.label("Path of Titans executable:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut game).desired_width(360.0));
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Path of Titans", &["exe"])
                            .set_file_name(inject::GAME_EXE_NAME)
                            .pick_file()
                        {
                            game = path.display().to_string();
                        }
                    }
                });
                if ui.button("Auto-detect on drives").clicked() {
                    self.auto_detect();
                }

                ui.add_space(10.0);
                ui.label("UEVRBackend.dll:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut backend).desired_width(360.0));
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("UEVR backend", &["dll"])
                            .pick_file()
                        {
                            backend = path.display().to_string();
                        }
                    }
                });
                if !PathBuf::from(&backend).is_file() {
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 90, 90),
                        "Missing! Run build.ps1 to build the UEVR mod.",
                    );
                }

                ui.add_space(14.0);
                ui.label("VR injection:");
                if ui.checkbox(&mut self.config.use_openvr, "Use OpenVR (not OpenXR)").changed() {
                    self.shared.log(&format!(
                        "OpenVR enabled: {}",
                        if self.config.use_openvr { "yes" } else { "no" }
                    ));
                    self.config.save().ok();
                }
                ui.label("    (OpenVR is preferred for this game; OpenXR is the fallback.)");
                ui.add_space(2.0);
                if ui.checkbox(&mut self.config.nullify_vr_plugins, "Disable other VR modules (plugin nullifier)").changed() {
                    self.shared.log(&format!(
                        "Nullify other VR plugins: {}",
                        if self.config.nullify_vr_plugins { "yes" } else { "no" }
                    ));
                    self.config.save().ok();
                }
                ui.label("    (Injects UEVRPluginNullifier.dll so competing VR runtimes stay disabled.)");

                ui.add_space(14.0);
                if ui.checkbox(&mut self.config.wipe_uevr_config_on_launch, "Wipe UEVR settings folder before each launch").changed() {
                    self.shared.log(&format!(
                        "Wipe UEVR settings on launch: {}",
                        if self.config.wipe_uevr_config_on_launch { "yes" } else { "no" }
                    ));
                    self.config.save().ok();
                }
                ui.label("    (Deletes %APPDATA%\\UnrealVRMod\\<game>\\ - config, imgui.ini and scripts - for a fresh start. Disable to keep your Lua scripts.)");

                ui.add_space(14.0);
                if ui.checkbox(&mut self.config.protect_desktop_settings, "Keep my desktop resolution after VR").changed() {
                    self.shared.log(&format!(
                        "Protect desktop settings: {}",
                        if self.config.protect_desktop_settings { "yes" } else { "no" }
                    ));
                    self.config.save().ok();
                }
                ui.label("    (Saves your resolution/window settings before a VR session and restores them when the game closes, so desktop play keeps your fullscreen setup. Your Alderon login is left untouched.)");

                ui.add_space(14.0);
                ui.label("Inject delay after launch:");
                let resp = ui.add(
                    egui::DragValue::new(&mut self.config.inject_delay_secs)
                        .range(0..=300)
                        .speed(1)
                        .suffix(" s"),
                );
                if resp.changed() {
                    self.config.save().ok();
                }
                ui.label("    (Seconds to wait after launching before injecting the backend. Set to 0 to inject immediately.)");

                ui.add_space(14.0);
                ui.label("SteamVR Home:");
                ui.horizontal(|ui| {
                    if ui.button("Add to SteamVR Home").clicked() {
                        let s = Arc::clone(&self.shared);
                        std::thread::spawn(move || match steamvr::register() {
                            Ok(m) => s.log(&format!("SteamVR Home: {m}")),
                            Err(e) => s.log(&format!("SteamVR Home ERROR: {e}")),
                        });
                    }
                    if ui.button("Remove from SteamVR Home").clicked() {
                        let s = Arc::clone(&self.shared);
                        std::thread::spawn(move || match steamvr::unregister() {
                            Ok(m) => s.log(&format!("SteamVR Home: {m}")),
                            Err(e) => s.log(&format!("SteamVR Home ERROR: {e}")),
                        });
                    }
                });
                ui.label("    (Adds a 'Path of Titans VR' tile to SteamVR Home that starts the game straight into VR. Restart SteamVR if it doesn't appear.)");

                ui.add_space(14.0);
                if ui.button("Save").clicked() {
                    self.config.game_exe = Some(PathBuf::from(&game));
                    self.config.backend_dll = Some(PathBuf::from(&backend));
                    self.config.save().ok();
                    self.shared.log("Settings saved.");
                    self.show_settings = false;
                }

                self.edit_game = game;
                self.edit_backend = backend;
            });
        self.show_settings = open;
    }

    fn log_ui(&mut self, ui: &mut egui::Ui) {
        let mut open = self.show_log;
        egui::Window::new("Log")
            .open(&mut open)
            .default_size((640.0, 320.0))
            .show(ui, |ui| {
                let lines = self.shared.log.lock().unwrap();
                let text = lines.join("\n");
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut text.as_str())
                                .font(egui::TextStyle::Monospace)
                                .interactive(false),
                        );
                    });
            });
        self.show_log = open;
    }

    fn notes_ui(&mut self, ui: &mut egui::Ui) {
        let mut open = self.show_notes;
        egui::Window::new("VR Notes")
            .open(&mut open)
            .default_size((560.0, 420.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        CommonMarkViewer::new().show(ui, &mut self.md_cache, VR_NOTES_MD);
                    });
            });
        self.show_notes = open;
    }
}

/// Headless launch used by the SteamVR Home tile (`--launch`).
/// Runs the normal launch+inject flow, mirrors the log to launch.log next to
/// the config, then blocks until the game exits so SteamVR Home keeps the app
/// session attached for the whole play session.
fn headless_launch() -> i32 {
    let mut config = Config::load();
    if config.game_exe.as_ref().is_none_or(|p| !p.is_file()) {
        config.game_exe = discover::quick_find();
    }
    if config.backend_dll.as_ref().is_none_or(|p| !p.is_file()) {
        config.backend_dll = Config::default_backend_dll().filter(|p| p.is_file());
    }
    let shared = Arc::new(Shared::new(Some(Config::config_dir().join("launch.log"))));
    shared.log("Launched from the command line (--launch, e.g. SteamVR Home).");

    let Some(exe) = config.game_exe.clone().filter(|p| p.is_file()) else {
        shared.log("ERROR: game path is not set or invalid - run the launcher GUI once and configure it.");
        return 1;
    };
    let Some(dll) = config.backend_dll_path() else {
        shared.log("ERROR: UEVRBackend.dll not found - run build.ps1 or configure the path in the GUI.");
        return 1;
    };

    if game_settings::restore_pending() && inject::find_running_game().is_none() {
        match game_settings::restore_desktop() {
            Ok(true) => shared.log("Restored desktop display settings from a prior session."),
            Ok(false) => {}
            Err(e) => shared.log(&format!("WARN: {e}")),
        }
    }

    run_launch_flow(
        Arc::clone(&shared),
        exe,
        dll,
        config.use_openvr,
        config.nullify_vr_plugins,
        config.inject_delay_secs,
        config.wipe_uevr_config_on_launch,
        config.protect_desktop_settings,
        false,
    );

    if State::from_u8(shared.state.load(Ordering::Relaxed)) == State::Failed {
        return 1;
    }
    let pid = shared.pid.load(Ordering::Relaxed);
    shared.log(&format!(
        "Keeping the launcher alive until the game (pid {pid}) exits, so SteamVR Home stays attached."
    ));
    inject::wait_for_exit(pid);
    if config.protect_desktop_settings {
        match game_settings::restore_desktop() {
            Ok(true) => shared.log("Game exited - desktop display settings restored."),
            Ok(false) => {}
            Err(e) => shared.log(&format!("WARN: could not restore desktop settings: {e}")),
        }
    }
    shared.log("Session complete.");
    0
}

fn headless_steamvr_home(register: bool) -> i32 {
    let shared = Shared::new(Some(Config::config_dir().join("launch.log")));
    let result = if register {
        steamvr::register()
    } else {
        steamvr::unregister()
    };
    match result {
        Ok(msg) => {
            shared.log(&format!("SteamVR Home: {msg}"));
            0
        }
        Err(e) => {
            shared.log(&format!("SteamVR Home ERROR: {e}"));
            1
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a.eq_ignore_ascii_case(flag));
    if has("--launch") {
        std::process::exit(headless_launch());
    }
    if has("--register") {
        std::process::exit(headless_steamvr_home(true));
    }
    if has("--unregister") {
        std::process::exit(headless_steamvr_home(false));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 420.0])
            .with_min_inner_size([520.0, 360.0])
            .with_title("Path of Titans VR Launcher"),
        ..Default::default()
    };
    eframe::run_native(
        "Path of Titans VR Launcher",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(LauncherApp::new()))
        }),
    )
}
