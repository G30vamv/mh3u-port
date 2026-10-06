//! The owner's own copy of the game: where it is and whether it is the right one. The only way any other crate learns where the game's
//! files are. See `docs/pc-port.md`: the port has nothing of the game in it and refuses to go on without these files.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// The game's title id (US), and its update's.
pub const TITLE_ID: &str = "0005000010118300";
pub const UPDATE_TITLE_ID: &str = "0005000e10118300";

/// A path as typed, with a leading `~` meaning the home folder.
pub fn expand_home(text: &str) -> PathBuf {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("~/").or_else(|| (text == "~").then_some(""))
        && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(text)
}

/// The game's files: the base game and, beside it, its update if there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dump {
    /// The base game's folder (the one with `code`, `content` and `meta`).
    pub base: PathBuf,
    /// The update's folder, when it was found beside the base game.
    pub update: Option<PathBuf>,
}

fn is_dump(folder: &Path) -> bool {
    folder.join("content").is_dir() && folder.join("code").is_dir()
}

/// Whether the folder's `meta/meta.xml` names this title (a Wii U dump says which title it is there).
fn names_title(folder: &Path, title: &str) -> bool {
    std::fs::read_to_string(folder.join("meta/meta.xml"))
        .map(|xml| xml.to_ascii_lowercase().contains(&title.to_ascii_lowercase()))
        .unwrap_or(false)
}

impl Dump {
    /// Find the game in `folder`: either the game's own folder or a folder that holds the dumps (the game's is the one named like
    /// `... [Game] [<title id>]`, its update `... [Update] [<update id>]`), and check that it is this game.
    pub fn locate(folder: &Path) -> Result<Dump> {
        let mut base = None;
        let mut update = None;
        let mut candidates: Vec<PathBuf> = vec![folder.to_path_buf()];
        if let Ok(entries) = std::fs::read_dir(folder) {
            candidates.extend(entries.filter_map(|e| e.ok().map(|e| e.path())));
        }
        for c in candidates.iter().filter(|c| c.is_dir()) {
            let name = c.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_dump(c) && (names_title(c, TITLE_ID) || name.contains(TITLE_ID)) && base.is_none() {
                base = Some(c.clone());
            } else if is_dump(c) && (names_title(c, UPDATE_TITLE_ID) || name.contains(UPDATE_TITLE_ID)) && update.is_none() {
                update = Some(c.clone());
            }
        }
        // a folder that is the dump, whose update sits in a sibling folder
        if let (Some(b), None) = (&base, &update)
            && let Some(parent) = b.parent()
            && let Ok(entries) = std::fs::read_dir(parent)
        {
            update = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| is_dump(p) && (names_title(p, UPDATE_TITLE_ID) || p.to_string_lossy().contains(UPDATE_TITLE_ID)));
        }
        match base {
            Some(base) => Ok(Dump { base, update }),
            None => bail!(
                "no copy of Monster Hunter 3 Ultimate (Wii U, title id {TITLE_ID}) in {}. The port needs your own dump of the game; see docs/pc-port.md",
                folder.display()
            ),
        }
    }

    /// The dump named by the `MH3U_GAME_DIR` environment variable, if it is set and holds the game (the tests and tools that need the
    /// game skip or stop without it).
    pub fn from_env() -> Option<Dump> {
        Dump::locate(&expand_home(&std::env::var("MH3U_GAME_DIR").ok()?)).ok()
    }

    /// The game's executable: the update's when there is one (it replaces the base game's), else the base game's.
    pub fn executable(&self) -> Result<PathBuf> {
        let rpx_in = |folder: &Path| -> Option<PathBuf> {
            std::fs::read_dir(folder.join("code"))
                .ok()?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| p.extension().is_some_and(|x| x == "rpx"))
        };
        self.update
            .as_deref()
            .and_then(rpx_in)
            .or_else(|| rpx_in(&self.base))
            .with_context(|| format!("no .rpx in {}", self.base.join("code").display()))
    }

    /// A file of the game's content: the update's copy when it has one, else the base game's.
    pub fn content(&self, relative: &str) -> Option<PathBuf> {
        [self.update.as_deref(), Some(self.base.as_path())]
            .into_iter()
            .flatten()
            .map(|f| f.join("content").join(relative))
            .find(|p| p.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(folder: &Path, title: &str) {
        std::fs::create_dir_all(folder.join("code")).unwrap();
        std::fs::create_dir_all(folder.join("content")).unwrap();
        std::fs::create_dir_all(folder.join("meta")).unwrap();
        std::fs::write(folder.join("meta/meta.xml"), format!("<title_id>{title}</title_id>")).unwrap();
    }

    #[test]
    fn the_game_and_its_update_are_found_in_a_folder_of_dumps_and_nothing_else_is() {
        let root = std::env::temp_dir().join(format!("mh3u-dump-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        make(&root.join("GAME [Game] [0005000010118300]"), TITLE_ID);
        make(&root.join("GAME [Update] [0005000e10118300]"), UPDATE_TITLE_ID);
        make(&root.join("Other Game"), "0005000010000000");
        std::fs::write(root.join("GAME [Game] [0005000010118300]/code/game.rpx"), b"x").unwrap();
        std::fs::write(root.join("GAME [Update] [0005000e10118300]/code/game.rpx"), b"y").unwrap();
        let dump = Dump::locate(&root).unwrap();
        assert!(dump.base.to_string_lossy().contains("[Game]"));
        assert!(dump.update.as_ref().unwrap().to_string_lossy().contains("[Update]"));
        assert!(
            dump.executable().unwrap().to_string_lossy().contains("[Update]"),
            "the update's executable wins"
        );
        // pointing at the game's own folder finds its update beside it
        let own = Dump::locate(&root.join("GAME [Game] [0005000010118300]")).unwrap();
        assert!(own.update.is_some());
        // a folder with some other game, or nothing, is refused and says what is needed
        let err = Dump::locate(&root.join("Other Game")).unwrap_err().to_string();
        assert!(err.contains("docs/pc-port.md") && err.contains(TITLE_ID), "{err}");
        assert!(Dump::locate(&root.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
