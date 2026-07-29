use std::fs;
use std::path::Path;

use chrono::Utc;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::library::LibraryStore;
use crate::models::{Manifest, Package, PackageFile, PackageKind};

const BANNER_STEMS: [&str; 3] = ["banner", "preview", "thumbnail"];
const IMAGE_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// Recursively copy a file or directory `src` to `dst` (which is the full
/// destination path, not a parent directory).
pub fn copy_recursively(src: &Path, dst: &Path) -> AppResult<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in WalkDir::new(src).min_depth(1) {
            let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
            let rel = entry
                .path()
                .strip_prefix(src)
                .map_err(|e| AppError::msg(e.to_string()))?;
            let target = dst.join(rel);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(entry.path(), &target)?;
            }
        }
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn is_banner_candidate(path: &Path) -> bool {
    IMAGE_EXTS.contains(&extension_of(path).to_ascii_lowercase().as_str())
}

/// Pick the highest-priority kind among a set of files.
fn classify_files(files: &[PackageFile]) -> PackageKind {
    let priority = |k: PackageKind| match k {
        PackageKind::Installer => 5,
        PackageKind::Zxp => 4,
        PackageKind::Plugin => 3,
        PackageKind::ScriptUiPanel => 2,
        PackageKind::Script => 1,
        PackageKind::Unknown => 0,
    };
    files
        .iter()
        .map(|f| f.kind)
        .max_by_key(|k| priority(*k))
        .unwrap_or(PackageKind::Unknown)
}

/// Classify the kind of an imported source path (file or bundle/folder).
fn classify_source(src: &Path, files: &[PackageFile]) -> PackageKind {
    // A bundle/folder or file whose own extension is meaningful wins first.
    let own = PackageKind::from_extension(&extension_of(src));
    if own != PackageKind::Unknown {
        return own;
    }
    // Otherwise pick the highest-priority kind among contained files.
    classify_files(files)
}

/// Enumerate the asset files under a package `files/` dir into [`PackageFile`]s.
fn collect_files(files_dir: &Path) -> AppResult<Vec<PackageFile>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(files_dir).min_depth(1) {
        let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(files_dir)
            .map_err(|e| AppError::msg(e.to_string()))?;
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(PackageFile {
            rel_path,
            kind: PackageKind::from_extension(&extension_of(entry.path())),
            size,
        });
    }
    Ok(out)
}

/// Look for a `manifest.json` inside the package files and parse it.
fn find_manifest(files_dir: &Path) -> Option<Manifest> {
    let mut best: Option<(usize, Manifest)> = None;
    for entry in WalkDir::new(files_dir).min_depth(1) {
        let Ok(entry) = entry else { continue };
        if entry.file_name() == "manifest.json" {
            if let Ok(data) = fs::read_to_string(entry.path()) {
                if let Ok(m) = serde_json::from_str::<Manifest>(&data) {
                    let depth = entry.depth();
                    // Prefer the shallowest manifest.
                    if best.as_ref().map(|(d, _)| depth < *d).unwrap_or(true) {
                        best = Some((depth, m));
                    }
                }
            }
        }
    }
    best.map(|(_, m)| m)
}

/// Find a banner image inside the package files, preferring conventional names
/// (banner/preview/thumbnail) and shallower paths.
fn find_banner(files_dir: &Path) -> Option<std::path::PathBuf> {
    let mut best: Option<(u32, std::path::PathBuf)> = None;
    for entry in WalkDir::new(files_dir).min_depth(1) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() || !is_banner_candidate(entry.path()) {
            continue;
        }
        let stem = entry
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let named = BANNER_STEMS.iter().any(|s| stem == *s);
        // Lower score = better. Conventional name and shallow depth win.
        let score = (if named { 0 } else { 100 }) + entry.depth() as u32;
        if best.as_ref().map(|(b, _)| score < *b).unwrap_or(true) {
            best = Some((score, entry.path().to_path_buf()));
        }
    }
    best.map(|(_, p)| p)
}

/// Find a banner image among a package's stored files and copy it to the
/// package root as `banner.<ext>`, returning that file name.
fn adopt_banner(store: &LibraryStore, id: &str) -> AppResult<Option<String>> {
    let files_dir = store.files_dir(id);
    match find_banner(&files_dir) {
        Some(src) => {
            let ext = extension_of(&src).to_ascii_lowercase();
            let banner_name = format!("banner.{ext}");
            fs::copy(&src, store.package_dir(id).join(&banner_name))?;
            Ok(Some(banner_name))
        }
        None => Ok(None),
    }
}

/// Import a single source path (file or folder) into the library as one package.
pub fn import_path(store: &LibraryStore, src: &Path) -> AppResult<Package> {
    if !src.exists() {
        return Err(AppError::msg(format!("Path does not exist: {}", src.display())));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let files_dir = store.files_dir(&id);
    fs::create_dir_all(&files_dir)?;

    // Copy the source in under its own basename to preserve bundles/folders.
    let basename = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".to_string());
    let dest = files_dir.join(&basename);
    if let Err(e) = copy_recursively(src, &dest) {
        let _ = fs::remove_dir_all(store.package_dir(&id));
        return Err(e);
    }

    let files = collect_files(&files_dir)?;
    let kind = classify_source(src, &files);

    // Manifest: from an embedded manifest.json, else derived defaults.
    let mut manifest = find_manifest(&files_dir).unwrap_or_default();
    if manifest.name.trim().is_empty() {
        manifest.name = src
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| basename.clone());
    }

    // Reject unsupported inputs (e.g. a lone image or text file).
    let final_kind = manifest.install_as.unwrap_or(kind);
    if final_kind == PackageKind::Unknown {
        let _ = fs::remove_dir_all(store.package_dir(&id));
        return Err(AppError::msg(format!(
            "対応していない形式です: {basename}\nスクリプト(.jsx/.js/.jsxbin)・エフェクト(.plugin/.aex)・プラグイン(.zxp)・インストーラー(.pkg/.exe/.msi)に対応しています。"
        )));
    }

    // Banner: copy to the package root as banner.<ext> for stable referencing.
    let banner = adopt_banner(store, &id)?;

    let now = Utc::now().to_rfc3339();
    let package = Package {
        id,
        kind: final_kind,
        manifest: manifest.clone(),
        banner,
        files,
        installs: Vec::new(),
        added_at: now.clone(),
        updated_at: now,
    };

    store.write_manifest(&package.id, &manifest)?;
    store.upsert(package.clone())?;
    Ok(package)
}

/// Copy an additional source (companion `.ffx`, image folder, etc.) into an
/// existing package's `files/` directory, preserving its structure.
pub fn add_source(store: &LibraryStore, id: &str, src: &Path) -> AppResult<()> {
    if !src.exists() {
        return Err(AppError::msg(format!("Path does not exist: {}", src.display())));
    }
    let files_dir = store.files_dir(id);
    fs::create_dir_all(&files_dir)?;
    let basename = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".to_string());
    copy_recursively(src, &files_dir.join(basename))
}

/// Re-read a package's `files/` directory and refresh its file list, kind and
/// banner. Honors a manifest `install_as` override for the kind.
pub fn rescan(store: &LibraryStore, pkg: &mut Package) -> AppResult<()> {
    let files_dir = store.files_dir(&pkg.id);
    pkg.files = collect_files(&files_dir)?;
    pkg.kind = pkg
        .manifest
        .install_as
        .unwrap_or_else(|| classify_files(&pkg.files));
    if pkg.banner.is_none() {
        pkg.banner = adopt_banner(store, &pkg.id)?;
    }
    pkg.updated_at = Utc::now().to_rfc3339();
    Ok(())
}

/// Import a distribution-format package directory: a flat folder holding a
/// `manifest.json`, a conventional banner image, and the asset files at its
/// root. Produced by [`crate::archive::export_package`].
pub fn import_distribution(store: &LibraryStore, dir: &Path) -> AppResult<Package> {
    let id = uuid::Uuid::new_v4().to_string();
    let files_dir = store.files_dir(&id);
    fs::create_dir_all(&files_dir)?;

    let manifest_path = dir.join("manifest.json");
    let mut manifest: Manifest = if manifest_path.exists() {
        serde_json::from_str(&fs::read_to_string(&manifest_path)?).unwrap_or_default()
    } else {
        Manifest::default()
    };

    // Copy each root entry into files/, lifting out manifest.json and a
    // conventionally-named banner image (banner/preview/thumbnail).
    let mut banner = None;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("manifest.json") {
            continue;
        }
        if banner.is_none() && path.is_file() && is_banner_candidate(&path) {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if BANNER_STEMS.contains(&stem.as_str()) {
                let ext = extension_of(&path).to_ascii_lowercase();
                let banner_name = format!("banner.{ext}");
                fs::copy(&path, store.package_dir(&id).join(&banner_name))?;
                banner = Some(banner_name);
                continue;
            }
        }
        copy_recursively(&path, &files_dir.join(&name))?;
    }

    let files = collect_files(&files_dir)?;
    if manifest.name.trim().is_empty() {
        manifest.name = files
            .iter()
            .find(|f| f.kind != PackageKind::Unknown)
            .or_else(|| files.first())
            .and_then(|f| Path::new(&f.rel_path).file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Imported Package".to_string());
    }
    let kind = manifest.install_as.unwrap_or_else(|| classify_files(&files));

    // Reject zips that aren't a Pluxel package (e.g. a plain zip of images).
    if kind == PackageKind::Unknown {
        let _ = fs::remove_dir_all(store.package_dir(&id));
        return Err(AppError::msg(
            "対応していない zip です。\nPluxel の配布パッケージ / バックアップ、または対応形式のファイルを含む zip を読み込んでください。".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let package = Package {
        id,
        kind,
        manifest: manifest.clone(),
        banner,
        files,
        installs: Vec::new(),
        added_at: now.clone(),
        updated_at: now,
    };
    store.write_manifest(&package.id, &manifest)?;
    store.upsert(package.clone())?;
    Ok(package)
}

/// Replace all of a package's stored files with fresh sources (a version bump),
/// preserving the package id, manifest and banner.
pub fn replace_files(store: &LibraryStore, pkg: &mut Package, srcs: &[std::path::PathBuf]) -> AppResult<()> {
    let files_dir = store.files_dir(&pkg.id);
    if files_dir.exists() {
        fs::remove_dir_all(&files_dir)?;
    }
    fs::create_dir_all(&files_dir)?;
    for src in srcs {
        add_source(store, &pkg.id, src)?;
    }
    rescan(store, pkg)
}
