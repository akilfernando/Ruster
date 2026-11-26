//! Standard per-OS configuration, state, home, and runtime directories.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// User home directory (`$HOME` on Unix, `%USERPROFILE%` on Windows).
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// User configuration base directory (`$XDG_CONFIG_HOME` / `~/.config` on Unix,
/// `%APPDATA%` on Windows), ignoring non-absolute environment overrides.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        resolve_windows_dir(
            std::env::var_os("APPDATA"),
            home_dir(),
            &["AppData", "Roaming"],
        )
    }
    #[cfg(not(windows))]
    {
        resolve_unix_config_dir(std::env::var_os("XDG_CONFIG_HOME"), home_dir())
    }
}

/// User state directory without filtering relative overrides so callers can
/// reject relative `XDG_STATE_HOME` / `LOCALAPPDATA` explicitly.
pub fn state_dir_raw() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join("AppData").join("Local")))
    }
    #[cfg(not(windows))]
    {
        resolve_unix_state_dir_raw(std::env::var_os("XDG_STATE_HOME"), home_dir())
    }
}

/// Private per-user runtime directory for `app`.
pub fn runtime_dir(app: &str) -> PathBuf {
    #[cfg(unix)]
    {
        let uid = rustix::process::getuid().as_raw();
        resolve_unix_runtime_dir(
            app,
            uid,
            std::env::var_os("XDG_RUNTIME_DIR"),
            Path::new(&format!("/run/user/{uid}")).is_dir(),
            &std::env::temp_dir(),
        )
    }
    #[cfg(not(unix))]
    {
        std::env::temp_dir().join(app)
    }
}

#[allow(dead_code)]
fn resolve_unix_config_dir(xdg: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    xdg.map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(".config")))
}

#[allow(dead_code)]
fn resolve_unix_state_dir_raw(xdg: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    xdg.map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".local/state")))
}

#[allow(dead_code)]
fn resolve_unix_runtime_dir(
    app: &str,
    uid: u32,
    xdg: Option<OsString>,
    run_user_exists: bool,
    temp_dir: &Path,
) -> PathBuf {
    match xdg.map(PathBuf::from) {
        Some(runtime) if runtime.is_absolute() => runtime.join(app),
        _ if run_user_exists => PathBuf::from(format!("/run/user/{uid}/{app}")),
        _ => temp_dir.join(format!("{app}-{uid}")),
    }
}

#[allow(dead_code)]
fn resolve_windows_dir(
    env: Option<OsString>,
    home: Option<PathBuf>,
    fallback: &[&str],
) -> Option<PathBuf> {
    env.map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(|mut home| {
                for segment in fallback {
                    home.push(segment);
                }
                home
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_directory_resolution_honors_absolute_xdg_and_fallbacks() {
        let home = Some(PathBuf::from("/home/creator"));
        assert_eq!(
            resolve_unix_config_dir(Some("/xdg/cfg".into()), home.clone()),
            Some(PathBuf::from("/xdg/cfg"))
        );
        assert_eq!(
            resolve_unix_config_dir(Some("relative/cfg".into()), home.clone()),
            Some(PathBuf::from("/home/creator/.config"))
        );
        assert_eq!(
            resolve_unix_state_dir_raw(Some("relative/state".into()), home.clone()),
            Some(PathBuf::from("relative/state"))
        );
        assert_eq!(
            resolve_unix_state_dir_raw(None, home),
            Some(PathBuf::from("/home/creator/.local/state"))
        );
        assert_eq!(
            resolve_unix_runtime_dir(
                "rot",
                1000,
                Some("/run/user/custom".into()),
                true,
                Path::new("/tmp")
            ),
            PathBuf::from("/run/user/custom/rot")
        );
        assert_eq!(
            resolve_unix_runtime_dir("rot", 1000, Some("rel".into()), true, Path::new("/tmp")),
            PathBuf::from("/run/user/1000/rot")
        );
        assert_eq!(
            resolve_unix_runtime_dir("rot", 1000, None, false, Path::new("/tmp")),
            PathBuf::from("/tmp/rot-1000")
        );
    }

    #[test]
    fn host_dirs_return_expected_shape() {
        assert!(!runtime_dir("rot").as_os_str().is_empty());
        let _ = (home_dir(), config_dir(), state_dir_raw());
    }
}
