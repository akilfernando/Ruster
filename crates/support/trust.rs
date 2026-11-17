//! The AI apps this user has trusted, in one owner-only file:
//! `$XDG_CONFIG_HOME/rot/ai-apps.json`. Each app has a name, a random token
//! that its adapter presents, and whether the creator chose Allow Always.
//! Same-user processes can read and change this file; like the socket, it
//! protects against mistakes and other users, not against the user's own
//! programs.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub fn path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/nonexistent"));
    config.join("rot").join("ai-apps.json")
}

/// A client name as the adapter reports it: printable, at most 64 bytes.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.chars().any(char::is_control)
}

/// The name a creator recognizes for common MCP client names.
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub apps: Vec<App>,
}

impl Store {
    /// A missing, unreadable, malformed or group/world-accessible file reads as
    /// empty, so nothing is trusted from it.
    pub fn load(path: &Path) -> Self {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return Self::default();
        };
        if !meta.is_file()
            || meta.mode() & 0o077 != 0
            || meta.uid() != rustix::process::getuid().as_raw()
            || meta.len() > 65536
        {
            return Self::default();
        }
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn allowed(&self, name: &str, token: &str) -> bool {
        self.apps
            .iter()
            .any(|app| app.allowed && app.name == name && app.token == token)
    }

    /// Change the store under an exclusive lock and replace it atomically.
    fn update<T>(path: &Path, change: impl FnOnce(&mut Self) -> T) -> io::Result<T> {
        let directory = path.parent().ok_or_else(|| io::Error::other("no parent"))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        let mut store = Self::load(path);
        let result = change(&mut store);
        let temporary = path.with_extension(format!("tmp{}", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&store)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(result)
    }

    /// The adapter's token for `name`, creating an untrusted entry if needed.
    pub fn token_for(path: &Path, name: &str) -> io::Result<String> {
        if let Some(app) = Self::load(path).apps.into_iter().find(|a| a.name == name) {
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

    /// Allow Always: trust this name with this token from now on.
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

    /// Revoke: forget the app, so its next connection asks again.
    pub fn revoke(path: &Path, name: &str) -> io::Result<()> {
        Self::update(path, |store| store.apps.retain(|a| a.name != name))
    }
}

#[cfg(test)]
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
        assert!(!Store::load(&path).allowed("codex", &token));

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            !Store::load(&path).allowed("claude-code", &token),
            "readable by others"
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        Store::revoke(&path, "claude-code").unwrap();
        assert!(!Store::load(&path).allowed("claude-code", &token));
        assert_ne!(Store::token_for(&path, "claude-code").unwrap(), token);
    }
}
