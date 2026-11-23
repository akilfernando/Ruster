//! Watch the open Linux project's Assets tree without polling the filesystem.
use crate::behavior_files::LaunchSpec;
use eframe::egui::Context;
use rot_assets::{AssetKind, AssetStatus, AssetStore, PrefabFile, ProjectFileIssue};
use rustix::event::{poll, PollFd, PollFlags};
use rustix::fd::OwnedFd;
use rustix::fs::{
    self as at,
    inotify::{self, ReadFlags, WatchFlags},
};
use rustix::io::Errno;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::{
    fd::AsRawFd,
    unix::{fs::MetadataExt, net::UnixStream},
};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

pub(super) struct Snapshot {
    pub(super) store: AssetStore,
    pub(super) issues: Vec<ProjectFileIssue>,
    pub(super) warning: Option<String>,
    pub(super) folders: Vec<PathBuf>,
    pub(super) prefabs: Vec<PrefabFile>,
}

type Update = Result<Snapshot, String>;
type Mailbox = Arc<Mutex<Option<Update>>>;

#[derive(Debug)]
pub(super) struct ImportResult {
    pub(super) errors: Vec<String>,
    pub(super) created_script: Option<(String, PathBuf)>,
}

struct ImportBatch {
    folder: PathBuf,
    files: Vec<PathBuf>,
    result: SyncSender<ImportResult>,
}

pub(super) struct PublishedAsset {
    pub(super) outcome: rot_assets::AssetOutcome,
    pub(super) warning: Option<String>,
}

type AssetResult = Result<PublishedAsset, String>;

pub(super) struct AssetWrite {
    pub(super) prepared: Receiver<()>,
    pub(super) publish: SyncSender<()>,
    pub(super) result: Receiver<AssetResult>,
}

struct AssetJob {
    deadline: Instant,
    request: rot_assets::AssetRequest,
    prepared: SyncSender<()>,
    publish: Receiver<()>,
    result: SyncSender<AssetResult>,
}

pub(super) enum FileAction {
    Reimport(PathBuf),
    InterpretRoughness(PathBuf),
    Duplicate(PathBuf),
    Rename {
        source: PathBuf,
        stem: String,
    },
    Move {
        id: String,
        source: PathBuf,
        folder: PathBuf,
    },
    Trash(PathBuf),
    CreateFolder {
        parent: PathBuf,
        name: String,
    },
    CreateScript {
        parent: PathBuf,
        name: String,
    },
    RestoreMissingSource {
        id: String,
    },
    OpenScript {
        id: String,
        source: PathBuf,
        launch: LaunchSpec,
        diagnostic: Option<ScriptLocation>,
    },
}

pub(super) struct ScriptLocation {
    pub expected_hash: String,
    pub line: Option<u32>,
}

struct FileJob {
    action: FileAction,
    catalog: rot_assets::AssetStateIdentity,
    result: SyncSender<ImportResult>,
}

enum Job {
    ImportFiles(ImportBatch),
    FileAction(FileJob),
    AssetWrite(Box<AssetJob>),
}

pub(super) struct ProjectAssets {
    // Both a wakeup for queued imports and a shutdown signal when dropped.
    signal: UnixStream,
    jobs: SyncSender<Job>,
    active: Arc<AtomicBool>,
    latest: Mailbox,
}

impl ProjectAssets {
    pub(super) fn start(root: PathBuf, context: &Context) -> io::Result<Self> {
        let (signal, wake) = UnixStream::pair()?;
        signal.set_nonblocking(true)?;
        wake.set_nonblocking(true)?;
        let (jobs, queued) = mpsc::sync_channel(1);
        let active = Arc::new(AtomicBool::new(true));
        let latest = Mailbox::default();
        let worker_active = active.clone();
        let worker_latest = latest.clone();
        let ctx = context.clone();
        std::thread::Builder::new()
            .name("rot-project-assets".into())
            .spawn(move || {
                let mut send = |update| {
                    if worker_active.load(Ordering::Acquire) {
                        *worker_latest.lock().unwrap_or_else(|err| err.into_inner()) = Some(update);
                        ctx.request_repaint();
                    }
                };
                if let Err(err) = run(&root, &wake, &worker_active, &queued, &ctx, &mut send) {
                    send(Err(err));
                }
            })?;
        Ok(Self {
            signal,
            jobs,
            active,
            latest,
        })
    }

    pub(super) fn import_files(
        &self,
        folder: PathBuf,
        files: Vec<PathBuf>,
    ) -> Result<Receiver<ImportResult>, String> {
        let (result, reply) = mpsc::sync_channel(1);
        self.submit(Job::ImportFiles(ImportBatch {
            folder,
            files,
            result,
        }))?;
        Ok(reply)
    }

    pub(super) fn file_action(
        &self,
        action: FileAction,
        catalog: rot_assets::AssetStateIdentity,
    ) -> Result<Receiver<ImportResult>, String> {
        let (result, reply) = mpsc::sync_channel(1);
        self.submit(Job::FileAction(FileJob {
            action,
            catalog,
            result,
        }))?;
        Ok(reply)
    }

    pub(super) fn write_asset(
        &self,
        request: rot_assets::AssetRequest,
        deadline: Instant,
    ) -> Result<AssetWrite, String> {
        let (prepared, ready) = mpsc::sync_channel(1);
        let (authorize, publish) = mpsc::sync_channel(1);
        let (result, reply) = mpsc::sync_channel(1);
        self.submit(Job::AssetWrite(Box::new(AssetJob {
            deadline,
            request,
            prepared,
            publish,
            result,
        })))?;
        Ok(AssetWrite {
            prepared: ready,
            publish: authorize,
            result: reply,
        })
    }

    fn submit(&self, job: Job) -> Result<(), String> {
        self.jobs.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "Another asset operation is waiting.",
            mpsc::TrySendError::Disconnected(_) => "The project asset worker is unavailable.",
        })?;
        loop {
            match (&self.signal).write(&[1]) {
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    pub(super) fn take_update(&self) -> Option<Update> {
        self.latest
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .take()
    }
}

impl Drop for ProjectAssets {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

fn run(
    root: &Path,
    wake: &UnixStream,
    active: &AtomicBool,
    queued: &Receiver<Job>,
    context: &Context,
    send: &mut impl FnMut(Update),
) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|err| err.to_string())?;
    let held_root = fs::File::open(&root).map_err(|err| err.to_string())?;
    let metadata = held_root.metadata().map_err(|err| err.to_string())?;
    let same_root = || {
        fs::symlink_metadata(&root).is_ok_and(|current| {
            current.is_dir() && current.ino() == metadata.ino() && current.dev() == metadata.dev()
        })
    };
    let fd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)
        .map_err(|err| err.to_string())?;
    let watcher = WatchTree::new(&fd, &root);
    let attached = || active.load(Ordering::Acquire);
    let active = || attached() && same_root();
    let mut store = AssetStore::open(&root).map_err(|err| err.to_string())?;
    let mut paths = BTreeSet::new();
    let mut delivered = None;
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            let mut issues = store.reconcile_project_files(&paths, active);
            let (prefabs, prefab_issues) = store.read_project_prefabs();
            issues.extend(prefab_issues);
            let warning = Some(format!("Couldn't watch Assets: {error}"));
            send(Ok(Snapshot {
                store,
                issues,
                warning,
                folders: Vec::new(),
                prefabs,
            }));
            return Ok(());
        }
    };
    while active() {
        let mut warning = None;
        if paths.remove(Path::new(".rot-assets.json"))
            && store.with_current_catalog(|| Ok(())).is_err()
        {
            match AssetStore::open(&root) {
                Ok(current) if current.revision() > store.revision() => {
                    store = current;
                    paths.insert(PathBuf::from("Assets"));
                }
                _ => {
                    warning = Some(
                        "The asset catalog changed externally. Reopen the project to reload it."
                            .into(),
                    )
                }
            }
        }
        let mut import = None;
        let mut asset_write = None;
        let mut reimport = None;
        let mut moved = None;
        let mut restored = None;
        let mut created_script = None;
        match queued.try_recv() {
            Ok(Job::ImportFiles(batch)) => {
                let mut failures = Vec::new();
                for file in &batch.files {
                    if !active() {
                        break;
                    }
                    match copy_source(&root, &held_root, &batch.folder, file, false, active) {
                        Ok(path) => {
                            paths.insert(path);
                        }
                        Err(error) => failures.push(format!("{}: {error}", file.display())),
                    }
                }
                import = Some((batch.result, failures));
            }
            Ok(Job::FileAction(job)) => {
                let is_create_script = matches!(&job.action, FileAction::CreateScript { .. });
                let is_launch = matches!(&job.action, FileAction::OpenScript { .. });
                let explicit_reimport = matches!(&job.action, FileAction::Reimport(_));
                let move_source = match &job.action {
                    FileAction::Move { id, source, .. } => {
                        let hash = store.asset_info(id).and_then(|info| info.hash.clone());
                        Some((id.clone(), source.clone(), hash))
                    }
                    _ => None,
                };
                let restore_id = match &job.action {
                    FileAction::RestoreMissingSource { id } => store
                        .asset_info(id)
                        .map(|info| (id.clone(), info.hash.clone())),
                    _ => None,
                };
                let outcome = if store.matches_identity(&job.catalog) && active() {
                    file_action(&mut store, &held_root, &mut watcher, job.action, active)
                } else {
                    Err("The project or asset catalog changed. Try again.".into())
                };
                let failures = match outcome {
                    Ok(path) => {
                        if explicit_reimport {
                            reimport = Some(path.clone());
                        }
                        if let Some((id, source, hash)) = move_source {
                            paths.insert(source);
                            moved = Some((id, path.clone(), hash));
                        }
                        if let Some((id, hash)) = restore_id {
                            restored = Some((id, path.clone(), hash));
                        }
                        if is_create_script {
                            created_script = Some(path.clone());
                        }
                        if !is_launch {
                            paths.insert(path);
                        }
                        Vec::new()
                    }
                    Err(error) => vec![error],
                };
                import = Some((job.result, failures));
            }
            Ok(Job::AssetWrite(job)) => {
                let outcome = finish_asset_write(
                    &mut store,
                    context,
                    active,
                    &job.prepared,
                    &job.publish,
                    job.request,
                    job.deadline,
                );
                asset_write = Some((job.result, outcome));
            }
            Err(_) => {}
        }
        if paths.is_empty()
            && delivered.is_some()
            && warning.is_none()
            && import.is_none()
            && asset_write.is_none()
        {
            let Some(changed) = watcher.wait(wake).map_err(|err| err.to_string())? else {
                break;
            };
            paths = changed;
            continue;
        }
        let (issues, prefabs): (Vec<ProjectFileIssue>, Vec<PrefabFile>) = if !paths.is_empty()
            || delivered.is_none()
            || asset_write
                .as_ref()
                .is_some_and(|(_, outcome)| outcome.is_ok())
        {
            let mut issues =
                store.reconcile_project_files_with_reimport(&paths, reimport.as_deref(), active);
            let (prefabs, prefab_issues) = store.read_project_prefabs();
            issues.extend(prefab_issues);
            (issues, prefabs)
        } else {
            delivered.as_ref().map_or_else(
                || (Vec::new(), Vec::new()),
                |(_, issues, _, _, prefabs): &(_, Vec<ProjectFileIssue>, _, _, Vec<PrefabFile>)| {
                    (issues.clone(), prefabs.clone())
                },
            )
        };
        let write_warning = asset_write.as_ref().and_then(|(_, result)| {
            let source = match result.as_ref().ok()? {
                rot_assets::AssetOutcome::Asset(info) => &info.source,
                rot_assets::AssetOutcome::Model(model) => &model.source,
            };
            issues
                .iter()
                .find(|issue| &issue.path == source)
                .map(ToString::to_string)
        });
        let state = store.state_identity();
        let mut folders: Vec<_> = watcher.paths.values().cloned().collect();
        folders.sort_unstable();
        if delivered.as_ref().is_none_or(
            |(previous, previous_issues, previous_warning, previous_folders, previous_prefabs)| {
                previous != &state
                    || previous_issues != &issues
                    || previous_warning != &warning
                    || previous_folders != &folders
                    || previous_prefabs != &prefabs
            },
        ) {
            send(Ok(Snapshot {
                store: store.project_snapshot(),
                issues: issues.clone(),
                warning: warning.clone(),
                folders: folders.clone(),
                prefabs: prefabs.clone(),
            }));
            delivered = Some((state, issues, warning, folders, prefabs));
        }
        if let Some((result, mut failures)) = import {
            if let Some((id, path, hash)) = moved {
                if !store.asset_info(&id).is_some_and(|info| {
                    Path::new(&info.source) == path
                        && info.status == AssetStatus::Ready
                        && info.hash == hash
                }) {
                    failures.push(format!(
                        "Files moved to {}, but the catalog could not confirm asset {id}. Check Assets.",
                        path.display()
                    ));
                }
            }
            if let Some((id, path, hash)) = restored {
                if !store.asset_info(&id).is_some_and(|info| {
                    Path::new(&info.source) == path
                        && info.status == AssetStatus::Ready
                        && info.hash == hash
                }) {
                    failures.push(format!(
                        "Published {}, but Assets could not confirm the restored source. Check its import status.",
                        path.display()
                    ));
                }
            }
            let created_script = created_script.and_then(|path| {
                let ready = store.infos().find(|info| {
                    info.kind == AssetKind::Script
                        && Path::new(&info.source) == path
                        && info.status == AssetStatus::Ready
                        && store.script(&info.id) == Some(OBJECT_LUA_TEMPLATE)
                });
                if let Some(info) = ready {
                    Some((info.id.clone(), path))
                } else {
                    failures.push(format!("Created {}, but Assets could not confirm the new script. Check its import status.", path.display()));
                    None
                }
            });
            let _ = result.try_send(ImportResult {
                errors: failures,
                created_script,
            });
            context.request_repaint();
        }
        if let Some((result, outcome)) = asset_write {
            let _ = result.try_send(outcome.map(|outcome| PublishedAsset {
                outcome,
                warning: write_warning,
            }));
            context.request_repaint();
        }
        let Some(changed) = watcher.wait(wake).map_err(|err| err.to_string())? else {
            break;
        };
        paths = changed;
    }
    if attached() && !same_root() {
        return Err("The project folder moved or was replaced.".into());
    }
    Ok(())
}

fn finish_asset_write(
    store: &mut AssetStore,
    context: &Context,
    active: impl Fn() -> bool,
    ready: &SyncSender<()>,
    publish: &Receiver<()>,
    request: rot_assets::AssetRequest,
    deadline: Instant,
) -> Result<rot_assets::AssetOutcome, String> {
    if !store.matches_identity(request.state_identity()) {
        return Err("Accepted catalog changed during preparation.".into());
    }
    let prepared = request.prepare();
    ready
        .try_send(())
        .map_err(|_| "The asset request was cancelled.")?;
    context.request_repaint();
    if publish
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .is_err()
    {
        return Err("The asset request was cancelled.".into());
    }
    store
        .publish_project_request(prepared, active)
        .map_err(|err| err.to_string())
}

fn open_folder(root: &fs::File, folder: &Path) -> rustix::io::Result<OwnedFd> {
    at::openat2(
        root,
        folder,
        at::OFlags::RDONLY | at::OFlags::DIRECTORY | at::OFlags::CLOEXEC,
        at::Mode::empty(),
        at::ResolveFlags::BENEATH | at::ResolveFlags::NO_SYMLINKS,
    )
}

fn ensure_assets(root: &fs::File, folder: &Path) -> Result<(), String> {
    if folder == Path::new("Assets") {
        match at::mkdirat(root, folder, at::Mode::from_raw_mode(0o755)) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn file_action(
    store: &mut AssetStore,
    held_root: &fs::File,
    watcher: &mut WatchTree<'_>,
    action: FileAction,
    active: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    let root = store.root();
    match action {
        FileAction::Reimport(source) => {
            AssetStore::project_source_limit(source.to_str().unwrap_or_default())
                .map_err(|error| error.to_string())?;
            Ok(source)
        }
        FileAction::InterpretRoughness(source) => {
            store
                .interpret_project_texture(
                    source.to_str().unwrap_or_default(),
                    rot_assets::TextureRole::Roughness,
                    active,
                )
                .map_err(|error| error.to_string())?;
            Ok(source)
        }
        FileAction::Duplicate(source) => {
            AssetStore::project_source_limit(source.to_str().unwrap_or_default())
                .map_err(|error| error.to_string())?;
            let parent = source
                .parent()
                .filter(|path| path.starts_with("Assets"))
                .unwrap_or(Path::new("Assets"));
            let path = copy_source(root, held_root, parent, &root.join(&source), true, &active)?;
            store
                .duplicate_import_settings(
                    source.to_str().unwrap_or_default(),
                    path.to_str().unwrap_or_default(),
                    active,
                )
                .map_err(|error| {
                    format!("File copied, but import settings couldn't be duplicated: {error}")
                })?;
            Ok(path)
        }
        FileAction::Rename { source, stem } => {
            validate_leaf(&stem)?;
            let (folder, leaf) = open_source_folder(held_root, &source)?;
            let extension = source
                .extension()
                .and_then(|ext| ext.to_str())
                .ok_or("Asset has no file extension.")?;
            let name = format!("{stem}.{extension}");
            let destination = source.with_file_name(&name);
            AssetStore::project_source_limit(destination.to_str().unwrap_or_default())
                .map_err(|error| error.to_string())?;
            if name != leaf {
                store
                    .with_current_catalog(|| {
                        store.validate_movable_sidecar(source.to_str().unwrap_or_default())?;
                        move_asset_pair(&folder, leaf, &folder, &name, &active)
                            .map_err(rot_assets::AssetError)
                    })
                    .map_err(|error| error.to_string())?;
            }
            Ok(destination)
        }
        FileAction::Move { id, source, folder } => {
            let info = store.asset_info(&id).filter(|info| Path::new(&info.source) == source
                && info.status == AssetStatus::Ready
                && matches!(info.kind, AssetKind::Mesh | AssetKind::Script | AssetKind::Sound))
                .ok_or("Choose a ready, registered OBJ, Lua or WAV in Assets. The asset may have changed.")?;
            if !folder.starts_with("Assets") || source.parent() == Some(folder.as_path())
                || folder.components().any(|part| !matches!(part, std::path::Component::Normal(name) if !name.to_string_lossy().starts_with('.')))
            {
                return Err("Choose a different existing folder inside Assets.".into());
            }
            let (origin, leaf) = open_source_folder(held_root, &source)?;
            let destination = folder.join(leaf);
            let limit = AssetStore::project_source_limit(destination.to_str().unwrap_or_default())
                .map_err(|error| error.to_string())?;
            let target = open_folder(held_root, &folder).map_err(|error| {
                format!("Destination must be an existing folder without symlinks: {error}")
            })?;
            store
                .with_current_catalog(|| {
                    let failure = rot_assets::AssetError;
                    let sidecar = format!("{leaf}.rotmeta");
                    for name in [leaf, sidecar.as_str()] {
                        match at::statat(&target, name, at::AtFlags::SYMLINK_NOFOLLOW) {
                            Ok(_) => {
                                return Err(failure(format!(
                                    "Destination already contains {name}."
                                )))
                            }
                            Err(Errno::NOENT) => {}
                            Err(error) => return Err(failure(error.to_string())),
                        }
                    }
                    let input = fs::File::from(
                        at::openat(
                            &origin,
                            leaf,
                            at::OFlags::RDONLY
                                | at::OFlags::NOFOLLOW
                                | at::OFlags::NONBLOCK
                                | at::OFlags::CLOEXEC,
                            at::Mode::empty(),
                        )
                        .map_err(|error| failure(error.to_string()))?,
                    );
                    let source_changed = || {
                        failure(
                            "Source changed since it was imported. Reimport and try again.".into(),
                        )
                    };
                    let original = at::fstat(&input).map_err(|error| failure(error.to_string()))?;
                    if at::FileType::from_raw_mode(original.st_mode) != at::FileType::RegularFile
                        || original.st_size < 0
                        || original.st_size as u64 > limit as u64
                    {
                        return Err(source_changed());
                    }
                    let mut hash = Sha256::new();
                    let bytes = io::copy(&mut input.take(limit as u64 + 1), &mut hash)
                        .map_err(|error| failure(error.to_string()))?;
                    if bytes != original.st_size as u64
                        || info.hash.as_deref() != Some(format!("{:x}", hash.finalize()).as_str())
                    {
                        return Err(source_changed());
                    }
                    let identity = |stat: at::Stat| {
                        (
                            stat.st_ino,
                            stat.st_dev,
                            stat.st_ctime,
                            stat.st_ctime_nsec,
                            stat.st_size,
                        )
                    };
                    let stat = |name: &str| {
                        at::statat(&origin, name, at::AtFlags::SYMLINK_NOFOLLOW)
                            .map(identity)
                            .map_err(|_| source_changed())
                    };
                    let original_sidecar = stat(&sidecar)?;
                    store.validate_movable_sidecar(&info.source)?;
                    if stat(leaf)? != identity(original) || stat(&sidecar)? != original_sidecar {
                        return Err(source_changed());
                    }
                    move_asset_pair(&origin, leaf, &target, leaf, &active).map_err(failure)
                })
                .map_err(|error| error.to_string())?;
            Ok(destination)
        }
        FileAction::Trash(source) => {
            let (folder, leaf) = open_source_folder(held_root, &source)?;
            let trash = Path::new(".rot-trash");
            match at::mkdirat(held_root, trash, at::Mode::from_raw_mode(0o700)) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(error) => return Err(error.to_string()),
            }
            let trash = open_folder(held_root, trash).map_err(|error| error.to_string())?;
            let staging = tempfile::Builder::new()
                .prefix("asset-")
                .tempdir_in(format!("/proc/self/fd/{}", trash.as_raw_fd()))
                .map_err(|error| error.to_string())?;
            let target = staging
                .path()
                .file_name()
                .ok_or("Couldn't create project trash.")?;
            let target = at::openat2(
                &trash,
                target,
                at::OFlags::RDONLY | at::OFlags::DIRECTORY | at::OFlags::CLOEXEC,
                at::Mode::empty(),
                at::ResolveFlags::BENEATH | at::ResolveFlags::NO_SYMLINKS,
            )
            .map_err(|error| error.to_string())?;
            // Retain even if a later rollback fails so neither file is discarded.
            let _ = staging.keep();
            store
                .with_current_catalog(|| {
                    store.validate_movable_sidecar(source.to_str().unwrap_or_default())?;
                    move_asset_pair(&folder, leaf, &target, leaf, &active)
                        .map_err(rot_assets::AssetError)
                })
                .map_err(|error| error.to_string())?;
            Ok(source)
        }
        FileAction::CreateScript { parent, name } => {
            create_script(store, held_root, &parent, &name, active)
        }
        FileAction::RestoreMissingSource { id } => {
            restore_missing_source(store, held_root, &id, active)
        }
        FileAction::OpenScript {
            id,
            source,
            launch,
            diagnostic,
        } => {
            let mut child = store
                .with_current_catalog(|| {
                    let mut command = project_script_command(
                        store,
                        held_root,
                        &id,
                        &source,
                        &launch,
                        diagnostic.as_ref(),
                        active,
                    )
                    .map_err(rot_assets::AssetError)?;
                    command
                        .spawn()
                        .map_err(|error| rot_assets::AssetError(error.to_string()))
                })
                .map_err(|error| {
                    format!(
                        "Couldn't open {} in the code editor: {error}",
                        source.display()
                    )
                })?;
            // Wait away from the asset worker and catalog lock. Closing the
            // project does not stop an external editor that is still running.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(source)
        }
        FileAction::CreateFolder { parent, name } => {
            validate_leaf(&name)?;
            if !parent.starts_with("Assets") {
                return Err("Choose a folder inside Assets.".into());
            }
            let folder = parent.join(&name);
            if AssetStore::project_source_limit(&folder.join("a.obj").to_string_lossy()).is_err() {
                return Err("The folder path is too long to contain assets.".into());
            }
            ensure_assets(held_root, &parent)?;
            let location = open_folder(held_root, &parent).map_err(|error| error.to_string())?;
            if !active() {
                return Err("Project closed before the folder was created.".into());
            }
            at::mkdirat(&location, name.as_str(), at::Mode::from_raw_mode(0o755))
                .map_err(|error| error.to_string())?;
            watcher.rebuild().map_err(|error| error.to_string())?;
            Ok(folder)
        }
    }
}

const OBJECT_LUA_TEMPLATE: &str = r#"-- Attach this script to an object with a Mesh Renderer.
-- update runs each simulation step. Return the object's transform.
return {
    update = function(object, properties, dt, host)
        -- Change object.translation, object.rotation or object.scale here.
        return object
    end,
}
"#;

pub(super) fn script_filename(input: &str) -> Result<String, String> {
    let input = input.trim();
    let stem = input.strip_suffix(".lua").unwrap_or(input);
    validate_leaf(stem)?;
    if stem.chars().any(char::is_control) || stem.len() > 120 {
        return Err("Use a name up to 120 UTF-8 bytes without control characters.".into());
    }
    Ok(format!("{stem}.lua"))
}

fn create_script(
    store: &AssetStore,
    root: &fs::File,
    folder: &Path,
    name: &str,
    active: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    let name = script_filename(name)?;
    if !folder.starts_with("Assets") {
        return Err("Choose a folder inside Assets.".into());
    }
    let source = folder.join(&name);
    let relative = source.to_str().ok_or("Script path must be Unicode.")?;
    AssetStore::project_source_limit(relative).map_err(|error| error.to_string())?;
    if !active() {
        return Err("Project closed before the script was created.".into());
    }
    ensure_assets(root, folder)?;
    let destination = open_folder(root, folder).map_err(|error| error.to_string())?;
    let identity = at::fstat(&destination).map_err(|error| error.to_string())?;
    let directory = PathBuf::from(format!("/proc/self/fd/{}", destination.as_raw_fd()));
    let mut scratch =
        tempfile::NamedTempFile::new_in(&directory).map_err(|error| error.to_string())?;
    scratch
        .write_all(OBJECT_LUA_TEMPLATE.as_bytes())
        .map_err(|error| error.to_string())?;
    scratch
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    store
        .with_current_catalog(|| {
            let error = |message: String| {
                rot_assets::AssetError(format!("Couldn't create {relative}: {message}"))
            };
            if !active()
                || !open_folder(root, folder)
                    .and_then(at::fstat)
                    .is_ok_and(|now| now.st_ino == identity.st_ino && now.st_dev == identity.st_dev)
            {
                return Err(error("The project or folder changed. Try again.".into()));
            }
            match at::statat(&destination, name.as_str(), at::AtFlags::SYMLINK_NOFOLLOW) {
                Ok(_) => return Err(error("A file with that name already exists.".into())),
                Err(Errno::NOENT) => {}
                Err(reason) => return Err(error(reason.to_string())),
            }
            match at::statat(
                &destination,
                format!("{name}.rotmeta"),
                at::AtFlags::SYMLINK_NOFOLLOW,
            ) {
                Ok(_) => return Err(error("That name already has an import sidecar.".into())),
                Err(Errno::NOENT) => {}
                Err(reason) => return Err(error(reason.to_string())),
            }
            scratch
                .persist_noclobber(directory.join(&name))
                .map_err(|failure| {
                    error(if failure.error.kind() == io::ErrorKind::AlreadyExists {
                        "A file with that name already exists.".into()
                    } else {
                        failure.error.to_string()
                    })
                })?;
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    Ok(source)
}

// Read one bounded file under an already verified directory, retaining the
// opened file's identity and change timestamps for the publication check.
fn read_restore_input(
    parent: &OwnedFd,
    leaf: &str,
    limit: usize,
) -> Result<(Vec<u8>, at::Stat), String> {
    let file = at::openat(
        parent,
        leaf,
        at::OFlags::RDONLY | at::OFlags::NOFOLLOW | at::OFlags::NONBLOCK | at::OFlags::CLOEXEC,
        at::Mode::empty(),
    )
    .map(fs::File::from)
    .map_err(|error| error.to_string())?;
    let before = at::fstat(&file).map_err(|error| error.to_string())?;
    if at::FileType::from_raw_mode(before.st_mode) != at::FileType::RegularFile {
        return Err("Expected a regular file.".into());
    }
    let mut bytes = Vec::new();
    (&file)
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("The file exceeds its size limit.".into());
    }
    let after = at::fstat(&file).map_err(|error| error.to_string())?;
    if !same_restore_input(&before, &after) {
        return Err("The file changed during verification. Try again.".into());
    }
    Ok((bytes, after))
}

fn same_restore_input(before: &at::Stat, now: &at::Stat) -> bool {
    before.st_ino == now.st_ino
        && before.st_dev == now.st_dev
        && before.st_size == now.st_size
        && before.st_mtime == now.st_mtime
        && before.st_mtime_nsec == now.st_mtime_nsec
        && before.st_ctime == now.st_ctime
        && before.st_ctime_nsec == now.st_ctime_nsec
}

fn restore_missing_source(
    store: &AssetStore,
    root: &fs::File,
    id: &str,
    active: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    let (cache, limit) = store
        .missing_source_restore_cache(id)
        .map_err(|error| error.to_string())?;
    let info = store.asset_info(id).expect("accepted missing asset");
    let source = Path::new(&info.source);
    let (destination, leaf) = open_source_folder(root, source)?;
    let identity = at::fstat(&destination).map_err(|error| error.to_string())?;
    let sidecar = format!("{leaf}.rotmeta");
    let (sidecar_bytes, sidecar_stat) = read_restore_input(
        &destination,
        &sidecar,
        rot_assets::MAX_PROJECT_SIDECAR_BYTES,
    )
    .map_err(|reason| {
        format!(
            "Check the original import sidecar for {}: {reason}",
            info.source
        )
    })?;
    let cached_folder = open_folder(root, cache.parent().expect("cache directory"))
        .map_err(|error| format!("Accepted source cache is unavailable: {error}"))?;
    let (bytes, _) = read_restore_input(
        &cached_folder,
        cache
            .file_name()
            .and_then(|name| name.to_str())
            .expect("ASCII cache name"),
        limit,
    )
    .map_err(|reason| format!("Accepted source cache is unavailable: {reason}"))?;
    store
        .verify_missing_source_restore(id, &sidecar_bytes, &bytes)
        .map_err(|error| error.to_string())?;
    let directory = PathBuf::from(format!("/proc/self/fd/{}", destination.as_raw_fd()));
    let mut staged =
        tempfile::NamedTempFile::new_in(&directory).map_err(|error| error.to_string())?;
    staged
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    store
        .with_current_catalog(|| {
            let error = |reason: &str| {
                rot_assets::AssetError(format!("Couldn't restore {}: {reason}", info.source))
            };
            if !active()
                || !open_source_folder(root, source)
                    .and_then(|(folder, _)| at::fstat(folder).map_err(|error| error.to_string()))
                    .is_ok_and(|now| now.st_ino == identity.st_ino && now.st_dev == identity.st_dev)
            {
                return Err(error("The project or asset folder changed. Try again."));
            }
            if !at::statat(
                &destination,
                sidecar.as_str(),
                at::AtFlags::SYMLINK_NOFOLLOW,
            )
            .is_ok_and(|now| same_restore_input(&sidecar_stat, &now))
            {
                return Err(error("The import sidecar changed. Check Assets and retry."));
            }
            staged
                .persist_noclobber(directory.join(leaf))
                .map_err(|failure| {
                    if failure.error.kind() == io::ErrorKind::AlreadyExists {
                        error("A source file already exists. Inspect it in Assets.")
                    } else {
                        error(&failure.error.to_string())
                    }
                })?;
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    Ok(source.to_owned())
}

fn project_script_command(
    store: &AssetStore,
    root: &fs::File,
    id: &str,
    source: &Path,
    launch: &LaunchSpec,
    diagnostic: Option<&ScriptLocation>,
    active: impl Fn() -> bool,
) -> Result<std::process::Command, String> {
    if !store
        .asset_info(id)
        .is_some_and(|info| info.kind == AssetKind::Script && Path::new(&info.source) == source)
    {
        return Err(
            "The selected script moved or is no longer registered. Select it again.".into(),
        );
    }
    let (folder, leaf) = open_source_folder(root, source)?;
    let stat = at::statat(&folder, leaf, at::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|error| error.to_string())?;
    if at::FileType::from_raw_mode(stat.st_mode) != at::FileType::RegularFile {
        return Err("Expected a regular Lua file in Assets.".into());
    }
    let verified = if let Some(location) = diagnostic {
        let changed =
            "The Lua source changed since this error. Run Play again for a current location.";
        let info = store.asset_info(id).expect("registered Lua asset");
        if info.status != AssetStatus::Ready
            || info.hash.as_deref() != Some(&location.expected_hash)
        {
            return Err(changed.into());
        }
        let file = at::openat(
            &folder,
            leaf,
            at::OFlags::RDONLY | at::OFlags::NOFOLLOW | at::OFlags::NONBLOCK,
            at::Mode::empty(),
        )
        .map(fs::File::from)
        .map_err(|error| error.to_string())?;
        let opened = file.metadata().map_err(|error| error.to_string())?;
        if !opened.is_file() || opened.ino() != stat.st_ino || opened.dev() != stat.st_dev {
            return Err(changed.into());
        }
        let mut bytes = Vec::new();
        (&file)
            .take((rot_assets::MAX_LUA_SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > rot_assets::MAX_LUA_SOURCE_BYTES
            || format!("{:x}", Sha256::digest(&bytes)) != location.expected_hash
        {
            return Err(changed.into());
        }
        Some(opened)
    } else {
        None
    };
    let absolute = store.root().join(source);
    let command = launch
        .project_command(
            store.root(),
            &absolute,
            diagnostic.map_or(Some(1), |location| location.line),
        )
        .map_err(|error| error.to_string())?;
    if !active()
        || !fs::symlink_metadata(&absolute).is_ok_and(|current| {
            current.is_file()
                && current.ino() == stat.st_ino
                && current.dev() == stat.st_dev
                && verified.as_ref().is_none_or(|before| {
                    current.len() == before.len()
                        && current.modified().ok() == before.modified().ok()
                        && current.ctime() == before.ctime()
                        && current.ctime_nsec() == before.ctime_nsec()
                })
        })
    {
        return Err("The project or script path changed. Try again.".into());
    }
    Ok(command)
}

fn validate_leaf(name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', '\0']) {
        return Err("Enter a valid name.".into());
    }
    Ok(())
}

fn open_source_folder<'a>(root: &fs::File, source: &'a Path) -> Result<(OwnedFd, &'a str), String> {
    if !source.starts_with("Assets") {
        return Err("Move the source and sidecar into Assets first.".into());
    }
    AssetStore::project_source_limit(source.to_str().unwrap_or_default())
        .map_err(|error| error.to_string())?;
    let parent = source
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let folder = open_folder(root, parent).map_err(|error| error.to_string())?;
    let leaf = source
        .file_name()
        .and_then(|leaf| leaf.to_str())
        .ok_or("Invalid source file.")?;
    Ok((folder, leaf))
}

fn move_asset_pair(
    origin: &OwnedFd,
    old: &str,
    target: &OwnedFd,
    new: &str,
    active: impl Fn() -> bool,
) -> Result<(), String> {
    let original_sidecar = format!("{old}.rotmeta");
    let new_sidecar = format!("{new}.rotmeta");
    for name in [old, &original_sidecar] {
        let file = at::statat(origin, name, at::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| format!("{name}: {error}"))?;
        if at::FileType::from_raw_mode(file.st_mode) != at::FileType::RegularFile {
            return Err("Expected a regular source file and its import sidecar.".into());
        }
    }
    if !active() {
        return Err("Project closed before the asset was moved.".into());
    }
    let no_replace = at::RenameFlags::NOREPLACE;
    at::renameat_with(
        origin,
        original_sidecar.as_str(),
        target,
        new_sidecar.as_str(),
        no_replace,
    )
    .map_err(|error| error.to_string())?;
    if let Err(error) = at::renameat_with(origin, old, target, new, no_replace) {
        if let Err(rollback) = at::renameat_with(
            target,
            new_sidecar.as_str(),
            origin,
            original_sidecar.as_str(),
            no_replace,
        ) {
            return Err(format!(
                "Asset move failed ({error}). Couldn't restore its sidecar ({rollback})."
            ));
        }
        return Err(error.to_string());
    }
    Ok(())
}

fn copy_source(
    root: &Path,
    held_root: &fs::File,
    folder: &Path,
    path: &Path,
    duplicate: bool,
    active: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    if !folder.starts_with("Assets") {
        return Err("Choose a destination inside Assets.".into());
    }
    let bad_file = || "Choose a regular OBJ, PNG, WAV or GLB file.".to_owned();
    let leaf = path
        .file_name()
        .and_then(|leaf| leaf.to_str())
        .filter(|leaf| !leaf.starts_with('.'))
        .ok_or_else(bad_file)?;
    let relative = folder.join(leaf);
    let limit = AssetStore::project_source_limit(relative.to_str().ok_or_else(bad_file)?)
        .map_err(|error| error.to_string())?;
    let source = fs::canonicalize(path).map_err(|error| error.to_string())?;
    let flags =
        at::OFlags::RDONLY | at::OFlags::NONBLOCK | at::OFlags::NOFOLLOW | at::OFlags::CLOEXEC;
    let input: fs::File = if duplicate {
        at::openat2(
            held_root,
            path.strip_prefix(root).map_err(|_| bad_file())?,
            flags,
            at::Mode::empty(),
            at::ResolveFlags::BENEATH | at::ResolveFlags::NO_SYMLINKS,
        )
    } else {
        at::open(&source, flags, at::Mode::empty())
    }
    .map_err(|error| error.to_string())?
    .into();
    let original = input.metadata().map_err(|error| error.to_string())?;
    if !original.is_file() {
        return Err(bad_file());
    }
    if original.len() > limit as u64 {
        return Err(format!("File exceeds the {limit}-byte import limit."));
    }
    if let Some(source) = (!duplicate)
        .then(|| source.strip_prefix(root).ok())
        .flatten()
    {
        if source.starts_with("Assets")
            && source.components().all(|part| {
                part.as_os_str()
                    .to_str()
                    .is_some_and(|name| !name.starts_with('.'))
            })
        {
            AssetStore::project_source_limit(source.to_str().ok_or_else(bad_file)?)
                .map_err(|error| error.to_string())?;
            return Ok(source.to_owned());
        }
    }
    if !active() {
        return Err("Project closed during import.".into());
    }
    ensure_assets(held_root, folder)?;
    let open_destination = || open_folder(held_root, folder);
    let destination = open_destination().map_err(|error| error.to_string())?;
    let identity = at::fstat(&destination).map_err(|error| error.to_string())?;
    // Resolve both the scratch and final names through the held directory.
    // A replaced Assets path cannot redirect writes into another directory.
    let directory = PathBuf::from(format!("/proc/self/fd/{}", destination.as_raw_fd()));
    let mut scratch =
        tempfile::NamedTempFile::new_in(&directory).map_err(|error| error.to_string())?;
    let mut input = input.take(limit as u64 + 1);
    let copied = io::copy(&mut input, scratch.as_file_mut()).map_err(|error| error.to_string())?;
    let current = input
        .get_ref()
        .metadata()
        .map_err(|error| error.to_string())?;
    if copied > limit as u64 {
        return Err(format!("File exceeds the {limit}-byte import limit."));
    }
    if copied != original.len()
        || current.len() != original.len()
        || current.modified().ok() != original.modified().ok()
    {
        return Err("File changed during import. Try again.".into());
    }
    scratch
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    let (stem, extension) = leaf.rsplit_once('.').ok_or_else(bad_file)?;
    for attempt in 1.. {
        let name = if attempt == 1 {
            leaf.to_owned()
        } else {
            format!("{stem} {attempt}.{extension}")
        };
        let source = folder.join(&name);
        AssetStore::project_source_limit(source.to_str().ok_or_else(bad_file)?)
            .map_err(|error| error.to_string())?;
        if !active()
            || !open_destination()
                .and_then(at::fstat)
                .is_ok_and(|now| now.st_ino == identity.st_ino && now.st_dev == identity.st_dev)
        {
            return Err("The project or destination folder changed during import.".into());
        }
        // A sidecar with a missing source still owns this name and identity.
        match fs::symlink_metadata(directory.join(format!("{name}.rotmeta"))) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match scratch.persist_noclobber(directory.join(&name)) {
            Ok(_) => return Ok(source),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                scratch = error.file
            }
            Err(error) => return Err(error.error.to_string()),
        }
    }
    unreachable!()
}

struct WatchTree<'a> {
    fd: &'a OwnedFd,
    root: &'a Path,
    watch_root: i32,
    paths: BTreeMap<i32, PathBuf>,
}

const CHANGES: WatchFlags = WatchFlags::CLOSE_WRITE
    .union(WatchFlags::CREATE)
    .union(WatchFlags::DELETE)
    .union(WatchFlags::MOVE)
    .union(WatchFlags::DELETE_SELF)
    .union(WatchFlags::MOVE_SELF)
    .union(WatchFlags::DONT_FOLLOW)
    .union(WatchFlags::ONLYDIR);

impl<'a> WatchTree<'a> {
    fn new(fd: &'a OwnedFd, root: &'a Path) -> io::Result<Self> {
        let mut tree = Self {
            fd,
            root,
            watch_root: inotify::add_watch(fd, root, CHANGES)?,
            paths: BTreeMap::new(),
        };
        tree.rebuild()?;
        Ok(tree)
    }

    fn rebuild(&mut self) -> io::Result<()> {
        let mut next = BTreeMap::new();
        let mut directories = vec![PathBuf::from("Assets")];
        while let Some(relative) = directories.pop() {
            let dir = self.root.join(&relative);
            match inotify::add_watch(self.fd, &dir, CHANGES) {
                Ok(watch) => {
                    next.insert(watch, relative.clone());
                }
                Err(Errno::NOENT | Errno::NOTDIR | Errno::LOOP) => continue,
                Err(err) => return Err(err.into()),
            }
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => return Err(err),
            };
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir()
                    && !entry.file_name().to_string_lossy().starts_with('.')
                {
                    directories.push(relative.join(entry.file_name()));
                }
            }
        }
        for watch in self.paths.keys().filter(|watch| !next.contains_key(watch)) {
            let _ = inotify::remove_watch(self.fd, *watch);
        }
        self.paths = next;
        Ok(())
    }

    fn wait(&mut self, mut wake: &UnixStream) -> io::Result<Option<BTreeSet<PathBuf>>> {
        let mut changed = BTreeSet::new();
        let mut tree_changed = false;
        let mut deadline: Option<Instant> = None;
        let mut buffers = [MaybeUninit::uninit(); 8192];
        loop {
            let timeout = match deadline {
                None => -1,
                Some(deadline) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    remaining.as_millis().clamp(1, 35) as i32
                }
            };
            let mut fds = [
                PollFd::new(wake, PollFlags::IN),
                PollFd::new(self.fd, PollFlags::IN),
            ];
            match poll(&mut fds, timeout) {
                Ok(0) => break,
                Ok(_) if fds[0].revents().intersects(PollFlags::HUP | PollFlags::ERR) => {
                    return Ok(None);
                }
                Ok(_) if fds[0].revents().contains(PollFlags::IN) => match wake.read(&mut [0]) {
                    Ok(0) => return Ok(None),
                    Ok(_) => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                },
                Ok(_) => {}
                Err(Errno::INTR) => continue,
                Err(err) => return Err(err.into()),
            }
            if !fds[1].revents().contains(PollFlags::IN) {
                return Err(io::Error::other("The asset watcher stopped."));
            }
            let mut reader = inotify::Reader::new(self.fd, &mut buffers);
            loop {
                let event = match reader.next() {
                    Ok(event) => event,
                    Err(Errno::AGAIN) => break,
                    Err(Errno::INTR) => continue,
                    Err(err) => return Err(err.into()),
                };
                let flags = event.events();
                if event.wd() == self.watch_root
                    && flags.intersects(ReadFlags::DELETE_SELF | ReadFlags::MOVE_SELF)
                {
                    return Err(io::Error::other("The project folder moved or was removed."));
                }
                let name = event.file_name().and_then(|name| name.to_str().ok());
                if flags.contains(ReadFlags::QUEUE_OVERFLOW)
                    || event.wd() == self.watch_root && name == Some("Assets")
                {
                    tree_changed = true;
                    changed.insert(PathBuf::from("Assets"));
                    continue;
                }
                if event.wd() == self.watch_root && name == Some(".rot-assets.json") {
                    changed.insert(PathBuf::from(".rot-assets.json"));
                    continue;
                }
                let Some(directory) = self.paths.get(&event.wd()) else {
                    continue;
                };
                if flags.intersects(ReadFlags::DELETE_SELF | ReadFlags::MOVE_SELF) {
                    tree_changed = true;
                    changed.insert(directory.clone());
                } else if let Some(name) = name.filter(|name| !name.starts_with('.')) {
                    if flags.contains(ReadFlags::ISDIR) {
                        tree_changed = true;
                        changed.insert(directory.join(name));
                    } else {
                        let source = name.strip_suffix(".rotmeta").unwrap_or(name);
                        if source.ends_with(".obj")
                            || source.ends_with(".glb")
                            || source.ends_with(".png")
                            || source.ends_with(".wav")
                            || source.ends_with(".lua")
                            || source.ends_with(".prefab")
                        {
                            changed.insert(directory.join(source));
                        }
                    }
                }
            }
            if !changed.is_empty() {
                deadline.get_or_insert_with(|| Instant::now() + Duration::from_millis(100));
            }
        }
        if tree_changed {
            self.rebuild()?;
        }
        Ok(Some(changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn reimport_recovers_spot_from_same_bytes_without_auto_upgrading_catalog() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::create_dir(root.join("Assets")).unwrap();
        let source = "Assets/Lantern.glb";
        let bytes = rot_assets::fixtures::punctual_glb(|_| {});
        fs::write(root.join(source), &bytes).unwrap();

        let mut store = AssetStore::open(root).unwrap();
        let admitted = store.import_glb_at(0, "lantern", source).unwrap();
        let catalog = root.join(".rot-assets.json");
        let mut prior: serde_json::Value =
            serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
        let model = &mut prior["models"]["lantern"];
        model["nodes"]["Rig/Spot"]
            .as_object_mut()
            .unwrap()
            .remove("light");
        model["warnings"][2] =
            serde_json::json!("Light \"Edge Lamp\": Skipped spot light. Its node is kept.");
        fs::write(catalog, serde_json::to_vec(&prior).unwrap()).unwrap();

        let ctx = Context::default();
        let worker = ProjectAssets::start(root.to_owned(), &ctx).unwrap();
        let next = || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(update) = worker.take_update() {
                    break update.unwrap();
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let old = next();
        assert!(old.store.model("lantern").unwrap().nodes["Rig/Spot"]
            .light
            .is_none());
        let errors = worker
            .file_action(
                FileAction::Reimport(source.into()),
                old.store.state_identity(),
            )
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .errors;
        assert!(errors.is_empty(), "{errors:?}");
        let current = next();
        assert!(current.issues.is_empty(), "{:?}", current.issues);
        assert!(current.store.revision() > old.store.revision());
        assert_eq!(current.store.model("lantern"), Some(&admitted));
        assert_eq!(fs::read(root.join(source)).unwrap(), bytes);
    }

    #[test]
    fn creates_a_ready_project_script_without_clobber_and_passes_one_exact_file_arg() {
        use std::os::unix::fs::symlink;
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let folder = PathBuf::from("Assets/Code");
        let directory = root.join(&folder);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("Saved.lua"), "return {}").unwrap();
        fs::write(
            directory.join("Reserved.lua.rotmeta"),
            br#"{"id":"reserved"}"#,
        )
        .unwrap();
        let ctx = Context::default();
        let worker = ProjectAssets::start(root.to_owned(), &ctx).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let next = || loop {
            if let Some(update) = worker.take_update() {
                break update.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let first = next();
        let create = |name: &str, snapshot: &Snapshot| {
            worker
                .file_action(
                    FileAction::CreateScript {
                        parent: folder.clone(),
                        name: name.to_owned(),
                    },
                    snapshot.store.state_identity(),
                )
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
        };
        let saved = create("Saved", &first);
        assert!(saved.errors[0].contains("already exists"), "{saved:?}");
        assert_eq!(
            fs::read_to_string(directory.join("Saved.lua")).unwrap(),
            "return {}"
        );
        let reserved = create("Reserved.lua", &first);
        assert!(
            reserved.errors[0].contains("already has an import sidecar"),
            "{reserved:?}"
        );
        assert!(!directory.join("Reserved.lua").exists());

        let created = create("Track & Ráil.lua", &first);
        assert!(created.errors.is_empty(), "{created:?}");
        let (id, relative) = created.created_script.unwrap();
        assert_eq!(relative, folder.join("Track & Ráil.lua"));
        let snapshot = next();
        let info = snapshot.store.asset_info(&id).unwrap();
        assert_eq!(info.status, AssetStatus::Ready);
        assert_eq!(snapshot.store.script(&id), Some(OBJECT_LUA_TEMPLATE));
        assert_eq!(
            fs::read_to_string(root.join(&relative)).unwrap(),
            OBJECT_LUA_TEMPLATE
        );
        assert!(directory.join("Track & Ráil.lua.rotmeta").is_file());

        let held = fs::File::open(root).unwrap();
        let launch = LaunchSpec {
            executable: "example-editor".into(),
            args: vec!["--new".into(), "{workspace}".into(), "{file}:{line}".into()],
        };
        let command = project_script_command(
            &snapshot.store,
            &held,
            &id,
            &relative,
            &launch,
            None,
            || true,
        )
        .unwrap();
        let expected = format!("{}:1", root.join(&relative).display());
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["--new", root.to_str().unwrap(), expected.as_str()]);
        fs::rename(root.join(&relative), directory.join("backup.lua")).unwrap();
        symlink(directory.join("Saved.lua"), root.join(&relative)).unwrap();
        assert!(project_script_command(
            &snapshot.store,
            &held,
            &id,
            &relative,
            &launch,
            None,
            || true
        )
        .is_err());
    }

    #[test]
    fn restores_missing_lua_and_wav_when_the_worker_confirms_the_same_identity() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::create_dir(root.join("Assets")).unwrap();
        let wav = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x02\0\0\0\0\0";
        for (path, bytes) in [
            ("Assets/Signal.lua", b"return {}\r\n".as_slice()),
            ("Assets/Bell.wav", wav.as_slice()),
        ] {
            fs::write(root.join(path), bytes).unwrap();
        }
        let worker = ProjectAssets::start(root.to_owned(), &Context::default()).unwrap();
        let next = || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(update) = worker.take_update() {
                    break update.unwrap();
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let initial = next();
        let records: Vec<_> = initial.store.infos().cloned().collect();
        assert_eq!(records.len(), 2);
        for record in records {
            let source = root.join(&record.source);
            let original = fs::read(&source).unwrap();
            let sidecar = root.join(format!("{}.rotmeta", record.source));
            let original_sidecar = fs::read(&sidecar).unwrap();
            let sidecar_ino = fs::metadata(&sidecar).unwrap().ino();

            fs::remove_file(&source).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut snapshot = next();
            while snapshot.store.missing_source_restore_available(&record.id) != Some(true) {
                assert!(Instant::now() < deadline);
                snapshot = next();
            }
            let result = worker
                .file_action(
                    FileAction::RestoreMissingSource {
                        id: record.id.clone(),
                    },
                    snapshot.store.state_identity(),
                )
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            assert!(result.errors.is_empty(), "{:?}", result.errors);
            let accepted = next();
            let ready = accepted.store.asset_info(&record.id).unwrap();
            assert_eq!(ready.source, record.source);
            assert_eq!(ready.hash, record.hash);
            assert_eq!(ready.status, AssetStatus::Ready);
            assert_eq!(fs::read(&source).unwrap(), original);
            assert_eq!(fs::read(&sidecar).unwrap(), original_sidecar);
            assert_eq!(fs::metadata(&sidecar).unwrap().ino(), sidecar_ino);
        }
    }

    #[test]
    fn restore_rejects_altered_bytes_and_sidecar_and_never_replaces_a_competing_source() {
        use std::os::unix::fs::symlink;
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::create_dir_all(root.join("Assets/Code")).unwrap();
        let source = root.join("Assets/Code/signal.lua");
        let sidecar = root.join("Assets/Code/signal.lua.rotmeta");
        let original_sidecar = b"{\n  \"id\": \"signal\"\n}\n";
        fs::write(&sidecar, original_sidecar).unwrap();
        fs::write(&source, "return {}\n").unwrap();
        let mut store = AssetStore::open(root).unwrap();
        store.import("signal", "Assets/Code/signal.lua").unwrap();
        fs::remove_file(&source).unwrap();
        let store = AssetStore::open(root).unwrap();
        let held = fs::File::open(root).unwrap();
        let (cache, _) = store.missing_source_restore_cache("signal").unwrap();
        let cache = root.join(cache);
        let original_cache = fs::read(&cache).unwrap();
        fs::write(&cache, b"return []\n").unwrap();
        assert!(restore_missing_source(&store, &held, "signal", || true)
            .unwrap_err()
            .contains("SHA-256"));
        assert!(!source.exists());
        let reopened = AssetStore::open(root).unwrap();
        assert_eq!(
            reopened.missing_source_restore_available("signal"),
            Some(false)
        );
        fs::write(&cache, original_cache).unwrap();

        fs::write(&sidecar, br#"{"id":"unrelated"}"#).unwrap();
        assert!(restore_missing_source(&store, &held, "signal", || true)
            .unwrap_err()
            .contains("sidecar"));
        assert!(!source.exists());
        fs::write(&sidecar, original_sidecar).unwrap();

        let dir = root.join("Assets/Code");
        let moved = root.join("Assets/Old Code");
        fs::rename(&dir, &moved).unwrap();
        symlink(&moved, &dir).unwrap();
        assert!(restore_missing_source(&store, &held, "signal", || true).is_err());
        assert!(!moved.join("signal.lua").exists());
        fs::remove_file(&dir).unwrap();
        fs::rename(&moved, &dir).unwrap();

        assert!(restore_missing_source(&store, &held, "signal", || {
            fs::write(&source, "competing source").unwrap();
            true
        })
        .unwrap_err()
        .contains("already exists"));
        assert_eq!(fs::read(&source).unwrap(), b"competing source");
        assert_eq!(fs::read(&sidecar).unwrap(), original_sidecar);

        fs::remove_file(&source).unwrap();
        assert!(restore_missing_source(&store, &held, "signal", || {
            fs::rename(&sidecar, dir.join("old-sidecar")).unwrap();
            fs::write(&sidecar, original_sidecar).unwrap();
            true
        })
        .unwrap_err()
        .contains("sidecar changed"));
        assert!(!source.exists());
    }

    #[test]
    fn error_location_opens_as_one_argument_only_when_run_bytes_match() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let path = Path::new("Assets/Code & Rail.lua");
        fs::create_dir(root.join("Assets")).unwrap();
        fs::write(root.join(path), "return {}\n").unwrap();
        let mut store = AssetStore::open(root).unwrap();
        store.import("rail", path.to_str().unwrap()).unwrap();
        let held = fs::File::open(root).unwrap();
        let launch = LaunchSpec {
            executable: "example-editor".into(),
            args: vec!["--new".into(), "{file}:{line}".into()],
        };
        let location = ScriptLocation {
            expected_hash: store.asset_info("rail").unwrap().hash.clone().unwrap(),
            line: Some(8),
        };
        let resolve = |location| {
            project_script_command(&store, &held, "rail", path, &launch, Some(location), || {
                true
            })
        };
        let absolute = root.join(path);
        let line = format!("{}:8", absolute.display());
        assert_eq!(
            resolve(&location).unwrap().get_args().collect::<Vec<_>>(),
            ["--new", &line]
        );
        let file_only = ScriptLocation {
            expected_hash: location.expected_hash.clone(),
            line: None,
        };
        assert_eq!(
            resolve(&file_only).unwrap().get_args().collect::<Vec<_>>(),
            ["--new", absolute.to_str().unwrap()]
        );

        // The watcher is deliberately held at the original Ready generation.
        fs::write(&absolute, "return []\n").unwrap();
        assert!(resolve(&location)
            .unwrap_err()
            .contains("changed since this error"));
        fs::remove_file(&absolute).unwrap();
        assert!(resolve(&location).is_err());
    }

    #[test]
    fn move_to_folder_waits_for_exact_catalog_and_preserves_the_pair() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let source = PathBuf::from("Assets/Meshes/Profile.obj");
        let destination = PathBuf::from("Assets/Archive/Profile.obj");
        fs::create_dir_all(root.join(source.parent().unwrap())).unwrap();
        fs::create_dir_all(root.join(destination.parent().unwrap())).unwrap();
        let bytes = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        fs::write(root.join(&source), bytes).unwrap();
        let sidecar = source.with_file_name("Profile.obj.rotmeta");
        let settings = r#"{"id":"profile","mesh_normal_policy":"authored_when_complete"}"#;
        fs::write(root.join(&sidecar), settings).unwrap();
        let dest_sidecar = destination.with_file_name("Profile.obj.rotmeta");
        fs::write(root.join(&dest_sidecar), "orphan sidecar").unwrap();
        std::os::unix::fs::symlink(root.join("Assets/Archive"), root.join("Assets/Link")).unwrap();

        let ctx = Context::default();
        let worker = ProjectAssets::start(root.to_owned(), &ctx).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let snapshot = loop {
            if let Some(update) = worker.take_update() {
                break update.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let pinned = snapshot.store.asset_info("profile").unwrap().clone();
        let run = |folder: &Path| {
            worker
                .file_action(
                    FileAction::Move {
                        id: "profile".into(),
                        source: source.clone(),
                        folder: folder.to_owned(),
                    },
                    snapshot.store.state_identity(),
                )
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .errors
        };
        assert!(run(Path::new("Assets/Link"))[0].contains("without symlinks"));
        assert!(
            run(destination.parent().unwrap())[0].contains("already contains Profile.obj.rotmeta")
        );
        assert_eq!(fs::read(root.join(&source)).unwrap(), bytes);
        assert_eq!(fs::read_to_string(root.join(&sidecar)).unwrap(), settings);
        assert_eq!(
            fs::read_to_string(root.join(&dest_sidecar)).unwrap(),
            "orphan sidecar"
        );
        fs::remove_file(root.join(&dest_sidecar)).unwrap();

        let errors = run(destination.parent().unwrap());
        assert!(errors.is_empty(), "{errors:?}");
        let moved = worker.take_update().unwrap().unwrap();
        let info = moved.store.asset_info("profile").unwrap();
        assert_eq!(info.source, destination.to_str().unwrap());
        assert_eq!(info.status, AssetStatus::Ready);
        assert_eq!(info.hash, pinned.hash);
        assert_eq!(info.mesh_normal_policy, pinned.mesh_normal_policy);
        assert_eq!(fs::read(root.join(&destination)).unwrap(), bytes);
        assert_eq!(
            fs::read_to_string(root.join(&dest_sidecar)).unwrap(),
            settings
        );
        assert!(!root.join(&source).exists());
        assert!(!root.join(&sidecar).exists());
        let stale = run(Path::new("Assets"));
        assert!(stale[0].contains("catalog changed"));
    }

    #[test]
    fn source_actions_preserve_identity_settings_and_recoverable_files() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let assets = root.join("Assets");
        fs::create_dir(&assets).unwrap();
        let source = assets.join("Prop.obj");
        let old = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        fs::write(&source, old).unwrap();
        let sidecar = assets.join("Prop.obj.rotmeta");
        let settings = r#"{"imports":[{"id":"flat","mesh_normal_policy":"flat"},{"id":"smooth","mesh_normal_policy":"authored_when_complete"}]}"#;
        fs::write(&sidecar, settings).unwrap();
        fs::write(assets.join("Taken.obj"), old).unwrap();
        let ctx = Context::default();
        let worker = ProjectAssets::start(root.to_owned(), &ctx).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut snapshot = loop {
            if let Some(update) = worker.take_update() {
                break update.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let run = |snapshot: &Snapshot, action| {
            worker
                .file_action(action, snapshot.store.state_identity())
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .errors
        };
        let command = |snapshot: &mut Snapshot, action| {
            let errors = run(snapshot, action);
            assert!(errors.is_empty(), "{errors:?}");
            *snapshot = worker.take_update().unwrap().unwrap();
        };
        command(
            &mut snapshot,
            FileAction::CreateFolder {
                parent: "Assets".into(),
                name: "Empty".into(),
            },
        );
        assert!(snapshot.folders.contains(&PathBuf::from("Assets/Empty")));

        let original = PathBuf::from("Assets/Prop.obj");
        command(&mut snapshot, FileAction::Duplicate(original.clone()));
        let copies: Vec<_> = snapshot
            .store
            .infos()
            .filter(|asset| asset.source == "Assets/Prop 2.obj")
            .collect();
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|copy| copy.id.starts_with("asset-")));
        assert_ne!(copies[0].mesh_normal_policy, copies[1].mesh_normal_policy);
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), settings);

        let failures = run(
            &snapshot,
            FileAction::Rename {
                source: original.clone(),
                stem: "Taken".into(),
            },
        );
        assert_eq!(failures.len(), 1);
        assert_eq!(fs::read(&source).unwrap(), old);
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), settings);

        command(
            &mut snapshot,
            FileAction::Rename {
                source: original,
                stem: "Renamed".into(),
            },
        );
        let renamed = PathBuf::from("Assets/Renamed.obj");
        assert!(!source.exists());
        assert!(!sidecar.exists());
        for id in ["flat", "smooth"] {
            assert_eq!(
                snapshot.store.asset_info(id).unwrap().source,
                "Assets/Renamed.obj"
            );
        }
        command(&mut snapshot, FileAction::Trash(renamed.clone()));
        assert!(!root.join(&renamed).exists());
        assert_eq!(
            snapshot.store.asset_info("flat").unwrap().status,
            rot_assets::AssetStatus::Missing
        );
        assert!(snapshot.store.mesh("flat").is_some());
        let trash = root
            .join(".rot-trash")
            .read_dir()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let trashed_source = trash.join("Renamed.obj");
        let trashed_sidecar = trash.join("Renamed.obj.rotmeta");
        assert_eq!(fs::read(&trashed_source).unwrap(), old);
        assert_eq!(fs::read_to_string(&trashed_sidecar).unwrap(), settings);

        fs::rename(trashed_source, root.join(&renamed)).unwrap();
        fs::rename(trashed_sidecar, assets.join("Renamed.obj.rotmeta")).unwrap();
        command(&mut snapshot, FileAction::Reimport(renamed));
        assert_eq!(
            snapshot.store.asset_info("flat").unwrap().status,
            rot_assets::AssetStatus::Ready
        );
    }

    #[test]
    fn dropped_sources_share_reconciliation_and_preserve_existing_files() {
        let project = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = project.path();
        let input = external.path().join("Crate.obj");
        let geometry = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";
        fs::write(&input, geometry).unwrap();
        let unsupported = external.path().join("notes.txt");
        fs::write(&unsupported, "Leave this outside the project.").unwrap();
        let oversized = external.path().join("Oversized.obj");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(AssetStore::project_source_limit("Assets/Oversized.obj").unwrap() as u64 + 1)
            .unwrap();

        let folder = PathBuf::from("Assets/Props");
        let destination = root.join(&folder);
        fs::create_dir_all(&destination).unwrap();
        let original = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        fs::write(destination.join("Crate.obj"), original).unwrap();
        // A missing source still has an identity owned by this sidecar.
        fs::write(
            destination.join("Crate 2.obj.rotmeta"),
            r#"{"id":"reserved"}"#,
        )
        .unwrap();
        let escape = root.join("Assets/Link");
        symlink(external.path(), &escape).unwrap();
        let ctx = Context::default();
        let worker = ProjectAssets::start(root.to_owned(), &ctx).unwrap();
        let result = worker
            .import_files(folder.clone(), vec![input.clone(), unsupported, oversized])
            .unwrap();
        let failures = result.recv_timeout(Duration::from_secs(5)).unwrap().errors;
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert!(failures[0].contains("Rot imports OBJ"));
        assert!(failures[1].contains("import limit"));
        let snapshot = worker.take_update().unwrap().unwrap();
        let placed = snapshot
            .store
            .infos()
            .find(|info| info.source == "Assets/Props/Crate 3.obj")
            .unwrap();
        assert_eq!(
            snapshot.store.mesh(&placed.id).unwrap().vertices[1].position[0],
            2.0
        );
        assert_eq!(
            fs::read_to_string(destination.join("Crate.obj")).unwrap(),
            original
        );
        assert_eq!(
            fs::read_to_string(destination.join("Crate 3.obj")).unwrap(),
            geometry
        );
        assert_eq!(fs::read_to_string(input).unwrap(), geometry);
        assert!(!destination.join("Crate 2.obj").exists());
        assert!(!destination.join("notes.txt").exists());
        assert!(!destination.join("Oversized.obj").exists());
        let existing = destination.join("Crate 3.obj");
        let result = worker.import_files(folder, vec![existing]).unwrap();
        assert!(result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .errors
            .is_empty());
        assert!(!destination.join("Crate 3 2.obj").exists());

        let another = external.path().join("Another.obj");
        fs::write(&another, geometry).unwrap();
        let result = worker
            .import_files(PathBuf::from("Assets/Link"), vec![another])
            .unwrap();
        assert_eq!(
            result
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .errors
                .len(),
            1
        );
        assert!(!external.path().join("Another 2.obj").exists());
    }
}
