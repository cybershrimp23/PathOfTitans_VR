//! Protects Path of Titans' DISPLAY settings (resolution / window mode / framerate) from
//! being polluted by a VR session, WITHOUT touching the rest of the file.
//!
//! PoT uses a custom `[/Script/PathOfTitans.IGameUserSettings]` config class. That same file
//! also stores the Alderon login (`AuthToken` / `RefreshToken` / `TokenType` / `ExpiresIn` /
//! `bRememberLogin`), which PoT ROTS when it auto-logs-in on exit. So we must only snapshot /
//! restore the display keys - restoring the whole file would clobber the rotated login token
//! and force a re-login next time. We capture the display keys to a sidecar before writing the
//! VR values, then put just those keys back when the game exits.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The custom PoT settings class that drives resolution / window mode (and stores login).
const VR_SECTION: &str = "/Script/PathOfTitans.IGameUserSettings";

/// Sidecar holding just the captured display keys; its existence is the "restore pending" marker.
const DISPLAY_SIDECAR: &str = "GameUserSettings.launcher-desktop-display.json";

/// Keys we preserve for the user's desktop settings. Deliberately EXCLUDES auth/token keys.
const DISPLAY_KEYS: &[&str] = &[
    "FullscreenMode",
    "LastConfirmedFullscreenMode",
    "ResolutionSizeX",
    "ResolutionSizeY",
    "LastUserConfirmedResolutionSizeX",
    "LastUserConfirmedResolutionSizeY",
    "DesiredScreenWidth",
    "DesiredScreenHeight",
    "LastUserConfirmedDesiredScreenWidth",
    "LastUserConfirmedDesiredScreenHeight",
    "FrameRateLimit",
];

/// The Windows config dir PoT writes its settings to (keyed by the *project* name,
/// not the exe name): %LOCALAPPDATA%\PathOfTitans\Saved\Config\Windows\
fn config_dir() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        Path::new(&local)
            .join("PathOfTitans")
            .join("Saved")
            .join("Config")
            .join("Windows"),
    )
}

pub fn ini_path() -> Option<PathBuf> {
    Some(config_dir()?.join("GameUserSettings.ini"))
}

fn sidecar_path() -> Option<PathBuf> {
    Some(config_dir()?.join(DISPLAY_SIDECAR))
}

/// Force VR display settings into PoT's `GameUserSettings.ini` just before launch.
/// PoT IGNORES `-ResX/-ResY/-Window` command-line args and applies resolution/window from
/// this ini, so this is the only reliable way to control the VR resolution. Only the display
/// keys are changed; login tokens and everything else are left intact. Call AFTER snapshot.
pub fn apply_vr_resolution(width: u32, height: u32) -> Result<(), String> {
    let Some(ini) = ini_path() else {
        return Ok(());
    };
    let content = fs::read_to_string(&ini).unwrap_or_default();
    let w = width.to_string();
    let h = height.to_string();
    let kvs: Vec<(String, String)> = vec![
        ("FullscreenMode".into(), "2".into()), // windowed (PoT's mapping; the tuned VR value)
        ("LastConfirmedFullscreenMode".into(), "2".into()),
        ("ResolutionSizeX".into(), w.clone()),
        ("ResolutionSizeY".into(), h.clone()),
        ("LastUserConfirmedResolutionSizeX".into(), w.clone()),
        ("LastUserConfirmedResolutionSizeY".into(), h.clone()),
        ("DesiredScreenWidth".into(), w.clone()),
        ("DesiredScreenHeight".into(), h.clone()),
        ("LastUserConfirmedDesiredScreenWidth".into(), w),
        ("LastUserConfirmedDesiredScreenHeight".into(), h),
    ];
    let updated = apply_ini_section_kv(&content, VR_SECTION, &kvs);
    fs::write(&ini, updated)
        .map_err(|e| format!("could not write VR resolution into {}: {e}", ini.display()))?;
    Ok(())
}

/// Capture the current desktop display keys into the sidecar (pending-restore marker).
/// Ok(()) if there is no ini yet (nothing to protect).
pub fn snapshot_desktop() -> Result<(), String> {
    let (Some(ini), Some(sidecar)) = (ini_path(), sidecar_path()) else {
        return Ok(());
    };
    if !ini.is_file() {
        return Ok(());
    }
    let content = fs::read_to_string(&ini).unwrap_or_default();
    let captured = capture_ini_section_kv(&content, VR_SECTION, DISPLAY_KEYS);
    let json = serde_json::to_string_pretty(&captured)
        .map_err(|e| format!("could not serialize captured settings: {e}"))?;
    fs::write(&sidecar, json)
        .map_err(|e| format!("could not write sidecar {}: {e}", sidecar.display()))?;
    Ok(())
}

/// Put the captured display keys back (only those keys), leaving login tokens untouched.
/// Returns true if a restore happened. No-op if the sidecar isn't there.
pub fn restore_desktop() -> Result<bool, String> {
    let (Some(ini), Some(sidecar)) = (ini_path(), sidecar_path()) else {
        return Ok(false);
    };
    if !sidecar.is_file() {
        return Ok(false);
    }
    if !ini.is_file() {
        // Nothing to merge into; drop the stale sidecar.
        fs::remove_file(&sidecar).ok();
        return Ok(false);
    }
    let json = fs::read_to_string(&sidecar).unwrap_or_default();
    let captured: BTreeMap<String, String> = serde_json::from_str(&json)
        .map_err(|e| format!("could not read sidecar {}: {e}", sidecar.display()))?;
    let kvs: Vec<(String, String)> = captured.into_iter().collect();
    let content = fs::read_to_string(&ini).unwrap_or_default();
    let updated = apply_ini_section_kv(&content, VR_SECTION, &kvs);
    fs::write(&ini, updated)
        .map_err(|e| format!("could not restore {}: {e}", ini.display()))?;
    fs::remove_file(&sidecar).ok();
    Ok(true)
}

/// True if a restore is pending (the display sidecar exists).
pub fn restore_pending() -> bool {
    sidecar_path().map(|p| p.is_file()).unwrap_or(false)
}

/// Read the values of `keys` (that exist) within `[section]` of INI `content`.
fn capture_ini_section_kv(content: &str, section: &str, keys: &[&str]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut in_section = false;
    for raw in content.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == format!("[{section}]");
            continue;
        }
        if in_section {
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                if keys.contains(&key) {
                    out.insert(key.to_string(), line[eq + 1..].to_string());
                }
            }
        }
    }
    out
}

/// Replace (or insert) `Key=Value` lines scoped to a single `[Section]`, preserving everything
/// else in that section and in the rest of the file.
fn apply_ini_section_kv(content: &str, section: &str, kvs: &[(String, String)]) -> String {
    let header = format!("[{section}]");
    let mut lines: Vec<String> = content
        .split('\n')
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();

    let (insert_at, sec_end) = match lines.iter().position(|l| l.trim() == header) {
        Some(si) => {
            let rest = si + 1;
            let end = lines[rest..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map(|p| rest + p)
                .unwrap_or(lines.len());
            (rest, end)
        }
        None => {
            lines.insert(0, header);
            lines.insert(1, String::new());
            (1, lines.len())
        }
    };

    let mut remaining: Vec<(String, String)> = kvs.to_vec();
    for line in &mut lines[insert_at..sec_end] {
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if let Some(pos) = remaining.iter().position(|(k, _)| *k == key) {
                *line = format!("{key}={}", remaining[pos].1);
                remaining.remove(pos);
            }
        }
    }
    for (k, v) in remaining.into_iter().rev() {
        lines.insert(insert_at, format!("{k}={v}"));
    }
    let mut out = lines.join("\r\n");
    if !out.ends_with("\r\n") {
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn config_paths_are_well_formed() {
        if std::env::var("LOCALAPPDATA").is_ok() {
            let d = config_dir().unwrap();
            assert!(d.ends_with(Path::new("PathOfTitans").join("Saved").join("Config").join("Windows")));
            assert_eq!(ini_path().unwrap().file_name().unwrap(), "GameUserSettings.ini");
            assert_eq!(sidecar_path().unwrap().file_name().unwrap(), DISPLAY_SIDECAR);
        }
    }

    #[test]
    fn capture_reads_only_display_keys_from_section() {
        let content = "[/Script/PathOfTitans.IGameUserSettings]\nFullscreenMode=0\nResolutionSizeX=3024\nResolutionSizeY=1296\nAuthToken=secret\nbRememberLogin=True\n[/Script/Engine.GameUserSettings]\nbUseDesiredScreenHeight=False\n";
        let got = capture_ini_section_kv(content, "/Script/PathOfTitans.IGameUserSettings", DISPLAY_KEYS);
        assert_eq!(got.get("FullscreenMode").map(String::as_str), Some("0"));
        assert_eq!(got.get("ResolutionSizeX").map(String::as_str), Some("3024"));
        // auth keys must NOT be captured
        assert!(!got.contains_key("AuthToken"));
        assert!(!got.contains_key("bRememberLogin"));
    }

    #[test]
    fn apply_preserves_auth_and_rotates_only_display() {
        let content = ";METADATA=(Diff=true)\n[/Script/PathOfTitans.IGameUserSettings]\nAuthToken=keepme\nRefreshToken=keepme2\nFullscreenMode=0\nResolutionSizeX=3024\n[/Script/Engine.GameUserSettings]\nx=1\n";
        let kvs = vec![
            ("FullscreenMode".to_string(), "2".to_string()),
            ("ResolutionSizeX".to_string(), "1280".to_string()),
        ];
        let out = apply_ini_section_kv(content, "/Script/PathOfTitans.IGameUserSettings", &kvs);
        assert!(out.contains("AuthToken=keepme\r\n"), "auth token must survive: {out}");
        assert!(out.contains("RefreshToken=keepme2\r\n"), "{out}");
        assert!(out.contains("FullscreenMode=2\r\n"), "{out}");
        assert!(out.contains("ResolutionSizeX=1280\r\n"), "{out}");
        assert!(out.contains("x=1"), "{out}");
    }
}
