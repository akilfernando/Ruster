//! The AI apps this user has trusted, in one owner-only file:
//! `$XDG_CONFIG_HOME/rot/ai-apps.json`. Each app has a name, a random token
//! that its adapter presents, and whether the creator chose Allow Always.
//! Programs running as this user can read the tokens and add trust; the user
//! accepted that on 2026-11-17 (docs/decisions/2026-11-17-trust-ai-apps-once.md).
//! The checks below keep out other users and accidental damage.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Untrusted entries kept; older ones are dropped so the file stays small.
#[cfg(unix)]
const UNTRUSTED_KEPT: usize = 32;
#[cfg(unix)]
const MAX_BYTES: u64 = 65536;

pub fn path() -> PathBuf {
    crate::platform::dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
        .join("rot")
        .join("ai-apps.json")
}

/// A client name as the adapter reports it: at most 64 bytes of letters,
/// digits, ASCII punctuation and single inner spaces. This keeps out
/// invisible, right-to-left and look-alike spacing characters.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.trim() == name
        && !name.contains("  ")
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c.is_ascii_punctuation())
}

/// The name a creator recognizes for common MCP client names; any other name
/// is shown as the app reported it.
pub fn display_name(name: &str) -> String {
    match name {
        "claude-code" => "Claude Code".into(),
        "claude-ai" | "claude-desktop" => "Claude".into(),
        "codex-mcp-client" | "codex" => "Codex".into(),
        "cursor-vscode" | "cursor" => "Cursor".into(),
        "Visual Studio Code" | "vscode" => "VS Code".into(),
        other => other.into(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub token: String,
    pub allowed: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub apps: Vec<App>,
}

/// The folder must be a real directory owned by this user that no one else
/// can write to.
#[cfg(unix)]
fn check_directory(directory: &Path) -> io::Result<()> {
    crate::platform::ipc::check_private_dir_writable_only_by_owner(directory)
}

impl Store {
    /// The store, `None` when there is no file yet, or an error when the file
    /// or its folder is not private to this user or cannot be read.
    #[cfg(unix)]
    pub fn read(path: &Path) -> io::Result<Option<Self>> {
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        check_directory(path.parent().ok_or_else(|| io::Error::other("no parent"))?)?;
        if !meta.is_file()
            || meta.mode() & 0o077 != 0
            || meta.uid() != crate::platform::ipc::current_uid()
            || meta.len() > MAX_BYTES
        {
            return Err(io::Error::other(format!(
                "{} must be a private file of at most 64 KiB",
                path.display()
            )));
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }

    #[cfg(not(unix))]
    pub fn read(_path: &Path) -> io::Result<Option<Self>> {
        Err(crate::platform::ipc::unsupported())
    }

    /// For trust decisions: anything unreadable trusts nobody.
    pub fn load(path: &Path) -> Self {
        Self::read(path).ok().flatten().unwrap_or_default()
    }

    /// When the file last changed, to notice edits by other processes.
    pub fn modified(path: &Path) -> Option<std::time::SystemTime> {
        fs::symlink_metadata(path).and_then(|m| m.modified()).ok()
    }

    pub fn allowed(&self, name: &str, token: &str) -> bool {
        self.apps
            .iter()
            .any(|app| app.allowed && app.name == name && app.token == token)
    }

    /// Whether `name` is allowed with some token (it may present a new one).
    pub fn known(&self, name: &str) -> bool {
        self.apps.iter().any(|app| app.allowed && app.name == name)
    }

    /// Change the store under an exclusive lock and replace it atomically.
    /// A file that exists but cannot be read is left alone, not overwritten.
    #[cfg(unix)]
    fn update<T>(path: &Path, change: impl FnOnce(&mut Self) -> T) -> io::Result<T> {
        let directory = path.parent().ok_or_else(|| io::Error::other("no parent"))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        check_directory(directory)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        let mut store = Self::read(path)?.unwrap_or_default();
        let result = change(&mut store);
        let untrusted = store.apps.iter().filter(|a| !a.allowed).count();
        let mut drop = untrusted.saturating_sub(UNTRUSTED_KEPT);
        store.apps.retain(|a| {
            let keep = a.allowed || drop == 0;
            if !keep {
                drop -= 1;
            }
            keep
        });
        let temporary = path.with_extension(format!("tmp{}", std::process::id()));
        // unlink never follows a link, and create_new (O_EXCL) refuses to
        // open through one.
        let _ = fs::remove_file(&temporary);
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&store)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(result)
    }

    /// The adapter's token for `name`, creating an untrusted entry if needed.
    #[cfg(unix)]
    pub fn token_for(path: &Path, name: &str) -> io::Result<String> {
        if let Some(app) = Self::read(path)?
            .unwrap_or_default()
            .apps
            .into_iter()
            .find(|a| a.name == name)
        {
            return Ok(app.token);
        }
        let token = crate::connection::unpredictable()?;
        Self::update(path, |store| {
            match store.apps.iter().find(|a| a.name == name) {
                Some(app) => app.token.clone(),
                None => {
                    store.apps.push(App {
                        name: name.into(),
                        token: token.clone(),
                        allowed: false,
                    });
                    token
                }
            }
        })
    }

    #[cfg(not(unix))]
    pub fn token_for(_path: &Path, _name: &str) -> io::Result<String> {
        Err(crate::platform::ipc::unsupported())
    }

    /// Allow Always: trust this name with this token from now on.
    #[cfg(unix)]
    pub fn allow(path: &Path, name: &str, token: &str) -> io::Result<()> {
        Self::update(path, |store| {
            store.apps.retain(|a| a.name != name);
            store.apps.push(App {
                name: name.into(),
                token: token.into(),
                allowed: true,
            });
        })
    }

    #[cfg(not(unix))]
    pub fn allow(_path: &Path, _name: &str, _token: &str) -> io::Result<()> {
        Err(crate::platform::ipc::unsupported())
    }

    /// Revoke: forget the app, so its next connection asks again.
    #[cfg(unix)]
    pub fn revoke(path: &Path, name: &str) -> io::Result<()> {
        Self::update(path, |store| store.apps.retain(|a| a.name != name))
    }

    #[cfg(not(unix))]
    pub fn revoke(_path: &Path, _name: &str) -> io::Result<()> {
        Err(crate::platform::ipc::unsupported())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn allow_always_survives_reload_and_revoke_forgets() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("rot").join("ai-apps.json");
        let token = Store::token_for(&path, "claude-code").unwrap();
        assert_eq!(Store::token_for(&path, "claude-code").unwrap(), token);
        assert!(!Store::load(&path).allowed("claude-code", &token));
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
            0o700
        );

        Store::allow(&path, "claude-code", &token).unwrap();
        assert!(Store::load(&path).allowed("claude-code", &token));
        assert!(!Store::load(&path).allowed("claude-code", &"0".repeat(64)));
        assert!(Store::load(&path).known("claude-code"));
        assert!(!Store::load(&path).allowed("codex", &token));

        Store::revoke(&path, "claude-code").unwrap();
        assert!(!Store::load(&path).allowed("claude-code", &token));
        assert_ne!(Store::token_for(&path, "claude-code").unwrap(), token);
    }

    // A rejected file trusts nobody and is never rewritten empty.
    #[test]
    fn unreadable_store_is_kept_and_many_names_do_not_wipe_trust() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("rot").join("ai-apps.json");
        let token = Store::token_for(&path, "codex").unwrap();
        Store::allow(&path, "codex", &token).unwrap();
        for i in 0..400 {
            Store::token_for(&path, &format!("app {i}")).unwrap();
        }
        let store = Store::load(&path);
        assert!(store.allowed("codex", &token), "trust survives many names");
        assert_eq!(store.apps.len(), 1 + UNTRUSTED_KEPT);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!Store::load(&path).allowed("codex", &token));
        assert!(Store::allow(&path, "other", &token).is_err());
        assert!(Store::token_for(&path, "new").is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(Store::load(&path).allowed("codex", &token), "not rewritten");

        let shared = root.path().join("shared");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(Store::token_for(&shared.join("ai-apps.json"), "x").is_err());
    }

    #[test]
    fn names_reject_invisible_and_reordering_characters() {
        for good in [
            "claude-code",
            "Visual Studio Code",
            "codex-mcp-client",
            "Zed 1.2",
        ] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "",
            " claude",
            "claude ",
            "a  b",
            "claude\u{202e}edoc",
            "cla\u{200b}ude",
            "claude\u{a0}code",
            "claude\ncode",
            &"x".repeat(65),
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
