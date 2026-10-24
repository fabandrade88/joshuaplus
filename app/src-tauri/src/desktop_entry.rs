//! Application-menu entry for the Linux AppImage (issue #15).
//!
//! An AppImage carries its `.desktop` file and icons inside its own image,
//! where the desktop never looks, so Joshua+ was missing from the launcher.
//! With the user's consent the app writes a host entry pointing at the
//! AppImage file itself (`$APPIMAGE`), copies its icons into the user's
//! hicolor theme, and rewrites the entry when the AppImage is moved. The
//! logic takes explicit directories so it is testable on any system.

use std::io;
use std::path::{Path, PathBuf};

/// File name of the entry in `applications/`.
pub const ENTRY_FILE: &str = "joshua-plus.desktop";
/// Icon name (and the window class the compositor matches the entry to).
const ICON: &str = "joshua-app";
/// hicolor sizes the bundle ships.
const ICON_SIZES: [&str; 4] = ["32x32", "128x128", "256x256", "512x512"];

/// Where the user's menu entries and icons live.
#[derive(Debug, Clone)]
pub struct Dirs {
    pub applications: PathBuf,
    pub icons: PathBuf,
}

impl Dirs {
    /// `$XDG_DATA_HOME` (default `~/.local/share`).
    pub fn for_user() -> Option<Self> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(Self { applications: data.join("applications"), icons: data.join("icons/hicolor") })
    }

    fn entry(&self) -> PathBuf {
        self.applications.join(ENTRY_FILE)
    }
}

/// The running AppImage and its mounted contents, if this is one.
pub fn running_appimage() -> Option<(PathBuf, Option<PathBuf>)> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let image = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_absolute())?;
    let appdir = std::env::var_os("APPDIR").map(PathBuf::from);
    Some((image, appdir))
}

/// Contents of the entry. `None` if the path cannot be written safely in an
/// `Exec` line (quotes, backslashes, `$`, backticks or line breaks).
pub fn entry_text(appimage: &Path) -> Option<String> {
    let path = appimage.to_str()?;
    if path.chars().any(|c| matches!(c, '"' | '\\' | '$' | '`' | '\n' | '\r')) {
        return None;
    }
    Some(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Joshua+\n\
         Comment=Stem separation and practice mixer for musicians\n\
         Exec=\"{path}\" %U\n\
         Icon={ICON}\n\
         Terminal=false\n\
         Categories=AudioVideo;Audio;Music;\n\
         StartupWMClass={ICON}\n\
         StartupNotify=true\n"
    ))
}

/// Whether an entry is installed.
pub fn is_installed(dirs: &Dirs) -> bool {
    dirs.entry().is_file()
}

/// The AppImage path the installed entry starts, if any.
pub fn installed_target(dirs: &Dirs) -> Option<PathBuf> {
    let text = std::fs::read_to_string(dirs.entry()).ok()?;
    let exec = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let quoted = exec.strip_prefix('"')?;
    Some(PathBuf::from(&quoted[..quoted.find('"')?]))
}

/// Writes the entry and copies the bundled icons (from `appdir`, the
/// mounted AppImage). Refreshes the desktop caches when the tools exist.
pub fn install(dirs: &Dirs, appimage: &Path, appdir: Option<&Path>) -> io::Result<()> {
    let text = entry_text(appimage)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "AppImage path not usable in a menu entry"))?;
    std::fs::create_dir_all(&dirs.applications)?;
    let tmp = dirs.applications.join(format!(".{ENTRY_FILE}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, dirs.entry())?;
    if let Some(appdir) = appdir {
        for size in ICON_SIZES {
            let from = appdir.join(format!("usr/share/icons/hicolor/{size}/apps/{ICON}.png"));
            if from.is_file() {
                let to = dirs.icons.join(size).join("apps");
                std::fs::create_dir_all(&to)?;
                std::fs::copy(&from, to.join(format!("{ICON}.png")))?;
            }
        }
    }
    refresh_caches(dirs);
    Ok(())
}

/// Removes the entry and the icons it installed.
pub fn remove(dirs: &Dirs) -> io::Result<()> {
    match std::fs::remove_file(dirs.entry()) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    for size in ICON_SIZES {
        let _ = std::fs::remove_file(dirs.icons.join(size).join("apps").join(format!("{ICON}.png")));
    }
    refresh_caches(dirs);
    Ok(())
}

/// The AppImage was moved or renamed since the entry was written: point the
/// entry at the new place. Returns whether it was rewritten.
pub fn follow_move(dirs: &Dirs, appimage: &Path, appdir: Option<&Path>) -> io::Result<bool> {
    match installed_target(dirs) {
        Some(target) if target != appimage => install(dirs, appimage, appdir).map(|()| true),
        _ => Ok(false),
    }
}

fn refresh_caches(dirs: &Dirs) {
    // Optional helpers: missing tools or failures leave a working entry.
    let _ = std::process::Command::new("update-desktop-database").arg(&dirs.applications).output();
    let _ = std::process::Command::new("gtk-update-icon-cache").args(["-q", "-t"]).arg(&dirs.icons).output();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn dirs(root: &Path) -> Dirs {
        Dirs { applications: root.join("applications"), icons: root.join("icons/hicolor") }
    }

    fn fake_appdir(root: &Path) -> PathBuf {
        let appdir = root.join("squashfs-root");
        for size in ICON_SIZES {
            let d = appdir.join(format!("usr/share/icons/hicolor/{size}/apps"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("joshua-app.png"), size).unwrap();
        }
        appdir
    }

    #[test]
    fn entry_starts_the_appimage_itself_and_matches_the_window() {
        let text = entry_text(Path::new("/home/ana/.local/bin/joshua plus")).unwrap();
        assert!(text.contains("Exec=\"/home/ana/.local/bin/joshua plus\" %U\n"));
        assert!(text.contains("StartupWMClass=joshua-app\n"));
        assert!(text.contains("Icon=joshua-app\n"));
        for bad in ["/tmp/a\"b", "/tmp/$HOME", "/tmp/a`b`", "/tmp/a\\b", "/tmp/a\nExec=evil"] {
            assert!(entry_text(Path::new(bad)).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn install_follow_move_and_remove() {
        let root = tempfile::tempdir().unwrap();
        let d = dirs(root.path());
        let appdir = fake_appdir(root.path());
        let first = root.path().join("Joshua+.AppImage");
        assert!(!is_installed(&d));

        install(&d, &first, Some(&appdir)).unwrap();
        assert!(is_installed(&d));
        assert_eq!(installed_target(&d), Some(first.clone()));
        for size in ICON_SIZES {
            assert!(d.icons.join(size).join("apps/joshua-app.png").is_file(), "{size}");
        }

        // Same place: nothing to do. Moved: the entry follows.
        assert!(!follow_move(&d, &first, Some(&appdir)).unwrap());
        let moved = root.path().join("bin/joshua-plus");
        assert!(follow_move(&d, &moved, Some(&appdir)).unwrap());
        assert_eq!(installed_target(&d), Some(moved));

        remove(&d).unwrap();
        assert!(!is_installed(&d));
        assert!(!d.icons.join("128x128/apps/joshua-app.png").exists());
        remove(&d).unwrap(); // removing twice is fine
    }

    #[test]
    fn nothing_is_written_when_not_installed() {
        let root = tempfile::tempdir().unwrap();
        let d = dirs(root.path());
        assert!(!follow_move(&d, Path::new("/x/Joshua+.AppImage"), None).unwrap());
        assert!(!d.applications.exists());
    }
}
