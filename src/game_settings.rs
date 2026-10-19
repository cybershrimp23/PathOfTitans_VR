//! Protects Path of Titans' own `GameUserSettings.ini` (resolution / window mode /
//! framerate) from being polluted by a VR session.
//!
//! PoT uses a custom `[/Script/PathOfTitans.IGameUserSettings]` config class and writes
//! the active resolution + window mode back to this file when the game exits. Because VR
//! forces a different resolution, that on-exit save leaks the VR settings into desktop
//! play. We snapshot the file before launching VR and restore it once the game exits.

use std::fs;
use std::path::{Path, PathBuf};

const BACKUP_NAME: &str = "GameUserSettings.launcher-desktop.bak";

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

fn backup_path() -> Option<PathBuf> {
    Some(config_dir()?.join(BACKUP_NAME))
}

/// Copy the current (desktop) settings to a sidecar backup. The backup's existence is the
/// "pending restore" marker. Ok(()) if there is no ini yet (nothing to protect).
pub fn snapshot_desktop() -> Result<(), String> {
    let (Some(ini), Some(bak)) = (ini_path(), backup_path()) else {
        return Ok(());
    };
    if !ini.is_file() {
        return Ok(());
    }
    fs::copy(&ini, &bak)
        .map_err(|e| format!("could not back up {} -> {}: {e}", ini.display(), bak.display()))?;
    Ok(())
}

/// Restore the backup over the ini if a backup (pending marker) exists. Returns true if a
/// restore happened. Leaves the game running on desktop settings, undoing the VR pollution.
pub fn restore_desktop() -> Result<bool, String> {
    let (Some(ini), Some(bak)) = (ini_path(), backup_path()) else {
        return Ok(false);
    };
    if !bak.is_file() {
        return Ok(false);
    }
    fs::copy(&bak, &ini).map_err(|e| {
        format!("could not restore {} -> {}: {e}", bak.display(), ini.display())
    })?;
    fs::remove_file(&bak)
        .map_err(|e| format!("could not remove backup {}: {e}", bak.display()))?;
    Ok(true)
}

/// The custom PoT settings class that actually drives resolution / window mode.
const VR_SECTION: &str = "/Script/PathOfTitans.IGameUserSettings";

/// Force VR display settings into PoT's `GameUserSettings.ini` just before launch.
/// PoT IGNORES `-ResX/-ResY/-Window` command-line args and applies resolution/window from
/// this ini's `[/Script/PathOfTitans.IGameUserSettings]` section, so this is the only reliable
/// way to control the VR resolution. Call AFTER snapshot_desktop() so the restore reverts it.
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

/// Replace (or insert) `Key=Value` lines scoped to a single `[Section]` in INI `content`.
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

/// True if a restore is pending (a backup exists).
pub fn restore_pending() -> bool {
    backup_path().map(|b| b.is_file()).unwrap_or(false)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn config_paths_are_well_formed() {
        // Paths should point at the PoT project settings dir without touching disk.
        let dir = config_dir();
        if std::env::var("LOCALAPPDATA").is_ok() {
            let d = dir.unwrap();
            assert!(d.ends_with(Path::new("PathOfTitans").join("Saved").join("Config").join("Windows")));
            assert_eq!(ini_path().unwrap().file_name().unwrap(), "GameUserSettings.ini");
            assert_eq!(backup_path().unwrap().file_name().unwrap(), BACKUP_NAME);
        }
    }

    #[test]
    fn apply_ini_section_kv_replaces_existing_and_preserves_others() {
        let content = ";METADATA=(Diff=true)\n[/Script/PathOfTitans.IGameUserSettings]\nFullscreenMode=0\nResolutionSizeX=3024\nResolutionSizeY=1296\nbChatHUD=True\n[/Script/Engine.GameUserSettings]\nbUseDesiredScreenHeight=False\n";
        let kvs = vec![
            ("FullscreenMode".to_string(), "2".to_string()),
            ("ResolutionSizeX".to_string(), "1280".to_string()),
            ("ResolutionSizeY".to_string(), "1024".to_string()),
            ("DesiredScreenWidth".to_string(), "1280".to_string()),
        ];
        let out = apply_ini_section_kv(content, "/Script/PathOfTitans.IGameUserSettings", &kvs);
        assert!(out.contains("FullscreenMode=2\r\n"), "{out}");
        assert!(out.contains("ResolutionSizeX=1280\r\n"), "{out}");
        assert!(out.contains("ResolutionSizeY=1024\r\n"), "{out}");
        // new key inserted inside the target section, not the engine one
        assert!(out.contains("DesiredScreenWidth=1280\r\n"), "{out}");
        // untouched keys preserved
        assert!(out.contains("bChatHUD=True"), "{out}");
        assert!(out.contains("ResolutionSizeY=1296") == false, "old 1296 should be gone");
    }

    #[test]
    fn restore_is_noop_without_backup() {
        // If no backup exists for the real game, restore simply reports false (no error).
        if !restore_pending() {
            assert_eq!(restore_desktop().unwrap(), false);
        }
    }
}
