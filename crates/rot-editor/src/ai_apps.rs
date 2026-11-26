//! Registering Rot Engine's MCP server (`rot`) with the AI apps on this
//! machine. Each app's own command-line tool is used where it has one (Claude
//! Code, Codex), so the app's file is written by the app itself; otherwise
//! the app's JSON file is read, only the `rot` entry is added or replaced,
//! and the file is replaced atomically. A copy of the file before the change
//! is kept beside it as `<file>.rot-backup`.
//!
//! Formats, from each app's documentation (read 2026-11-17):
//! - Claude Code: `claude mcp add-json rot '<json>' --scope user`
//!   (`~/.claude.json`, `mcpServers`), project file `.mcp.json`.
//! - Codex: `~/.codex/config.toml`, table `[mcp_servers.rot]` with
//!   `command`; project file `.codex/config.toml` (trusted projects). The
//!   ChatGPT desktop app shares this file. Written as text, not with
//!   `codex mcp add`: that command rewrites every other server's table
//!   (reorders keys, drops defaults), and other entries must stay as they are.
//! - Claude Desktop: `~/.config/Claude/claude_desktop_config.json`,
//!   `mcpServers`.
//! - Cursor: `~/.cursor/mcp.json`, `mcpServers`, `"type": "stdio"`.
//! - VS Code: `~/.config/Code/User/mcp.json`, `servers`, `"type": "stdio"`.
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NAME: &str = "rot";

#[derive(Clone, Debug, PartialEq)]
pub enum How {
    /// Claude Code's `claude` command.
    ClaudeCli(PathBuf),
    /// Codex's `config.toml`, edited as text.
    CodexToml(PathBuf),
    /// A JSON file with servers under `key`; `typed` adds `"type": "stdio"`.
    Json {
        path: PathBuf,
        key: &'static str,
        typed: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub name: &'static str,
    pub how: How,
}

/// Where to look: the home folder, the config folder and the program path.
#[derive(Clone, Debug)]
pub struct Machine {
    pub home: PathBuf,
    pub config: PathBuf,
    pub path: Vec<PathBuf>,
}
impl Machine {
    pub fn current() -> Self {
        let home = rot_support::platform::dirs::home_dir().unwrap_or_default();
        let config =
            rot_support::platform::dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        Self { home, config, path }
    }
    fn program(&self, name: &str) -> Option<PathBuf> {
        self.path
            .iter()
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    }
    fn codex_home(&self) -> PathBuf {
        std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".codex"))
    }
}

/// The AI apps installed here, in a fixed order.
pub fn installed(machine: &Machine) -> Vec<App> {
    let mut apps = Vec::new();
    if let Some(claude) = machine.program("claude") {
        apps.push(App {
            name: "Claude Code",
            how: How::ClaudeCli(claude),
        });
    }
    if machine.program("codex").is_some() || machine.codex_home().is_dir() {
        apps.push(App {
            name: "Codex",
            how: How::CodexToml(machine.codex_home().join("config.toml")),
        });
    }
    for (name, folder, file, key, typed) in [
        (
            "Claude Desktop",
            machine.config.join("Claude"),
            "claude_desktop_config.json",
            "mcpServers",
            false,
        ),
        (
            "Cursor",
            machine.home.join(".cursor"),
            "mcp.json",
            "mcpServers",
            true,
        ),
        (
            "VS Code",
            machine.config.join("Code").join("User"),
            "mcp.json",
            "servers",
            true,
        ),
    ] {
        if folder.is_dir() {
            apps.push(App {
                name,
                how: How::Json {
                    path: folder.join(file),
                    key,
                    typed,
                },
            });
        }
    }
    apps
}

/// The server entry for JSON configurations.
pub fn entry(adapter: &Path, typed: bool) -> Value {
    let mut entry = json!({"command": adapter, "args": []});
    if typed {
        entry["type"] = json!("stdio");
    }
    entry
}

/// The `rot-mcp` beside this editor, else the one on the program path.
pub fn adapter(machine: &Machine) -> PathBuf {
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name("rot-mcp"))
        .filter(|p| p.is_file())
        .or_else(|| machine.program("rot-mcp"))
        .unwrap_or_else(|| PathBuf::from("rot-mcp"))
}

/// What Copy Configuration puts on the clipboard.
pub fn snippet(adapter: &Path) -> String {
    let json = json!({"mcpServers": {NAME: entry(adapter, true)}});
    format!(
        "{}\n\n# Codex (~/.codex/config.toml)\n{}",
        serde_json::to_string_pretty(&json).unwrap_or_default(),
        with_rot_table("", adapter)
    )
}

fn toml_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

/// The file each app keeps its servers in.
fn file(app: &App, machine: &Machine) -> PathBuf {
    match &app.how {
        How::ClaudeCli(_) => machine.home.join(".claude.json"),
        How::CodexToml(path) => path.clone(),
        How::Json { path, .. } => path.clone(),
    }
}

/// Whether the app already lists `rot`.
pub fn connected(app: &App, machine: &Machine) -> bool {
    let path = file(app, machine);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    match &app.how {
        How::CodexToml(_) => has_rot_table(&text),
        How::ClaudeCli(_) => {
            serde_json::from_str::<Value>(&text).is_ok_and(|v| v["mcpServers"].get(NAME).is_some())
        }
        How::Json { key, .. } => {
            serde_json::from_str::<Value>(&text).is_ok_and(|v| v[*key].get(NAME).is_some())
        }
    }
}

/// What Undo needs to put back.
#[derive(Clone, Debug)]
pub struct Change {
    pub app: App,
    pub backup: Option<PathBuf>,
    /// The `rot` entry before Connect, if there was one (JSON apps).
    previous: Option<Value>,
}

fn keep_backup(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let mut backup = path.as_os_str().to_owned();
    backup.push(".rot-backup");
    let backup = PathBuf::from(backup);
    std::fs::copy(path, &backup)
        .map_err(|e| format!("Could not back up {}: {e}", path.display()))?;
    Ok(Some(backup))
}

fn run(program: &Path, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Could not run {}: {e}", program.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let error = String::from_utf8_lossy(&output.stderr);
    Err(error.lines().next().unwrap_or("failed").to_owned())
}

/// Read a JSON configuration; a missing file is empty, a broken one an error.
fn read_json(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} is not valid JSON ({e}); left unchanged", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("Could not read {}: {e}", path.display())),
    }
}

/// Write beside the file, then rename over it, keeping its permissions.
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".rot-{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(&temporary, text).map_err(fail)?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(&temporary, meta.permissions()).map_err(fail)?;
    }
    std::fs::rename(&temporary, path).map_err(fail)
}

fn set_entry(path: &Path, key: &str, value: Option<Value>) -> Result<Option<Value>, String> {
    let mut config = read_json(path)?;
    let root = config
        .as_object_mut()
        .ok_or_else(|| format!("{} is not a JSON object; left unchanged", path.display()))?;
    let servers = root
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| {
            format!(
                "{key} in {} is not an object; left unchanged",
                path.display()
            )
        })?;
    let previous = match value {
        Some(value) => servers.insert(NAME.into(), value),
        None => servers.remove(NAME),
    };
    write_json(path, &config)?;
    Ok(previous)
}

/// Register `rot` with an app. Only the `rot` entry changes.
pub fn connect(app: &App, machine: &Machine, adapter: &Path) -> Result<Change, String> {
    let backup = keep_backup(&file(app, machine))?;
    let mut previous = None;
    match &app.how {
        How::ClaudeCli(claude) => {
            let json = serde_json::to_string(
                &json!({"type":"stdio","command":adapter,"args":[],"env":{}}),
            )
            .map_err(|e| e.to_string())?;
            let _ = run(
                claude,
                &[
                    "mcp".as_ref(),
                    "remove".as_ref(),
                    NAME.as_ref(),
                    "--scope".as_ref(),
                    "user".as_ref(),
                ],
            );
            run(
                claude,
                &[
                    "mcp".as_ref(),
                    "add-json".as_ref(),
                    NAME.as_ref(),
                    json.as_ref(),
                    "--scope".as_ref(),
                    "user".as_ref(),
                ],
            )?;
        }
        How::CodexToml(path) => {
            let text = read_text(path)?;
            write_text(path, &with_rot_table(&without_rot_table(&text), adapter))?;
        }
        How::Json { path, key, typed } => {
            previous = set_entry(path, key, Some(entry(adapter, *typed)))?;
        }
    }
    Ok(Change {
        app: app.clone(),
        backup,
        previous,
    })
}

/// Undo a Connect: remove `rot`, or put back the entry it replaced.
pub fn undo(change: &Change) -> Result<(), String> {
    match &change.app.how {
        How::ClaudeCli(claude) => run(
            claude,
            &[
                "mcp".as_ref(),
                "remove".as_ref(),
                NAME.as_ref(),
                "--scope".as_ref(),
                "user".as_ref(),
            ],
        ),
        How::CodexToml(path) => {
            let text = read_text(path)?;
            write_text(path, &without_rot_table(&text))
        }
        How::Json { path, key, .. } => set_entry(path, key, change.previous.clone()).map(|_| ()),
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Could not read {}: {e}", path.display())),
    }
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".rot-{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, text).map_err(fail)?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(&temporary, meta.permissions()).map_err(fail)?;
    }
    std::fs::rename(&temporary, path).map_err(fail)
}

/// A TOML table header line, without brackets: `[a.b]` → `a.b`.
fn table_header(line: &str) -> Option<&str> {
    let line = line.trim();
    let inner = line.strip_prefix('[')?.split(']').next()?;
    Some(inner.trim())
}

fn is_rot_table(header: &str) -> bool {
    let rot = format!("mcp_servers.{NAME}");
    header == rot || header.starts_with(&format!("{rot}."))
}

fn has_rot_table(text: &str) -> bool {
    text.lines().filter_map(table_header).any(is_rot_table)
}

/// The text with the `[mcp_servers.rot]` table (and its sub-tables) and the
/// blank line before it removed; every other line is kept as it was.
fn without_rot_table(text: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in text.split_inclusive('\n') {
        if let Some(header) = table_header(line) {
            skipping = is_rot_table(header);
            if skipping && kept.last().is_some_and(|l| l.trim().is_empty()) {
                kept.pop();
            }
        }
        if !skipping {
            kept.push(line);
        }
    }
    kept.concat()
}

/// The text with a `[mcp_servers.rot]` table added at the end.
fn with_rot_table(text: &str, adapter: &Path) -> String {
    let mut text = text.to_owned();
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
    }
    // Codex passes MCP servers only the environment it is told to; rot-mcp
    // needs these to find the editor and the trusted-apps file.
    text.push_str(&format!(
        "[mcp_servers.{NAME}]\ncommand = {}\nenv_vars = [\"XDG_RUNTIME_DIR\", \"XDG_CONFIG_HOME\"]\n",
        toml_string(&adapter.to_string_lossy())
    ));
    text
}

/// Add `rot` to the project's own AI configuration files: `.mcp.json`
/// (Claude Code and others) and, when Codex is installed,
/// `.codex/config.toml`. Returns the files written.
pub fn add_to_project(
    project: &Path,
    machine: &Machine,
    adapter: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::new();
    let mcp = project.join(".mcp.json");
    set_entry(&mcp, "mcpServers", Some(entry(adapter, true)))?;
    written.push(mcp);
    if machine.program("codex").is_some() {
        let codex = project.join(".codex").join("config.toml");
        let text = read_text(&codex)?;
        write_text(&codex, &with_rot_table(&without_rot_table(&text), adapter))?;
        written.push(codex);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(root: &Path) -> Machine {
        Machine {
            home: root.join("home"),
            config: root.join("home/.config"),
            path: vec![root.join("bin")],
        }
    }

    // Only the rot entry changes; other servers and settings stay, and Undo
    // removes it or puts back what it replaced.
    #[test]
    fn json_apps_change_only_the_rot_entry_and_undo() {
        let root = tempfile::tempdir().unwrap();
        let machine = machine(root.path());
        std::fs::create_dir_all(machine.home.join(".cursor")).unwrap();
        std::fs::create_dir_all(machine.config.join("Code/User")).unwrap();
        let cursor = machine.home.join(".cursor/mcp.json");
        let original = json!({"mcpServers":{"other":{"command":"x"}},"theme":"dark"});
        std::fs::write(&cursor, serde_json::to_string(&original).unwrap()).unwrap();
        let apps = installed(&machine);
        assert_eq!(
            apps.iter().map(|a| a.name).collect::<Vec<_>>(),
            ["Cursor", "VS Code"]
        );

        let adapter = Path::new("/opt/rot/rot-mcp");
        let change = connect(&apps[0], &machine, adapter).unwrap();
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&cursor).unwrap()).unwrap();
        assert_eq!(
            written["mcpServers"]["other"],
            original["mcpServers"]["other"]
        );
        assert_eq!(written["theme"], "dark");
        assert_eq!(written["mcpServers"]["rot"]["command"], "/opt/rot/rot-mcp");
        assert_eq!(written["mcpServers"]["rot"]["type"], "stdio");
        assert!(connected(&apps[0], &machine));
        let backup: Value = serde_json::from_str(
            &std::fs::read_to_string(change.backup.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(backup, original);
        undo(&change).unwrap();
        let restored: Value =
            serde_json::from_str(&std::fs::read_to_string(&cursor).unwrap()).unwrap();
        assert_eq!(restored, original);
        assert!(!connected(&apps[0], &machine));

        // VS Code uses "servers"; a file that does not exist yet is created.
        let change = connect(&apps[1], &machine, adapter).unwrap();
        assert!(change.backup.is_none());
        let vscode = machine.config.join("Code/User/mcp.json");
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&vscode).unwrap()).unwrap();
        assert_eq!(written["servers"]["rot"]["type"], "stdio");

        // A broken file is never overwritten.
        std::fs::write(&cursor, "{ not json").unwrap();
        assert!(connect(&apps[0], &machine, adapter).is_err());
        assert_eq!(std::fs::read_to_string(&cursor).unwrap(), "{ not json");
    }

    // Other servers' tables stay byte for byte; Undo restores the file.
    #[test]
    fn codex_table_is_added_and_removed_as_text() {
        let root = tempfile::tempdir().unwrap();
        let mut machine = machine(root.path());
        machine.path.clear();
        let config = machine.home.join(".codex/config.toml");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        let original = "model = \"o3\"\n\n[mcp_servers.other]\ncommand = \"x\"\nargs = []\nstartup_timeout_sec = 120\n\n[mcp_servers.other.env]\nB = \"2\"\nA = \"1\"\n";
        std::fs::write(&config, original).unwrap();
        let apps = installed(&machine);
        assert_eq!(apps[0].name, "Codex");
        let change = connect(&apps[0], &machine, Path::new("/opt/rot/rot-mcp")).unwrap();
        let written = std::fs::read_to_string(&config).unwrap();
        assert!(written.starts_with(original));
        assert!(written.ends_with("\n[mcp_servers.rot]\ncommand = \"/opt/rot/rot-mcp\"\nenv_vars = [\"XDG_RUNTIME_DIR\", \"XDG_CONFIG_HOME\"]\n"));
        assert!(connected(&apps[0], &machine));
        connect(&apps[0], &machine, Path::new("/opt/rot/rot-mcp")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            written,
            "Reconnect replaces"
        );
        undo(&change).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
    }

    #[test]
    fn project_configuration_adds_rot_and_keeps_existing_codex_settings() {
        let root = tempfile::tempdir().unwrap();
        let mut machine = machine(root.path());
        std::fs::create_dir_all(root.path().join("bin")).unwrap();
        std::fs::write(root.path().join("bin/codex"), "").unwrap();
        machine.path = vec![root.path().join("bin")];
        let project = root.path().join("Game");
        std::fs::create_dir_all(project.join(".codex")).unwrap();
        std::fs::write(project.join(".codex/config.toml"), "model = \"o3\"").unwrap();
        let files = add_to_project(&project, &machine, Path::new("/opt/rot/rot-mcp")).unwrap();
        assert_eq!(files.len(), 2);
        let toml = std::fs::read_to_string(project.join(".codex/config.toml")).unwrap();
        assert_eq!(
            toml,
            "model = \"o3\"\n\n[mcp_servers.rot]\ncommand = \"/opt/rot/rot-mcp\"\nenv_vars = [\"XDG_RUNTIME_DIR\", \"XDG_CONFIG_HOME\"]\n"
        );
        add_to_project(&project, &machine, Path::new("/opt/rot/rot-mcp")).unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join(".codex/config.toml")).unwrap(),
            toml
        );
        let mcp: Value =
            serde_json::from_str(&std::fs::read_to_string(project.join(".mcp.json")).unwrap())
                .unwrap();
        assert_eq!(mcp["mcpServers"]["rot"]["command"], "/opt/rot/rot-mcp");
    }
}
