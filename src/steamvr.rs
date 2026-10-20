use std::path::{Path, PathBuf};
use std::process::Command;

use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub const MANIFEST_FILE: &str = "pottvr.vrmanifest";
pub const HOME_IMAGE: &str = "vr_image.png";
const APP_KEY: &str = "com.cybershrimp23.pot_vr_launcher";

fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

pub fn manifest_path() -> Option<PathBuf> {
    Some(exe_dir()?.join(MANIFEST_FILE))
}

/// The OpenVR path registry: %LOCALAPPDATA%\OpenVR\openvrpaths.vrpath.
#[allow(dead_code)]
fn path_registry() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    Some(local.join("OpenVR").join("openvrpaths.vrpath"))
}

/// SteamVR's app-manifest list lives in <config>\appconfig.json under
/// "manifest_paths". The config dir comes from the path registry's "config" node.
fn app_config_path() -> Result<PathBuf, String> {
    let reg = path_registry().ok_or("LOCALAPPDATA is not available")?;
    if !reg.is_file() {
        return Err(format!(
            "OpenVR path registry not found at {} - launch SteamVR at least once.",
            reg.display()
        ));
    }
    let text = std::fs::read_to_string(&reg).map_err(|e| format!("could not read {reg:?}: {e}"))?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("could not parse {reg:?}: {e}"))?;
    let dirs = doc["config"]
        .as_array()
        .ok_or("path registry has no 'config' node")?;
    for entry in dirs {
        if let Some(p) = entry.as_str().map(PathBuf::from).filter(|p| p.is_dir()) {
            return Ok(p.join("appconfig.json"));
        }
    }
    Err("no usable VR config directory in the OpenVR path registry".to_string())
}

/// Adds (or removes) our manifest from appconfig.json's "manifest_paths" array (idempotent).
fn edit_manifest_paths(add: bool) -> Result<String, String> {
    let cfg = app_config_path()?;
    let mut doc: serde_json::Value = match std::fs::read_to_string(&cfg) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("could not parse {cfg:?}: {e}"))?,
        Err(_) => serde_json::json!({ "manifest_paths": [] }),
    };
    let manifest = manifest_path()
        .ok_or("cannot resolve the launcher's install directory")?
        .to_string_lossy()
        .to_string();

    let obj = doc.as_object_mut().ok_or("app config root is not an object")?;
    let arr = obj
        .entry("manifest_paths")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let arr = arr
        .as_array_mut()
        .ok_or("app config 'manifest_paths' node is not an array")?;

    let present = arr.iter().any(|v| v.as_str() == Some(manifest.as_str()));
    if add {
        if present {
            return Ok("Already registered in SteamVR's app config.".to_string());
        }
        arr.push(serde_json::Value::String(manifest.clone()));
    } else if !present {
        return Ok("Not registered (manifest path not in the app config).".to_string());
    } else {
        arr.retain(|v| v.as_str() != Some(manifest.as_str()));
    }
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    // vrserver only reads the app config at startup - a SteamVR restart applies changes.
    std::fs::write(&cfg, text).map_err(|e| format!("could not write {cfg:?}: {e}"))?;
    if add {
        Ok(format!("Added {manifest} to {}", cfg.display()))
    } else {
        Ok(format!("Removed {manifest} from {}", cfg.display()))
    }
}

fn write_json(manifest: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("exe has no parent dir")?;
    let image = dir.join(HOME_IMAGE);

    let mut app = serde_json::json!({
        "app_key": APP_KEY,
        "launch_type": "binary",
        "binary_path_windows": exe.to_string_lossy(),
        "working_dir": dir.to_string_lossy(),
        "arguments": "--launch",
        "strings": {
            "en_us": {
                "name": "Path of Titans VR",
                "description": "Launch Path of Titans in VR via UEVR (unofficial launcher by cybershrimp23)."
            }
        }
    });
    if image.is_file() {
        app["image_path"] = serde_json::Value::String(image.to_string_lossy().to_string());
    }
    let doc = serde_json::json!({
        "source": "PoT VR Launcher",
        "applications": [app]
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(manifest, text).map_err(|e| format!("could not write {manifest:?}: {e}"))
}

/// SteamVR's vrcmd.exe (found via the path registry's "runtime" node). `vrcmd
/// --appmanifest <path>` registers a manifest with the RUNNING vrserver, so the
/// app shows up in Home without a restart.
fn find_vrcmd() -> Option<PathBuf> {
    let reg = path_registry()?;
    let text = std::fs::read_to_string(&reg).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let runtimes = doc["runtime"].as_array()?;
    for entry in runtimes {
        let dir = entry.as_str().map(PathBuf::from)?;
        for exe in [
            dir.join(r"bin\win64\vrcmd.exe"),
            dir.join(r"bin\win32\vrcmd.exe"),
        ] {
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Self-heal: the first builds wrongly added the manifest to openvrpaths.vrpath's
/// "manifest_paths" node. Remove it so vrserver's app list stays clean.
fn scrub_path_registry() {
    let Some(reg) = path_registry().filter(|p| p.is_file()) else {
        return;
    };
    let Some(manifest) = manifest_path() else { return };
    let manifest = manifest.to_string_lossy().to_string();
    let Ok(text) = std::fs::read_to_string(&reg) else {
        return;
    };
    let Ok(mut doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let changed = match doc.as_object_mut().and_then(|o| o.get_mut("manifest_paths")) {
        Some(serde_json::Value::Array(arr)) => {
            let before = arr.len();
            arr.retain(|v| v.as_str() != Some(manifest.as_str()));
            arr.len() != before
        }
        _ => false,
    };
    if changed {
        if let Ok(out) = serde_json::to_string_pretty(&doc) {
            std::fs::write(&reg, out).ok();
        }
    }
}

pub fn register() -> Result<String, String> {
    let manifest = manifest_path().ok_or("cannot resolve the launcher's install directory")?;
    write_json(&manifest)?;
    scrub_path_registry();
    let msg = edit_manifest_paths(true)?;
    let live = match find_vrcmd() {
        Some(vrcmd) => {
            let out = Command::new(&vrcmd)
                .arg("--appmanifest")
                .arg(&manifest)
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    " Live-registered with the running SteamVR session - 'Path of Titans VR' should appear in Home now."
                        .to_string()
                }
                Ok(o) => format!(
                    " Live registration said: {} (already loaded?) - restart SteamVR to apply.",
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .chain(String::from_utf8_lossy(&o.stderr).lines())
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                Err(e) => format!(" Could not run vrcmd ({e}) - restart SteamVR to apply."),
            }
        }
        None => " SteamVR not found - will register when SteamVR starts next.".to_string(),
    };
    Ok(format!("{msg}.{live}"))
}

pub fn unregister() -> Result<String, String> {
    scrub_path_registry();
    let msg = edit_manifest_paths(false)?;
    let manifest = manifest_path();
    if let Some(m) = manifest.filter(|p| p.is_file()) {
        std::fs::remove_file(&m).ok();
    }
    Ok(format!(
        "{msg} Restart SteamVR to drop the tile from Home if it is still shown."
    ))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn manifest_json_has_required_fields() {
        let dir = std::env::temp_dir().join("POTSteamvrTest");
        std::fs::create_dir_all(&dir).unwrap();
        let mpath = dir.join(MANIFEST_FILE);
        write_json(&mpath).unwrap();
        let text = std::fs::read_to_string(&mpath).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        let app = &doc["applications"][0];
        assert_eq!(app["app_key"].as_str().unwrap(), APP_KEY);
        assert_eq!(app["launch_type"].as_str().unwrap(), "binary");
        assert_eq!(app["arguments"].as_str().unwrap(), "--launch");
        let bin = app["binary_path_windows"].as_str().unwrap();
        assert!(bin.ends_with(".exe"), "single-escaped path: {bin}");
        assert!(!bin.contains("\\\\"), "must not double-escape: {bin}");
        std::fs::remove_file(&mpath).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn path_registry_exists_on_this_machine() {
        match path_registry() {
            Some(p) => assert!(p.is_file(), "expected {p:?}"),
            None => eprintln!("no LOCALAPPDATA - skipping"),
        }
    }
}
