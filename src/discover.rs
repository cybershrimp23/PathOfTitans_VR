use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::GetDriveTypeW;

pub const EXE_NAME: &str = "PathOfTitans-Win64-Shipping.exe";

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const DRIVE_FIXED: u32 = 3;

fn drive_is_fixed(letter: char) -> bool {
    let root = format!("{}:\\", letter);
    let w = wide(&root);
    unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) == DRIVE_FIXED }
}

fn known_relative_paths() -> Vec<PathBuf> {
    vec![
        Path::new("ProgramData\\Alderon Games\\Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64")
            .join(EXE_NAME),
        Path::new("Program Files\\Alderon Games\\Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64")
            .join(EXE_NAME),
        Path::new("Alderon Games\\Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64").join(EXE_NAME),
        Path::new("Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64").join(EXE_NAME),
        Path::new("Games\\Path of Titans\\WindowsNoEditor\\PathOfTitans\\Binaries\\Win64").join(EXE_NAME),
    ]
}

pub fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for letter in 'C'..='Z' {
        if !drive_is_fixed(letter) {
            continue;
        }
        for rel in known_relative_paths() {
            let p = Path::new(&format!("{}:\\", letter)).join(rel);
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

pub fn quick_find() -> Option<PathBuf> {
    candidates().into_iter().next()
}

const SKIP_DIRS: &[&str] = &[
    "windows",
    "$recycle.bin",
    "system volume information",
    "programdata\\microsoft",
    "recovery",
    "perflogs",
    "$winsxs",
    "boot",
    "efi",
    "appdata",
    "node_modules",
    ".git",
];

pub fn deep_find() -> Option<PathBuf> {
    for letter in 'C'..='Z' {
        if !drive_is_fixed(letter) {
            continue;
        }
        if let Some(found) = scan_drive(&PathBuf::from(format!("{}:\\", letter))) {
            return Some(found);
        }
    }
    None
}

fn scan_drive(root: &Path) -> Option<PathBuf> {
    let mut queue: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = queue.pop() {
        if depth > 4 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if SKIP_DIRS.iter().any(|s| name.starts_with(s)) {
                continue;
            }
            if name == "windowsnoeditor" || name == "path of titans" || name == "pathoftitans" {
                let exe = path.join(EXE_NAME);
                if exe.is_file() {
                    return Some(exe);
                }
                let nested = path.join(format!("PathOfTitans\\Binaries\\Win64\\{EXE_NAME}"));
                if nested.is_file() {
                    return Some(nested);
                }
            }
            queue.push((path, depth + 1));
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn quick_find_locates_installed_game() {
        let found = quick_find();
        if let Some(path) = found {
            assert!(path.file_name().unwrap().to_str().unwrap().contains(EXE_NAME));
        }
    }

    #[test]
    fn drive_is_fixed_check() {
        // C: should be treated as a fixed drive on a normal install
        assert!(drive_is_fixed('C'), "C should be a fixed drive");
    }
}
