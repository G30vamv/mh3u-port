//! The window's icon: the game's own, from the dump's `meta/iconTex.tga` (`mh3u_dump::Dump::meta`). It is read when the window
//! opens and only kept in memory. Nothing is written to disk, and the repository has no artwork. Without the file, or with one
//! that cannot be read, the window keeps the default icon and one `[icon]` line says why.

use std::path::PathBuf;
use std::sync::Mutex;
use winit::window::{Icon, WindowAttributes};

/// The window icon's size (title bar and Alt+Tab), and the taskbar's on Windows.
pub const WINDOW_SIDE: u32 = 64;
pub const TASKBAR_SIDE: u32 = 128;

static FILE: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// The icon file to use (`None`: the dump has none). Set before `run`.
pub fn use_file(path: Option<PathBuf>) {
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

/// The icon at `side` x `side` from TGA bytes.
pub fn from_tga(data: &[u8], side: u32) -> anyhow::Result<Icon> {
    let img = mh3u_formats::tga::decode(data)?.resized(side);
    Ok(Icon::from_rgba(img.rgba, img.width, img.height)?)
}

/// The window's attributes with the game's icon, when there is one.
pub fn apply(attributes: WindowAttributes) -> WindowAttributes {
    let Some(path) = FILE.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return attributes;
    };
    let Some(path) = path else {
        eprintln!("[icon] no meta/iconTex.tga in the dump: the window keeps the default icon");
        return attributes;
    };
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[icon] {} could not be read ({e}): the default icon is kept", path.display());
            return attributes;
        }
    };
    match from_tga(&data, WINDOW_SIDE) {
        Ok(icon) => {
            eprintln!("[icon] the game's icon from {}", path.display());
            let attributes = attributes.with_window_icon(Some(icon));
            taskbar(attributes, &data)
        }
        Err(e) => {
            eprintln!(
                "[icon] {} is not an icon the port reads ({e:#}): the default icon is kept",
                path.display()
            );
            attributes
        }
    }
}

#[cfg(windows)]
fn taskbar(attributes: WindowAttributes, data: &[u8]) -> WindowAttributes {
    use winit::platform::windows::WindowAttributesExtWindows;
    match from_tga(data, TASKBAR_SIDE) {
        Ok(icon) => attributes.with_taskbar_icon(Some(icon)),
        Err(_) => attributes,
    }
}

#[cfg(not(windows))]
fn taskbar(attributes: WindowAttributes, _: &[u8]) -> WindowAttributes {
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tga_becomes_an_icon_and_a_bad_one_does_not() {
        // 1x1, 32 bits: no game file
        let tga = [0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 32, 8, 10, 20, 30, 255];
        assert!(from_tga(&tga, WINDOW_SIDE).is_ok());
        assert!(from_tga(&tga[..10], WINDOW_SIDE).is_err());
    }

    #[test]
    fn no_file_keeps_the_attributes() {
        use_file(None);
        let _ = apply(WindowAttributes::default());
        use_file(Some(PathBuf::from("/nonexistent/iconTex.tga")));
        let _ = apply(WindowAttributes::default());
    }
}
