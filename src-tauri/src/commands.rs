use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::ae;
use crate::archive;
use crate::error::{AppError, AppResult};
use crate::importer;
use crate::installer;
use crate::library::LibraryStore;
use crate::models::{
    AeInstallation, Bundle, InstallRecord, Library, Manifest, Package, PackageKind,
};
use crate::{activity, maintenance, preview};

/// Resolve the Pluxel data directory and open a [`LibraryStore`] on it.
fn store(app: &AppHandle) -> AppResult<LibraryStore> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::msg(format!("Cannot resolve app data dir: {e}")))?;
    LibraryStore::new(base.join("Pluxel"))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

static LIBRARY_OPERATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn operation_guard() -> AppResult<std::sync::MutexGuard<'static, ()>> {
    LIBRARY_OPERATION
        .lock()
        .map_err(|_| AppError::msg("ライブラリ処理のロックを取得できません"))
}

/// Hashing, copying and recovery may take time for large plugins. Keep them
/// off the window's event loop and serialize the full library transaction.
async fn library_task<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&LibraryStore) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = operation_guard()?;
        action(&store(&app)?)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn take_operation_warnings() -> Vec<String> {
    crate::warnings::take()
}

fn log_action(store: &LibraryStore, action: &str, pkg: Option<&Package>, detail: &str) {
    if activity::append(store, action, pkg, detail).is_err() {
        crate::warnings::push("maintenance.warningHistory");
    }
}

#[tauri::command]
pub async fn prepare_import(
    app: AppHandle,
    paths: Vec<String>,
) -> AppResult<preview::PreparedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = operation_guard()?;
        preview::prepare(&store(&app)?, &paths)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn commit_import(app: AppHandle, token: String) -> AppResult<Vec<Package>> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = operation_guard()?;
        let store = store(&app)?;
        commit_prepared(&store, &token)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

pub(crate) fn commit_prepared(store: &LibraryStore, token: &str) -> AppResult<Vec<Package>> {
    let packages = preview::commit(store, token)?;
    for pkg in &packages {
        log_action(store, "import", Some(pkg), "");
    }
    Ok(packages)
}

#[tauri::command]
pub async fn discard_import(app: AppHandle, id: String, replacement_id: String) -> AppResult<()> {
    library_task(app, move |store| {
        discard_duplicate(store, &id, &replacement_id)
    })
    .await
}

pub(crate) fn discard_duplicate(
    store: &LibraryStore,
    id: &str,
    replacement_id: &str,
) -> AppResult<()> {
    if id == replacement_id {
        return Err(AppError::msg("同じパッケージは指定できません"));
    }
    let source = store.get(id)?;
    let _existing = store.get(replacement_id)?;
    if !source.installs.is_empty() {
        return Err(AppError::msg("インストール済みパッケージは破棄できません"));
    }
    let mut library = store.load()?;
    library.packages.retain(|p| p.id != id);
    for bundle in &mut library.bundles {
        for member in &mut bundle.package_ids {
            if *member == id {
                *member = replacement_id.to_string();
            }
        }
        let mut seen = std::collections::HashSet::new();
        bundle.package_ids.retain(|id| seen.insert(id.clone()));
    }
    store.save(&library)?;
    if std::fs::remove_dir_all(store.package_dir(id)).is_err() {
        crate::warnings::push("maintenance.warningCleanup");
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_import(app: AppHandle, token: String) -> AppResult<()> {
    let _guard = operation_guard()?;
    preview::cancel(&store(&app)?, &token)
}

#[tauri::command]
pub async fn check_integrity(
    app: AppHandle,
    id: Option<String>,
) -> AppResult<Vec<maintenance::IntegrityEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = operation_guard()?;
        maintenance::check(&store(&app)?, id.as_deref())
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn get_history(app: AppHandle) -> AppResult<Vec<activity::ActivityEvent>> {
    activity::history(&store(&app)?)
}

#[tauri::command]
pub async fn get_revisions(app: AppHandle, id: String) -> AppResult<Vec<activity::Revision>> {
    library_task(app, move |store| activity::revisions(store, &id)).await
}

#[tauri::command]
pub async fn restore_revision(
    app: AppHandle,
    id: String,
    revision_id: String,
) -> AppResult<Package> {
    library_task(app, move |store| {
        let pkg = activity::restore(store, &id, &revision_id)?;
        log_action(store, "restore", Some(&pkg), &revision_id);
        Ok(pkg)
    })
    .await
}

#[tauri::command]
pub async fn reinstall_package(
    app: AppHandle,
    id: String,
    params: InstallParams,
) -> AppResult<Package> {
    library_task(app, move |store| {
        let pkg = store.get(&id)?;
        if !pkg.kind.bundleable() {
            return Err(AppError::msg(
                "この形式の再インストールには対応していません",
            ));
        }
        let mut requests = Vec::new();
        for rec in &pkg.installs {
            if rec.paths.is_empty() {
                continue;
            }
            let effect_subdir = if rec.kind == PackageKind::Plugin {
                rec.ae
                    .as_ref()
                    .and_then(|ae| {
                        Path::new(&rec.target_dir)
                            .strip_prefix(&ae.plugins_dir)
                            .ok()
                    })
                    .filter(|rel| !rel.as_os_str().is_empty())
                    .map(|rel| rel.to_string_lossy().to_string())
            } else {
                None
            };
            let one = resolve_install_params(InstallParams {
                installation: rec.ae.clone(),
                kind: Some(rec.kind),
                custom_dir: rec.ae.is_none().then(|| rec.target_dir.clone()),
                label: Some(rec.label.clone()),
                elevated: params.elevated,
                effect_subdir,
                allow_overwrite: params.allow_overwrite,
            })?;
            requests.push((id.clone(), one));
        }
        crate::deployment::apply(store, &requests)?;
        store.get(&id)
    })
    .await
}

/// Copy `src` into `dir` as `banner.<ext>`, returning the stored filename.
fn write_banner_file(dir: &Path, src: &Path) -> AppResult<String> {
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "png".into());
    let banner_name = format!("banner.{ext}");
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Err(AppError::msg("対応していないバナー画像形式です"));
    }
    if std::fs::metadata(src)?.len() > MAX_BANNER_BYTES {
        return Err(AppError::msg("バナー画像は32 MiB以下にしてください"));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::copy(src, dir.join(&banner_name))?;
    Ok(banner_name)
}

/// Best-effort delete of a stored banner file.
fn remove_banner_file(dir: &Path, name: &str) {
    if !crate::library::is_safe_segment(name) {
        return;
    }
    let path = dir.join(name);
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// Read a stored banner as a `data:` URL, or `None` if the file is missing.
fn banner_data_url(dir: &Path, name: &str) -> AppResult<Option<String>> {
    // Defence in depth: never join a banner name that isn't a plain filename
    // (import already sanitizes, but a stale/tampered manifest must not read out
    // of the package directory).
    if !crate::library::is_safe_segment(name) {
        return Ok(None);
    }
    let path = dir.join(name);
    if !path.exists() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(MAX_BANNER_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BANNER_BYTES {
        return Err(AppError::msg("バナー画像は32 MiB以下にしてください"));
    }
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

#[tauri::command]
pub fn get_library(app: AppHandle) -> AppResult<Library> {
    let _guard = operation_guard()?;
    store(&app)?.load()
}

/// The absolute path of the Pluxel data directory (for display / reveal).
#[tauri::command]
pub fn get_data_dir(app: AppHandle) -> AppResult<String> {
    Ok(store(&app)?.root().to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_paths(app: AppHandle, paths: Vec<String>) -> AppResult<Vec<Package>> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut imported = Vec::new();
    for p in paths {
        imported.push(importer::import_path(&store, Path::new(&p))?);
    }
    Ok(imported)
}

/// Attach companion files/folders (e.g. `.ffx`, image assets) to a package.
#[tauri::command]
pub async fn add_files(app: AppHandle, id: String, paths: Vec<String>) -> AppResult<Package> {
    library_task(app, move |store| add_companions(store, &id, &paths)).await
}

pub(crate) fn add_companions(
    store: &LibraryStore,
    id: &str,
    paths: &[String],
) -> AppResult<Package> {
    let mut pkg = store.get(id)?;
    let mut names = std::collections::HashSet::new();
    for path in paths {
        let path = Path::new(path);
        crate::deployment::validate_tree(path)?;
        let canonical = path.canonicalize()?;
        if store.root().canonicalize()?.starts_with(&canonical) {
            return Err(AppError::msg("保管庫自身を同梱することはできません"));
        }
        let name = path
            .file_name()
            .ok_or_else(|| AppError::msg("ファイル名が不正です"))?;
        if store.files_dir(id).join(name).symlink_metadata().is_ok()
            || !names.insert(name.to_string_lossy().to_lowercase())
        {
            return Err(AppError::msg("同名のファイル・フォルダがあります。既存の同梱ファイルを削除するか、本体の差し替えを使用してください。"));
        }
    }
    if paths.is_empty() {
        return Ok(pkg);
    }
    let revision = activity::snapshot(store, &pkg, "add_files")?;
    let result = (|| -> AppResult<()> {
        for path in paths {
            importer::add_source(store, id, Path::new(path))?;
        }
        importer::rescan(store, &mut pkg)?;
        store.upsert(pkg.clone())
    })();
    if let Err(error) = result {
        activity::rollback(store, id, &revision.id)?;
        return Err(error);
    }
    log_action(store, "addFiles", Some(&pkg), &paths.len().to_string());
    Ok(pkg)
}

/// Replace all of a package's stored files with new sources (a version bump).
#[tauri::command]
pub async fn replace_files(app: AppHandle, id: String, paths: Vec<String>) -> AppResult<Package> {
    library_task(app, move |store| {
        let mut pkg = store.get(&id)?;
        let srcs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let revision = activity::snapshot(store, &pkg, "replace_files")?;
        let result =
            importer::replace_files(store, &mut pkg, &srcs).and_then(|_| store.upsert(pkg.clone()));
        if let Err(e) = result {
            activity::rollback(store, &id, &revision.id)?;
            return Err(e);
        }
        log_action(store, "update", Some(&pkg), "");
        Ok(pkg)
    })
    .await
}

#[tauri::command]
pub async fn remove_package(app: AppHandle, id: String) -> AppResult<()> {
    library_task(app, move |store| {
        let pkg = store.get(&id)?;
        store.remove(&id)?;
        log_action(store, "delete", Some(&pkg), "");
        Ok(())
    })
    .await
}

/// Metadata for a replace: adopt the source manifest when it carries its own
/// metadata (a `.ppf` package), otherwise keep the target's (a plain asset file
/// has no manifest, so version/author/etc. must not be wiped). User tags are
/// always kept from the target.
pub(crate) fn merged_replace_manifest(old: Manifest, src: Manifest) -> Manifest {
    let src_has_metadata = !(src.version.trim().is_empty()
        && src.author.trim().is_empty()
        && src.description.trim().is_empty()
        && src.homepage.trim().is_empty());
    if src_has_metadata {
        Manifest {
            tags: old.tags,
            ..src
        }
    } else {
        // Refresh the display name from the new file, keep everything else.
        let name = if src.name.trim().is_empty() {
            old.name.clone()
        } else {
            src.name.clone()
        };
        Manifest { name, ..old }
    }
}

/// Update an existing package (`target_id`) in place with the content of a
/// freshly-imported one (`source_id`), then discard the source. The target
/// keeps its id, user tags and install records; its files and kind come from
/// the source. Metadata depends on the source: a package (`.ppf`) carries its
/// own manifest, which is adopted; a plain asset file has none, so the target's
/// existing version/author/description/etc. are preserved. The banner is
/// updated only if the source has one.
#[tauri::command]
pub async fn replace_package(
    app: AppHandle,
    target_id: String,
    source_id: String,
) -> AppResult<Package> {
    library_task(app, move |store| {
        replace_existing(store, &target_id, &source_id)
    })
    .await
}

pub(crate) fn replace_existing(
    store: &LibraryStore,
    target_id: &str,
    source_id: &str,
) -> AppResult<Package> {
    let mut target = store.get(&target_id)?;
    let source = store.get(&source_id)?;
    if !source.installs.is_empty() {
        return Err(AppError::msg(
            "インストール記録のあるパッケージは差し替え元に使用できません",
        ));
    }
    if target_id == source_id {
        return Err(AppError::msg("同じパッケージで置き換えることはできません"));
    }
    let revision = activity::snapshot(store, &target, "replace_package")?;
    let result = (|| -> AppResult<Package> {
        // Physically replace the target's files with the source's.
        let tfiles = store.files_dir(&target_id);
        if tfiles.exists() {
            std::fs::remove_dir_all(&tfiles)?;
        }
        importer::copy_recursively(&store.files_dir(&source_id), &tfiles)?;

        target.manifest = merged_replace_manifest(target.manifest.clone(), source.manifest.clone());
        target.kind = source.kind;
        target.files = source.files.clone();

        // Copy the source's banner over, if any.
        if let Some(b) = &source.banner {
            if !crate::library::is_safe_segment(b) {
                return Err(AppError::msg("バナーのファイル名が不正です"));
            }
            let sp = store.package_dir(&source_id).join(b);
            if sp.exists() {
                std::fs::copy(&sp, store.package_dir(&target_id).join(b))?;
                target.banner = Some(b.clone());
            }
        }
        target.updated_at = now();

        store.write_manifest(&target_id, &target.manifest)?;
        let mut library = store.load()?;
        let entry = library
            .packages
            .iter_mut()
            .find(|p| p.id == target_id)
            .ok_or_else(|| AppError::msg("パッケージが見つかりません"))?;
        *entry = target.clone();
        library.packages.retain(|p| p.id != source_id);
        for bundle in &mut library.bundles {
            for id in &mut bundle.package_ids {
                if *id == source_id {
                    *id = target_id.to_string();
                }
            }
            let mut seen = std::collections::HashSet::new();
            bundle.package_ids.retain(|id| seen.insert(id.clone()));
        }
        store.save(&library)?;
        Ok(target)
    })();
    match result {
        Ok(pkg) => {
            if std::fs::remove_dir_all(store.package_dir(&source_id)).is_err() {
                crate::warnings::push("maintenance.warningCleanup");
            }
            log_action(store, "update", Some(&pkg), "");
            Ok(pkg)
        }
        Err(e) => {
            activity::rollback(store, &target_id, &revision.id)?;
            Err(e)
        }
    }
}

/// Remove a single stored file (or a top-level folder/bundle) from a package.
/// `rel_path` is relative to the package `files/` directory.
#[tauri::command]
pub async fn remove_file(app: AppHandle, id: String, rel_path: String) -> AppResult<Package> {
    library_task(app, move |store| remove_companion(store, &id, &rel_path)).await
}

pub(crate) fn remove_companion(
    store: &LibraryStore,
    id: &str,
    rel_path: &str,
) -> AppResult<Package> {
    if rel_path.is_empty()
        || rel_path.contains('\\')
        || rel_path
            .split('/')
            .any(|part| !crate::library::is_safe_segment(part))
    {
        return Err(AppError::msg("不正なパスです"));
    }
    let mut pkg = store.get(id)?;
    if crate::assets::protects(&pkg, rel_path) {
        return Err(AppError::msg(
            "本体、または本体を含むフォルダは削除できません",
        ));
    }
    let files = store.files_dir(id).canonicalize()?;
    let target = files.join(rel_path);
    let resolved = target.canonicalize()?;
    if resolved == files || !resolved.starts_with(&files) {
        return Err(AppError::msg("不正なパスです"));
    }
    crate::deployment::validate_target(&target)?;
    let revision = activity::snapshot(store, &pkg, "remove_file")?;
    let result = (|| -> AppResult<()> {
        if target.is_dir() {
            std::fs::remove_dir_all(&target)?;
        } else {
            std::fs::remove_file(&target)?;
        }
        importer::rescan(store, &mut pkg)?;
        store.upsert(pkg.clone())
    })();
    if let Err(error) = result {
        activity::rollback(store, id, &revision.id)?;
        return Err(error);
    }
    log_action(store, "removeFile", Some(&pkg), rel_path);
    Ok(pkg)
}

#[tauri::command]
pub fn update_manifest(app: AppHandle, id: String, manifest: Manifest) -> AppResult<Package> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    if pkg.manifest.version != manifest.version {
        activity::snapshot(&store, &pkg, "metadata_version")?;
    }
    if let Some(kind) = manifest.install_as {
        pkg.kind = kind;
    }
    pkg.manifest = manifest.clone();
    pkg.updated_at = now();
    store.write_manifest(&id, &manifest)?;
    store.upsert(pkg.clone())?;
    log_action(&store, "update", Some(&pkg), "metadata");
    Ok(pkg)
}

/// Replace a package banner with the image at `source_path`.
#[tauri::command]
pub fn set_banner(app: AppHandle, id: String, source_path: String) -> AppResult<Package> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    let banner_name = write_banner_file(&store.package_dir(&id), &PathBuf::from(&source_path))?;
    pkg.banner = Some(banner_name);
    pkg.updated_at = now();
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Replace a package banner with the image currently on the clipboard.
#[tauri::command]
pub fn set_banner_clipboard(app: AppHandle, id: String) -> AppResult<Package> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    let banner_name = write_banner_from_clipboard(&store.package_dir(&id))?;
    pkg.banner = Some(banner_name);
    pkg.updated_at = now();
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Read the clipboard image and write it to `dir` as `banner.png`. Errors with a
/// user-facing message when the clipboard holds no image.
fn write_banner_from_clipboard(dir: &Path) -> AppResult<String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| AppError::msg(format!("クリップボードを開けません: {e}")))?;
    let img = clipboard
        .get_image()
        .map_err(|_| AppError::msg("クリップボードに画像がありません"))?;
    write_clipboard_image(dir, img.width, img.height, &img.bytes)?;
    Ok("banner.png".to_string())
}

const MAX_BANNER_BYTES: u64 = 32 * 1024 * 1024;

/// Reject oversized/invalid RGBA data before encoding. Commit only a complete,
/// flushed image, so errors leave the previous banner intact.
pub(crate) fn write_clipboard_image(
    dir: &Path,
    width: usize,
    height: usize,
    bytes: &[u8],
) -> AppResult<()> {
    let pixels = width.checked_mul(height).filter(|n| *n <= 8 * 1024 * 1024);
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || pixels.and_then(|n| n.checked_mul(4)) != Some(bytes.len())
    {
        return Err(AppError::msg(
            "バナー画像のサイズが不正、または大きすぎます（最大8192px・8メガピクセル）",
        ));
    }
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".banner-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> AppResult<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let mut output = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(&mut output, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| AppError::msg(e.to_string()))?;
        writer
            .write_image_data(bytes)
            .map_err(|e| AppError::msg(e.to_string()))?;
        writer.finish().map_err(|e| AppError::msg(e.to_string()))?;
        output.flush()?;
        drop(output);
        if std::fs::metadata(&temp)?.len() > MAX_BANNER_BYTES {
            return Err(AppError::msg("バナー画像は32 MiB以下にしてください"));
        }
        std::fs::rename(&temp, dir.join("banner.png"))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&temp);
    result
}

/// Remove a package's banner image (delete the file and clear the reference).
#[tauri::command]
pub fn clear_banner(app: AppHandle, id: String) -> AppResult<Package> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    if let Some(banner) = pkg.banner.take() {
        remove_banner_file(&store.package_dir(&id), &banner);
    }
    pkg.updated_at = now();
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Return a package banner as a `data:` URL, or `None` if it has none.
#[tauri::command]
pub fn read_banner(app: AppHandle, id: String) -> AppResult<Option<String>> {
    let store = store(&app)?;
    let pkg = store.get(&id)?;
    let Some(banner) = pkg.banner else {
        return Ok(None);
    };
    banner_data_url(&store.package_dir(&id), &banner)
}

#[tauri::command]
pub fn detect_ae() -> AppResult<Vec<AeInstallation>> {
    Ok(ae::detect_installations())
}

/// Launch a package's native installer (`.pkg` / `.exe` / `.msi`) with the OS,
/// rather than copying it into an After Effects folder.
#[tauri::command]
pub fn run_installer(app: AppHandle, id: String) -> AppResult<Package> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut pkg = store.get(&id)?; // constrain `id` to a real stored package (UUID)
    let files_dir = store.files_dir(&id);

    // Find the shallowest installer file inside the package.
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in walkdir::WalkDir::new(&files_dir).min_depth(1) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let is_installer = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| {
                let ext = e.to_ascii_lowercase();
                #[cfg(target_os = "macos")]
                {
                    ext == "pkg"
                }
                #[cfg(target_os = "windows")]
                {
                    matches!(ext.as_str(), "exe" | "msi")
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    PackageKind::from_extension(&ext) == PackageKind::Installer
                }
            })
            .unwrap_or(false);
        if is_installer {
            let depth = entry.depth();
            if best.as_ref().map(|(d, _)| depth < *d).unwrap_or(true) {
                best = Some((depth, entry.path().to_path_buf()));
            }
        }
    }

    let installer = best
        .map(|(_, p)| p)
        .ok_or_else(|| AppError::msg("このOSに対応するインストーラーが見つかりません"))?;

    launch(&installer)?;

    // Launching a native installer is a black box (we can't verify what it did),
    // so once the user runs it we simply mark the package as installed. The
    // record has no paths — "uninstalling" it just clears the mark. Only one
    // installer record is kept.
    pkg.installs.retain(|r| r.kind != PackageKind::Installer);
    pkg.installs.push(InstallRecord {
        id: uuid::Uuid::new_v4().to_string(),
        label: installer
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "installer".to_string()),
        kind: PackageKind::Installer,
        target_dir: "installer".to_string(),
        ae: None,
        paths: Vec::new(),
        installed_at: now(),
    });
    store.upsert(pkg.clone())?;
    log_action(&store, "runInstaller", Some(&pkg), "");
    Ok(pkg)
}

/// Open a file with the OS default handler (launches installers).
fn launch(path: &Path) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open").arg(path).status()?;
        if !status.success() {
            return Err(AppError::msg("インストーラーを起動できませんでした"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Invoke the shell default action without going through `cmd /C start`, whose
        // re-parsing lets a crafted file name inject commands.
        use std::os::windows::ffi::OsStrExt;
        let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err(AppError::msg(format!(
                "インストーラーを起動できませんでした (OS code {})",
                result as isize
            )));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Reveal a file or folder in the OS file manager (Finder / Explorer),
/// selecting the item when it is a file. Used to show where a package was
/// installed on disk.
#[tauri::command]
pub fn reveal_path(path: String) -> AppResult<()> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(AppError::msg(format!("path not found: {path}")));
    }
    #[cfg(target_os = "macos")]
    {
        // `-R` reveals (selects) the item in Finder.
        std::process::Command::new("open")
            .arg("-R")
            .arg(p)
            .spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // `/select,` opens Explorer with the item highlighted. The flag must stay
        // unquoted while only the path is quoted — if std quotes the whole
        // "/select,<path>" token (which it does when the path has spaces),
        // Explorer ignores the flag and opens the default folder (Documents).
        // Build the command line raw and normalise separators to backslashes.
        let win = p.display().to_string().replace('/', "\\");
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{win}\""))
            .spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // No portable "reveal and select"; open the containing folder instead.
        let dir = p.parent().unwrap_or(p);
        std::process::Command::new("xdg-open").arg(dir).spawn()?;
    }
    Ok(())
}

/// State of the admin (elevated-install) session.
#[derive(serde::Serialize)]
pub struct AdminStatus {
    /// Whether session-based admin auth applies on this OS (macOS only; Windows
    /// uses a per-operation UAC consent that can't be cached).
    pub supported: bool,
    /// Whether an admin password is currently cached for this session.
    pub unlocked: bool,
}

/// Report whether the elevated-install session is unlocked.
#[tauri::command]
pub fn admin_status() -> AdminStatus {
    AdminStatus {
        supported: cfg!(target_os = "macos"),
        unlocked: installer::has_session_password(),
    }
}

/// Validate an admin password and cache it for the rest of this app session, so
/// subsequent elevated installs don't prompt again. macOS only.
#[tauri::command]
pub fn admin_unlock(password: String) -> AppResult<()> {
    installer::unlock_session(&password)
}

/// Forget the cached admin password (e.g. on user request).
#[tauri::command]
pub fn admin_lock() -> AppResult<()> {
    installer::set_session_password(None);
    crate::helper::lock_session()
}

#[tauri::command]
pub async fn helper_status() -> AppResult<crate::helper::HelperStatus> {
    tauri::async_runtime::spawn_blocking(crate::helper::status)
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn helper_register(app: AppHandle) -> AppResult<crate::helper::HelperStatus> {
    library_task(app, |_| crate::helper::configure(true)).await
}

#[tauri::command]
pub async fn helper_unregister(app: AppHandle) -> AppResult<crate::helper::HelperStatus> {
    library_task(app, |_| crate::helper::configure(false)).await
}

#[tauri::command]
pub async fn helper_authenticate() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(crate::helper::authenticate)
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstallParams {
    /// A detected AE installation (used with `kind` when `custom_dir` is unset).
    pub installation: Option<AeInstallation>,
    /// The target kind (Scripts / ScriptUI Panels / Plug-ins / CEP).
    pub kind: Option<PackageKind>,
    /// A user-chosen folder that overrides the AE-derived location entirely.
    pub custom_dir: Option<String>,
    /// Explicit label (used when reinstalling/updating an existing record).
    pub label: Option<String>,
    /// Perform the install with elevated privileges (password / UAC).
    pub elevated: Option<bool>,
    /// Subfolder under Plug-ins to nest effect plugins into (e.g. "Effects").
    pub effect_subdir: Option<String>,
    #[serde(default)]
    pub allow_overwrite: bool,
}

/// Compute final paths without writing/extracting anything.
pub(crate) fn planned_paths(
    store: &LibraryStore,
    pkg: &Package,
    params: &InstallParams,
) -> AppResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for src in installer::installable_entries(&store.files_dir(&pkg.id))? {
        let target = if let Some(custom) = params
            .custom_dir
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            PathBuf::from(custom)
        } else {
            let ae = params
                .installation
                .as_ref()
                .ok_or_else(|| AppError::msg("インストール先が指定されていません"))?;
            installer::route(
                ae,
                installer::entry_kind(&src),
                params.kind.unwrap_or(pkg.kind),
                params.effect_subdir.as_deref(),
            )
        };
        let name = if installer::entry_kind(&src) == PackageKind::Zxp {
            src.file_stem()
        } else {
            src.file_name()
        };
        paths.push(target.join(name.ok_or_else(|| AppError::msg("ファイル名が不正です"))?));
    }
    Ok(paths)
}

pub(crate) use crate::paths::reject_privileged_dir;

/// Validate and canonicalize install params once, before any package is
/// installed. An AE installation must match a currently-detected one, and we
/// substitute the DETECTED struct so a tampered `plugins_dir`/`scripts_dir`
/// can't redirect an elevated write to an arbitrary location. Elevated
/// custom-dir installs are kept out of privileged locations, and any effect
/// subfolder must be a single plain name. The deployment engine must receive parameters resolved through this first.
pub(crate) fn resolve_install_params(mut params: InstallParams) -> AppResult<InstallParams> {
    if let Some(ae) = &params.installation {
        let detected = ae::detect_installations()
            .into_iter()
            .find(|d| d.root == ae.root)
            .ok_or_else(|| {
                AppError::msg("インストール先の After Effects が確認できませんでした。")
            })?;
        params.installation = Some(detected);
    }
    if params.elevated.unwrap_or(false) {
        if let Some(dir) = params
            .custom_dir
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            reject_privileged_dir(dir)?;
        }
    }
    if let Some(sub) = params
        .effect_subdir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !crate::library::is_safe_segment(sub) {
            return Err(AppError::msg("エフェクトのサブフォルダ名が不正です"));
        }
    }
    Ok(params)
}

/// Install requests are preflighted and committed by one recoverable engine.
#[tauri::command]
pub async fn install_package(
    app: AppHandle,
    id: String,
    params: InstallParams,
) -> AppResult<Package> {
    library_task(app, move |store| {
        let params = resolve_install_params(params)?;
        crate::deployment::apply(store, &[(id.clone(), params)])?;
        store.get(&id)
    })
    .await
}

#[tauri::command]
pub async fn install_all_ae(
    app: AppHandle,
    id: String,
    mut params: InstallParams,
) -> AppResult<Package> {
    library_task(app, move |store| {
        params.installation = None;
        params.custom_dir = None;
        let params = resolve_install_params(params)?;
        let aes = ae::detect_installations();
        if aes.is_empty() {
            return Err(AppError::msg("After Effects が見つかりません"));
        }
        install_to_all(store, &id, &aes, &params)
    })
    .await
}

pub(crate) fn install_to_all(
    store: &LibraryStore,
    id: &str,
    aes: &[AeInstallation],
    params: &InstallParams,
) -> AppResult<Package> {
    let requests: Vec<_> = aes
        .iter()
        .map(|ae| {
            let mut one = params.clone();
            one.installation = Some(ae.clone());
            one.custom_dir = None;
            (id.to_string(), one)
        })
        .collect();
    crate::deployment::apply(store, &requests)?;
    store.get(id)
}

/// Remove an installed copy: delete the written files and drop its record.
/// Removal errors are surfaced (not ignored); a permission failure signals the
/// frontend to retry with `elevated`. The record is only dropped once every
/// path is actually gone.
#[tauri::command]
pub async fn uninstall_package(
    app: AppHandle,
    id: String,
    record_id: String,
    elevated: Option<bool>,
) -> AppResult<Package> {
    library_task(app, move |store| {
        let mut pkg = store.get(&id)?;
        let mut record = pkg
            .installs
            .iter()
            .find(|r| r.id == record_id)
            .cloned()
            .ok_or_else(|| AppError::msg("インストール記録が見つかりません"))?;

        maintenance::ensure_uninstall_owned(store, &pkg, &record)?;
        // CEP directories can be shared by this package's installs in several AE
        // versions. Removing one record must not remove the others' shared files.
        record.paths.retain(|path| {
            !pkg.installs
                .iter()
                .filter(|r| r.id != record_id)
                .any(|r| r.paths.contains(path))
        });

        for path in &record.paths {
            crate::deployment::validate_target(Path::new(path))?;
        }

        if elevated.unwrap_or(false) {
            let paths: Vec<PathBuf> = record.paths.iter().map(PathBuf::from).collect();
            installer::elevated_remove(&paths)?;
        } else {
            let mut perm_denied: Option<String> = None;
            for p in &record.paths {
                let path = Path::new(p);
                let res = if path.is_dir() {
                    std::fs::remove_dir_all(path)
                } else if path.exists() {
                    std::fs::remove_file(path)
                } else {
                    Ok(()) // already gone
                };
                if let Err(e) = res {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        perm_denied = Some(path.display().to_string());
                    } else {
                        return Err(AppError::from(e));
                    }
                }
            }
            // Leave the record in place so the user can retry (with elevation).
            if let Some(p) = perm_denied {
                return Err(AppError::msg(format!(
                    "削除できませんでした: {p}\nPermission denied (os error 13)"
                )));
            }
        }

        pkg.installs.retain(|r| r.id != record_id);
        store.upsert(pkg.clone())?;
        log_action(store, "uninstall", Some(&pkg), &record.label);
        Ok(pkg)
    })
    .await
}

#[tauri::command]
pub async fn export_library(
    app: AppHandle,
    package_ids: Vec<String>,
    dest: String,
) -> AppResult<usize> {
    library_task(app, move |store| {
        archive::export_library(store, &package_ids, Path::new(&dest))
    })
    .await
}

/// Export one package as a self-contained distribution `.zip`
/// (manifest + banner + files).
#[tauri::command]
pub async fn export_package(app: AppHandle, id: String, dest: String) -> AppResult<()> {
    library_task(app, move |store| {
        archive::export_package(store, &id, Path::new(&dest))
    })
    .await
}

#[tauri::command]
pub fn import_archive(app: AppHandle, zip_path: String) -> AppResult<Vec<Package>> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    archive::import_archive(&store, Path::new(&zip_path))
}

// ---- Bundles (series of effect plugins / scripts) ----

/// Create a bundle, optionally pre-populated with selected members (filtered to
/// existing, bundleable packages).
#[tauri::command]
pub fn create_bundle(app: AppHandle, name: String, package_ids: Vec<String>) -> AppResult<Bundle> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let lib = store.load()?;
    let members: Vec<String> = package_ids
        .into_iter()
        .filter(|pid| {
            lib.packages
                .iter()
                .any(|p| &p.id == pid && p.kind.bundleable())
        })
        .collect();
    let ts = now();
    let bundle = Bundle {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        description: String::new(),
        author: String::new(),
        homepage: String::new(),
        banner: None,
        package_ids: members,
        created_at: ts.clone(),
        updated_at: ts,
    };
    store.upsert_bundle(bundle.clone())?;
    Ok(bundle)
}

/// Set a bundle's banner image from the file at `source_path`.
#[tauri::command]
pub fn set_bundle_banner(app: AppHandle, id: String, source_path: String) -> AppResult<Bundle> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut bundle = store.get_bundle(&id)?;
    let banner_name = write_banner_file(&store.bundle_dir(&id), &PathBuf::from(&source_path))?;
    bundle.banner = Some(banner_name);
    bundle.updated_at = now();
    store.upsert_bundle(bundle.clone())?;
    Ok(bundle)
}

/// Replace a bundle banner with the image currently on the clipboard.
#[tauri::command]
pub fn set_bundle_banner_clipboard(app: AppHandle, id: String) -> AppResult<Bundle> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut bundle = store.get_bundle(&id)?;
    let banner_name = write_banner_from_clipboard(&store.bundle_dir(&id))?;
    bundle.banner = Some(banner_name);
    bundle.updated_at = now();
    store.upsert_bundle(bundle.clone())?;
    Ok(bundle)
}

#[tauri::command]
pub fn clear_bundle_banner(app: AppHandle, id: String) -> AppResult<Bundle> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut bundle = store.get_bundle(&id)?;
    if let Some(banner) = bundle.banner.take() {
        remove_banner_file(&store.bundle_dir(&id), &banner);
    }
    bundle.updated_at = now();
    store.upsert_bundle(bundle.clone())?;
    Ok(bundle)
}

#[tauri::command]
pub fn read_bundle_banner(app: AppHandle, id: String) -> AppResult<Option<String>> {
    let store = store(&app)?;
    let bundle = store.get_bundle(&id)?;
    let Some(banner) = bundle.banner else {
        return Ok(None);
    };
    banner_data_url(&store.bundle_dir(&id), &banner)
}

/// Update a bundle. Members are filtered to existing, bundleable packages
/// (effect plugins / scripts only).
#[tauri::command]
pub fn update_bundle(app: AppHandle, bundle: Bundle) -> AppResult<Bundle> {
    let _guard = operation_guard()?;
    let store = store(&app)?;
    let mut b = store.get_bundle(&bundle.id)?;
    let lib = store.load()?;
    b.name = bundle.name;
    b.description = bundle.description;
    b.author = bundle.author;
    b.homepage = bundle.homepage;
    b.package_ids = bundle.package_ids;
    b.package_ids.retain(|pid| {
        lib.packages
            .iter()
            .any(|p| p.id == *pid && p.kind.bundleable())
    });
    b.updated_at = now();
    store.upsert_bundle(b.clone())?;
    Ok(b)
}

#[tauri::command]
pub fn delete_bundle(app: AppHandle, id: String) -> AppResult<()> {
    let _guard = operation_guard()?;
    store(&app)?.remove_bundle(&id)
}

/// Bundles and selections share preflight, rollback and one elevation prompt.
#[tauri::command]
pub async fn install_bundle(
    app: AppHandle,
    id: String,
    params: InstallParams,
) -> AppResult<Vec<Package>> {
    library_task(app, move |store| {
        let bundle = store.get_bundle(&id)?;
        let params = resolve_install_params(params)?;
        let requests: Vec<_> = bundle
            .package_ids
            .iter()
            .filter(|id| store.get(id).is_ok_and(|p| p.kind.bundleable()))
            .map(|id| (id.clone(), params.clone()))
            .collect();
        crate::deployment::apply(store, &requests)
    })
    .await
}

#[tauri::command]
pub async fn install_packages(
    app: AppHandle,
    ids: Vec<String>,
    params: InstallParams,
) -> AppResult<Vec<Package>> {
    library_task(app, move |store| {
        let params = resolve_install_params(params)?;
        let ids: std::collections::BTreeSet<_> = ids.into_iter().collect();
        let requests: Vec<_> = ids.into_iter().map(|id| (id, params.clone())).collect();
        crate::deployment::apply(store, &requests)
    })
    .await
}

/// Export a bundle as a single distribution `.zip` (the whole series).
#[tauri::command]
pub async fn export_bundle(app: AppHandle, id: String, dest: String) -> AppResult<()> {
    library_task(app, move |store| {
        archive::export_bundle(store, &id, Path::new(&dest))
    })
    .await
}
