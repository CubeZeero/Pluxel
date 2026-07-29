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

/// Copy `src` into `dir` as `banner.<ext>`, returning the stored filename.
fn write_banner_file(dir: &Path, src: &Path) -> AppResult<String> {
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "png".into());
    let banner_name = format!("banner.{ext}");
    std::fs::create_dir_all(dir)?;
    std::fs::copy(src, dir.join(&banner_name))?;
    Ok(banner_name)
}

/// Best-effort delete of a stored banner file.
fn remove_banner_file(dir: &Path, name: &str) {
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
    let bytes = std::fs::read(&path)?;
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
    store(&app)?.load()
}

/// The absolute path of the Pluxel data directory (for display / reveal).
#[tauri::command]
pub fn get_data_dir(app: AppHandle) -> AppResult<String> {
    Ok(store(&app)?.root().to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_paths(app: AppHandle, paths: Vec<String>) -> AppResult<Vec<Package>> {
    let store = store(&app)?;
    let mut imported = Vec::new();
    for p in paths {
        imported.push(importer::import_path(&store, Path::new(&p))?);
    }
    Ok(imported)
}

/// Attach companion files/folders (e.g. `.ffx`, image assets) to a package.
#[tauri::command]
pub fn add_files(app: AppHandle, id: String, paths: Vec<String>) -> AppResult<Package> {
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    for p in &paths {
        importer::add_source(&store, &id, Path::new(p))?;
    }
    importer::rescan(&store, &mut pkg)?;
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Replace all of a package's stored files with new sources (a version bump).
#[tauri::command]
pub fn replace_files(app: AppHandle, id: String, paths: Vec<String>) -> AppResult<Package> {
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    let srcs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    importer::replace_files(&store, &mut pkg, &srcs)?;
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

#[tauri::command]
pub fn remove_package(app: AppHandle, id: String) -> AppResult<()> {
    store(&app)?.remove(&id)
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
        Manifest { tags: old.tags, ..src }
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
pub fn replace_package(app: AppHandle, target_id: String, source_id: String) -> AppResult<Package> {
    let store = store(&app)?;
    let mut target = store.get(&target_id)?;
    let source = store.get(&source_id)?;

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
        let sp = store.package_dir(&source_id).join(b);
        if sp.exists() {
            std::fs::copy(&sp, store.package_dir(&target_id).join(b))?;
            target.banner = Some(b.clone());
        }
    }
    target.updated_at = now();

    store.write_manifest(&target_id, &target.manifest)?;
    store.upsert(target.clone())?;
    store.remove(&source_id)?; // discard the freshly-imported copy
    Ok(target)
}

/// Remove a single stored file (or a top-level folder/bundle) from a package.
/// `rel_path` is relative to the package `files/` directory.
#[tauri::command]
pub fn remove_file(app: AppHandle, id: String, rel_path: String) -> AppResult<Package> {
    // Reject anything that isn't a purely-relative path: an absolute path,
    // Windows drive/UNC prefix, or a `..` component would let `join` escape the
    // package's files/ directory and delete arbitrary files.
    let rel = Path::new(&rel_path);
    let path_ok = !rel_path.trim().is_empty()
        && rel.components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir));
    if !path_ok {
        return Err(AppError::msg("不正なパスです"));
    }
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    let files_dir = store.files_dir(&id);
    let target = files_dir.join(&rel_path);
    // Belt-and-suspenders: resolve symlinks and confirm the target stays inside
    // files_dir before deleting anything.
    if let Ok(canon) = target.canonicalize() {
        let base = files_dir.canonicalize().unwrap_or(files_dir.clone());
        if !canon.starts_with(&base) {
            return Err(AppError::msg("不正なパスです"));
        }
    }
    if target.is_dir() {
        std::fs::remove_dir_all(&target)?;
    } else if target.exists() {
        std::fs::remove_file(&target)?;
    } else {
        return Err(AppError::msg("ファイルが見つかりません"));
    }
    importer::rescan(&store, &mut pkg)?;
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

#[tauri::command]
pub fn update_manifest(app: AppHandle, id: String, manifest: Manifest) -> AppResult<Package> {
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    if let Some(kind) = manifest.install_as {
        pkg.kind = kind;
    }
    pkg.manifest = manifest.clone();
    pkg.updated_at = now();
    store.write_manifest(&id, &manifest)?;
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Replace a package banner with the image at `source_path`.
#[tauri::command]
pub fn set_banner(app: AppHandle, id: String, source_path: String) -> AppResult<Package> {
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    let banner_name = write_banner_file(&store.package_dir(&id), &PathBuf::from(&source_path))?;
    pkg.banner = Some(banner_name);
    pkg.updated_at = now();
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Remove a package's banner image (delete the file and clear the reference).
#[tauri::command]
pub fn clear_banner(app: AppHandle, id: String) -> AppResult<Package> {
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
            .map(|e| PackageKind::from_extension(e) == PackageKind::Installer)
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
        .ok_or_else(|| AppError::msg("インストーラー (.pkg / .exe / .msi) が見つかりません"))?;

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
    Ok(pkg)
}

/// Open a file with the OS default handler (launches installers).
fn launch(path: &Path) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        // Launch via Explorer with the path as a single argument — this invokes
        // the shell default action without going through `cmd /C start`, whose
        // re-parsing lets a crafted file name inject commands.
        std::process::Command::new("explorer.exe").arg(path).spawn()?;
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
        std::process::Command::new("open").arg("-R").arg(p).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        // `/select,` opens Explorer with the item highlighted. Passed as a single
        // argument (no `cmd /C`) so there is no shell re-parsing / injection.
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", p.display()))
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
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct InstallParams {
    /// A detected AE installation (used with `kind` when `custom_dir` is unset).
    pub installation: Option<AeInstallation>,
    /// The target kind (Scripts / ScriptUI Panels / Plug-ins / CEP).
    pub kind: Option<PackageKind>,
    /// A user-chosen folder that overrides the AE-derived location entirely.
    pub custom_dir: Option<String>,
    /// Explicit label (used when reinstalling/updating an existing record).
    pub label: Option<String>,
    /// Perform the install with elevated privileges (password / Touch ID / UAC).
    pub elevated: Option<bool>,
    /// Subfolder under Plug-ins to nest effect plugins into (e.g. "Effects").
    pub effect_subdir: Option<String>,
}

/// Reject an elevated custom install into well-known privileged/system
/// locations, so a tampered `custom_dir` can't drop a root-owned payload (e.g.
/// a LaunchDaemon or Windows service) via the elevated copy.
pub(crate) fn reject_privileged_dir(dir: &str) -> AppResult<()> {
    let norm = dir.replace('\\', "/");
    let low = norm.trim_end_matches('/').to_ascii_lowercase();
    #[cfg(target_os = "macos")]
    let blocked: &[&str] = &[
        "/system",
        "/usr",
        "/bin",
        "/sbin",
        "/private",
        "/library/launchdaemons",
        "/library/launchagents",
        "/library/startupitems",
        "/library/privilegedhelpertools",
    ];
    #[cfg(target_os = "windows")]
    let blocked: &[&str] = &["c:/windows", "c:/program files", "c:/program files (x86)"];
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let blocked: &[&str] = &["/bin", "/sbin", "/usr", "/etc", "/boot"];
    if blocked
        .iter()
        .any(|b| low == *b || low.starts_with(&format!("{b}/")))
    {
        return Err(AppError::msg(
            "このフォルダへの管理者権限インストールは許可されていません。",
        ));
    }
    Ok(())
}

/// Validate and canonicalize install params once, before any package is
/// installed. An AE installation must match a currently-detected one, and we
/// substitute the DETECTED struct so a tampered `plugins_dir`/`scripts_dir`
/// can't redirect an elevated write to an arbitrary location. Elevated
/// custom-dir installs are kept out of privileged locations, and any effect
/// subfolder must be a single plain name. Callers of `install_one` /
/// `install_ids_elevated` must pass params that went through this first.
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
        if let Some(dir) = params.custom_dir.as_deref().filter(|s| !s.trim().is_empty()) {
            reject_privileged_dir(dir)?;
        }
    }
    if let Some(sub) = params.effect_subdir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if !crate::library::is_safe_segment(sub) {
            return Err(AppError::msg("エフェクトのサブフォルダ名が不正です"));
        }
    }
    Ok(params)
}

/// Install one package and record where it landed. `params` must already be
/// validated by [`resolve_install_params`].
fn install_one(store: &LibraryStore, id: &str, params: &InstallParams) -> AppResult<Package> {
    let mut pkg = store.get(id)?;
    let primary = params.kind.unwrap_or(pkg.kind);
    let elevated = params.elevated.unwrap_or(false);
    let effect_subdir = params
        .effect_subdir
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let (paths, target_str, label, ae) = match params.custom_dir.as_deref() {
        Some(dir) if !dir.trim().is_empty() => {
            let dir_path = PathBuf::from(dir);
            let paths = if elevated {
                installer::install_to_elevated(store, &pkg, &dir_path)?
            } else {
                installer::install_to(store, &pkg, &dir_path)?
            };
            (paths, dir.to_string(), format!("カスタム: {dir}"), None)
        }
        _ => {
            let ae = params
                .installation
                .clone()
                .ok_or_else(|| AppError::msg("インストール先が指定されていません"))?;
            let paths = if elevated {
                installer::install_to_ae_elevated(store, &pkg, &ae, primary, effect_subdir)?
            } else {
                installer::install_to_ae(store, &pkg, &ae, primary, effect_subdir)?
            };
            let target = installer::route(&ae, primary, primary, effect_subdir)
                .to_string_lossy()
                .to_string();
            let label = ae.name.clone();
            (paths, target, label, Some(ae))
        }
    };

    // Same destination → update in place (drop the old record, add the new one).
    pkg.installs.retain(|r| r.target_dir != target_str);
    pkg.installs.push(InstallRecord {
        id: uuid::Uuid::new_v4().to_string(),
        label: params.label.clone().filter(|l| !l.trim().is_empty()).unwrap_or(label),
        kind: primary,
        target_dir: target_str,
        ae,
        paths,
        installed_at: now(),
    });
    store.upsert(pkg.clone())?;
    Ok(pkg)
}

/// Install a package, recording where it landed so it can be updated/uninstalled.
/// Re-installing to the same target directory replaces the previous record.
#[tauri::command]
pub fn install_package(app: AppHandle, id: String, params: InstallParams) -> AppResult<Package> {
    let store = store(&app)?;
    let params = resolve_install_params(params)?;
    install_one(&store, &id, &params)
}

/// Remove an installed copy: delete the written files and drop its record.
/// Removal errors are surfaced (not ignored); a permission failure signals the
/// frontend to retry with `elevated`. The record is only dropped once every
/// path is actually gone.
#[tauri::command]
pub fn uninstall_package(
    app: AppHandle,
    id: String,
    record_id: String,
    elevated: Option<bool>,
) -> AppResult<Package> {
    let store = store(&app)?;
    let mut pkg = store.get(&id)?;
    let record = pkg
        .installs
        .iter()
        .find(|r| r.id == record_id)
        .cloned()
        .ok_or_else(|| AppError::msg("インストール記録が見つかりません"))?;

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
    Ok(pkg)
}

#[tauri::command]
pub fn export_library(app: AppHandle, package_ids: Vec<String>, dest: String) -> AppResult<usize> {
    let store = store(&app)?;
    archive::export_library(&store, &package_ids, Path::new(&dest))
}

/// Export one package as a self-contained distribution `.zip`
/// (manifest + banner + files).
#[tauri::command]
pub fn export_package(app: AppHandle, id: String, dest: String) -> AppResult<()> {
    let store = store(&app)?;
    archive::export_package(&store, &id, Path::new(&dest))
}

#[tauri::command]
pub fn import_archive(app: AppHandle, zip_path: String) -> AppResult<Vec<Package>> {
    let store = store(&app)?;
    archive::import_archive(&store, Path::new(&zip_path))
}

// ---- Bundles (series of effect plugins / scripts) ----

/// Create a bundle, optionally pre-populated with selected members (filtered to
/// existing, bundleable packages).
#[tauri::command]
pub fn create_bundle(app: AppHandle, name: String, package_ids: Vec<String>) -> AppResult<Bundle> {
    let store = store(&app)?;
    let lib = store.load()?;
    let members: Vec<String> = package_ids
        .into_iter()
        .filter(|pid| lib.packages.iter().any(|p| &p.id == pid && p.kind.bundleable()))
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
    let store = store(&app)?;
    let mut bundle = store.get_bundle(&id)?;
    let banner_name = write_banner_file(&store.bundle_dir(&id), &PathBuf::from(&source_path))?;
    bundle.banner = Some(banner_name);
    bundle.updated_at = now();
    store.upsert_bundle(bundle.clone())?;
    Ok(bundle)
}

#[tauri::command]
pub fn clear_bundle_banner(app: AppHandle, id: String) -> AppResult<Bundle> {
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
    let store = store(&app)?;
    let lib = store.load()?;
    let mut b = bundle;
    b.package_ids
        .retain(|pid| lib.packages.iter().any(|p| p.id == *pid && p.kind.bundleable()));
    b.updated_at = now();
    store.upsert_bundle(b.clone())?;
    Ok(b)
}

#[tauri::command]
pub fn delete_bundle(app: AppHandle, id: String) -> AppResult<()> {
    store(&app)?.remove_bundle(&id)
}

/// Install every member of a bundle to the given target (a whole series).
#[tauri::command]
pub fn install_bundle(app: AppHandle, id: String, params: InstallParams) -> AppResult<Vec<Package>> {
    let store = store(&app)?;
    let bundle = store.get_bundle(&id)?;
    let params = resolve_install_params(params)?;

    if params.elevated.unwrap_or(false) {
        // Batch every member's copies into ONE elevated operation, so the user
        // authenticates once for the whole series instead of per member.
        return install_ids_elevated(&store, &bundle.package_ids, &params, true);
    }

    let mut out = Vec::new();
    for pid in &bundle.package_ids {
        match store.get(pid) {
            Ok(pkg) if pkg.kind.bundleable() => out.push(install_one(&store, pid, &params)?),
            _ => {}
        }
    }
    Ok(out)
}

/// Install several packages (by id) at once. Elevated installs are batched into
/// a single auth/UAC prompt. Re-installing an already-installed target updates
/// it. Used by the multi-select "install selected" action.
#[tauri::command]
pub fn install_packages(
    app: AppHandle,
    ids: Vec<String>,
    params: InstallParams,
) -> AppResult<Vec<Package>> {
    let store = store(&app)?;
    let params = resolve_install_params(params)?;
    if params.elevated.unwrap_or(false) {
        return install_ids_elevated(&store, &ids, &params, false);
    }
    let mut out = Vec::new();
    for id in &ids {
        out.push(install_one(&store, id, &params)?);
    }
    Ok(out)
}

/// Install a set of packages (by id) with a single elevated copy (one auth / UAC
/// prompt), then record where each package landed. `bundleable_only` skips
/// non-bundleable members (used for bundles/series).
fn install_ids_elevated(
    store: &LibraryStore,
    ids: &[String],
    params: &InstallParams,
    bundleable_only: bool,
) -> AppResult<Vec<Package>> {
    let effect_subdir = params.effect_subdir.as_deref().filter(|s| !s.trim().is_empty());
    let mut all_copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut staged: Vec<(Package, InstallRecord)> = Vec::new();

    for pid in ids {
        let pkg = match store.get(pid) {
            Ok(p) if !bundleable_only || p.kind.bundleable() => p,
            _ => continue,
        };
        let primary = params.kind.unwrap_or(pkg.kind);
        let files_dir = store.files_dir(&pkg.id);

        let (copies, written, target_str, label, ae) = match params.custom_dir.as_deref() {
            Some(dir) if !dir.trim().is_empty() => {
                let (c, w) = installer::plan_copies(&files_dir, |_| PathBuf::from(dir))?;
                (c, w, dir.to_string(), format!("カスタム: {dir}"), None)
            }
            _ => {
                let ae = params
                    .installation
                    .clone()
                    .ok_or_else(|| AppError::msg("インストール先が指定されていません"))?;
                let (c, w) = installer::plan_copies(&files_dir, |p| {
                    installer::route(&ae, installer::entry_kind(p), primary, effect_subdir)
                })?;
                let target = installer::route(&ae, primary, primary, effect_subdir)
                    .to_string_lossy()
                    .to_string();
                (c, w, target, ae.name.clone(), Some(ae))
            }
        };

        all_copies.extend(copies);
        staged.push((
            pkg,
            InstallRecord {
                id: uuid::Uuid::new_v4().to_string(),
                label: params
                    .label
                    .clone()
                    .filter(|l| !l.trim().is_empty())
                    .unwrap_or(label),
                kind: primary,
                target_dir: target_str,
                ae,
                paths: written,
                installed_at: now(),
            },
        ));
    }

    // The single elevated write for the entire series.
    installer::elevated_copy(&all_copies)?;

    let mut out = Vec::new();
    for (mut pkg, record) in staged {
        pkg.installs.retain(|r| r.target_dir != record.target_dir);
        pkg.installs.push(record);
        store.upsert(pkg.clone())?;
        out.push(pkg);
    }
    Ok(out)
}

/// Export a bundle as a single distribution `.zip` (the whole series).
#[tauri::command]
pub fn export_bundle(app: AppHandle, id: String, dest: String) -> AppResult<()> {
    let store = store(&app)?;
    archive::export_bundle(&store, &id, Path::new(&dest))
}
