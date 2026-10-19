use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub game_exe: Option<PathBuf>,
    #[serde(default)]
    pub backend_dll: Option<PathBuf>,
    #[serde(default)]
    pub use_openvr: bool,
    #[serde(default = "default_true")]
    pub nullify_vr_plugins: bool,
    /// Wipe the per-game UEVR folder (config/imgui/scripts) before each launch.
    #[serde(default)]
    pub wipe_uevr_config_on_launch: bool,
    /// Snapshot/restore PoT's GameUserSettings.ini so VR resolution doesn't leak into desktop.
    #[serde(default = "default_true")]
    pub protect_desktop_settings: bool,
    /// Seconds to wait after launching the game before injecting the backend.
    #[serde(default = "default_inject_delay_secs")]
    pub inject_delay_secs: u32,
}

fn default_true() -> bool {
    true
}

fn default_inject_delay_secs() -> u32 {
    10
}

impl Default for Config {
    fn default() -> Self {
        Self {
            game_exe: None,
            backend_dll: None,
            // The user's stated preference: OpenVR rather than OpenXR.
            use_openvr: true,
            // Default: always disable other VR plugins.
            nullify_vr_plugins: true,
            // Do NOT wipe the per-game UEVR folder by default (keeps config/imgui/scripts).
            wipe_uevr_config_on_launch: false,
            protect_desktop_settings: true,
            inject_delay_secs: default_inject_delay_secs(),
        }
    }
}

impl Config {
    /// Directory containing the running launcher exe (the install dir when packaged).
    fn exe_dir() -> Option<PathBuf> {
        Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
    }

    /// Ordered UEVR build/output directories to search for a DLL filename:
    /// 1. THIS repo's `PoT_UEVR` submodule build output (preferred),
    /// 2. the running exe's own dir (the installer bundles DLLs there),
    /// 3. a sibling `~/PoT_UEVR` checkout. (No generic `~/UEVR`.)
    fn uevr_base_dirs() -> Vec<PathBuf> {
        let mut v = Vec::new();
        v.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("PoT_UEVR")
                .join("build")
                .join("bin")
                .join("uevr"),
        );
        if let Some(d) = Self::exe_dir() {
            v.push(d);
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            v.push(
                Path::new(&home)
                    .join("PoT_UEVR")
                    .join("build")
                    .join("bin")
                    .join("uevr"),
            );
        }
        v
    }

    /// Concrete candidate paths for `file` across the ordered UEVR dirs.
    fn uevr_candidates(file: &str) -> Vec<PathBuf> {
        Self::uevr_base_dirs().into_iter().map(|d| d.join(file)).collect()
    }

    fn dir() -> PathBuf {
        let dirs = directories::ProjectDirs::from("", "", "PoTVRLauncher");
        dirs.map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn path() -> PathBuf {
        Self::dir().join("config.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        serde_json::to_writer_pretty(std::fs::File::create(p)?, self)
            .map_err(std::io::Error::other)
    }

    /// Candidate UEVR backend DLL locations, install/submodule first.
    pub fn default_backend_candidates() -> Vec<PathBuf> {
        Self::uevr_candidates("UEVRBackend.dll")
    }

    pub fn default_backend_dll() -> Option<PathBuf> {
        Self::default_backend_candidates().into_iter().find(|p| p.is_file())
    }

    pub fn backend_dll_path(&self) -> Option<PathBuf> {
        if let Some(p) = self.backend_dll.as_ref() {
            if p.is_file() {
                return Some(p.clone());
            }
        }
        Self::default_backend_dll()
    }

    /// Backend config key value for the VR runtime: openvr_api.dll vs openxr_loader.dll.
    pub fn requested_runtime(&self) -> &'static str {
        if self.use_openvr {
            "openvr_api.dll"
        } else {
            "openxr_loader.dll"
        }
    }

    /// Candidate locations for the runtime DLL (openvr_api.dll / openxr_loader.dll) in the
    /// UEVR build dirs, newest build first. These must be copied next to the game EXE because
    /// the backend resolves the runtime with `get_executable()`'s directory, not its own.
    pub fn runtime_dll_candidates(&self) -> Vec<PathBuf> {
        Self::uevr_candidates(self.requested_runtime())
    }

    /// The runtime DLL for this build, newest build first; None if none exists.
    pub fn runtime_dll(&self) -> Option<PathBuf> {
        self.runtime_dll_candidates().into_iter().find(|p| p.is_file())
    }

    /// Candidate UEVR plugin-nullifier DLL locations, install/submodule first.
    pub fn default_nullifier_candidates() -> Vec<PathBuf> {
        Self::uevr_candidates("UEVRPluginNullifier.dll")
    }

    pub fn default_nullifier_dll() -> Option<PathBuf> {
        Self::default_nullifier_candidates().into_iter().find(|p| p.is_file())
    }

    pub fn game_valid(&self) -> bool {
        self.game_exe
            .as_ref()
            .map(|p| p.is_file())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn candidates_ordered_submodule_then_installed_then_sibling() {
        let cands = Config::default_backend_candidates();
        assert_eq!(cands.len(), 3, "in-repo submodule + exe-dir + sibling ~/PoT_UEVR");
        // No generic ~/UEVR fallback should appear.
        assert!(
            !cands.iter().any(|p| p.to_string_lossy().contains("\\UEVR\\build")),
            "~/UEVR must not be a candidate: {cands:?}"
        );
        // First candidate is the in-repo submodule (under CARGO_MANIFEST_DIR).
        let in_repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("PoT_UEVR");
        assert!(
            cands[0].starts_with(&in_repo),
            "in-repo submodule should be first, got {:?}", cands[0]
        );
        // Second is the running exe's install dir.
        assert_eq!(cands[1], Config::exe_dir().unwrap().join("UEVRBackend.dll"));
    }

    #[test]
    fn default_backend_falls_back_to_available_path() {
        // With the real build present at ~/UEVR only, the fallback must resolve.
        let dll = Config::default_backend_dll();
        let present = Config::default_backend_candidates().into_iter().any(|p| p.is_file());
        if present {
            assert!(dll.is_some(), "should resolve to an existing candidate");
        } else {
            assert!(dll.is_none(), "should be None when no candidate exists");
        }
    }

    #[test]
    fn backend_dll_path_filters_to_existing_file() {
        // A stored config pointing at a missing DLL must be ignored in favour
        // of a default candidate that does exist.
        let cfg = Config {
            game_exe: None,
            backend_dll: Some(PathBuf::from("C:\\nonexistent\\\\UEVRBackend.dll")),
            use_openvr: false,
            nullify_vr_plugins: false,
            wipe_uevr_config_on_launch: true,
            protect_desktop_settings: true,
            inject_delay_secs: 10,
        };
        let resolved = cfg.backend_dll_path();
        let present = Config::default_backend_candidates().into_iter().any(|p| p.is_file());
        assert_eq!(resolved.is_some(), present);
    }

    #[test]
    fn defaults_inject_delay_to_10s() {
        assert_eq!(Config::default().inject_delay_secs, 10);
    }

    #[test]
    fn defaults_wipe_on_launch_to_false() {
        assert!(!Config::default().wipe_uevr_config_on_launch);
    }

    #[test]
    fn defaults_nullify_vr_plugins_to_true() {
        assert!(Config::default().nullify_vr_plugins);
    }

    #[test]
    fn defaults_protect_desktop_to_true() {
        assert!(Config::default().protect_desktop_settings);
    }

    #[test]
    fn defaults_to_openvr_runtime() {
        let cfg = Config::default();
        assert_eq!(cfg.requested_runtime(), "openvr_api.dll");
    }

    #[test]
    fn requested_runtime_switches_with_checkbox() {
        let openvr = Config {
            use_openvr: true,
            ..Default::default()
        };
        assert_eq!(openvr.requested_runtime(), "openvr_api.dll");

        let openxr = Config {
            use_openvr: false,
            ..Default::default()
        };
        assert_eq!(openxr.requested_runtime(), "openxr_loader.dll");
    }

    #[test]
    fn nullifier_candidates_ordered() {
        let cands = Config::default_nullifier_candidates();
        assert_eq!(cands.len(), 3, "in-repo submodule + exe-dir + sibling ~/PoT_UEVR");
        let in_repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("PoT_UEVR");
        assert!(cands[0].starts_with(&in_repo), "in-repo submodule should lead: {:?}", cands[0]);
    }
}
