use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use chrono::Utc;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

use crate::error::{AppError, AppResult};
use crate::library::{is_safe_segment, LibraryStore};
use crate::models::{Bundle, Library, Package};

/// A Pluxel export bundle mirrors the on-disk store layout:
/// ```text
/// export.zip
///   library.json          index of the included packages
///   packages/<id>/...      each package directory verbatim
/// ```
const INDEX_ENTRY: &str = "library.json";

/// Export the given packages (or all, when `package_ids` is empty) into a
/// single `.zip` at `dest`.
pub fn export_library(
    store: &LibraryStore,
    package_ids: &[String],
    dest: &Path,
) -> AppResult<usize> {
    let lib = store.load()?;
    let selected: Vec<Package> = if package_ids.is_empty() {
        lib.packages.clone()
    } else {
        lib.packages
            .iter()
            .filter(|p| package_ids.contains(&p.id))
            .cloned()
            .collect()
    };
    if selected.is_empty() {
        return Err(AppError::msg("No packages selected for export"));
    }

    let file = fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Write the index describing what's inside (bundles referencing included
    // packages are carried along; ids are remapped on import).
    let selected_ids: std::collections::HashSet<&String> = selected.iter().map(|p| &p.id).collect();
    let bundles = lib
        .bundles
        .iter()
        .filter(|b| b.package_ids.iter().any(|id| selected_ids.contains(id)))
        .cloned()
        .collect();
    let index = Library { packages: selected.clone(), bundles };
    zip.start_file(INDEX_ENTRY, opts)?;
    zip.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;

    // Write each package directory verbatim.
    for pkg in &selected {
        let dir = store.package_dir(&pkg.id);
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir).min_depth(1) {
            let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
            let rel = entry
                .path()
                .strip_prefix(store.packages_dir())
                .map_err(|e| AppError::msg(e.to_string()))?;
            let name = format!("packages/{}", rel.to_string_lossy().replace('\\', "/"));
            if entry.file_type().is_dir() {
                zip.add_directory(format!("{name}/"), opts)?;
            } else {
                zip.start_file(&name, opts)?;
                let mut f = fs::File::open(entry.path())?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)?;
                zip.write_all(&buf)?;
            }
        }
    }

    // Include each bundle's banner file (stored outside packages/) so bundle
    // banners survive a backup round-trip; ids are remapped on import.
    for b in &index.bundles {
        if let Some(banner) = b.banner.as_ref().filter(|s| !s.is_empty()) {
            let src = store.bundle_dir(&b.id).join(banner);
            if src.exists() {
                zip.start_file(format!("bundles/{}/{}", b.id, banner), opts)?;
                zip.write_all(&fs::read(&src)?)?;
            }
        }
    }

    zip.finish()?;
    Ok(selected.len())
}

/// Export a single package as a self-contained **distribution** `.zip`:
/// ```text
/// <name>.zip
///   manifest.json      the package metadata
///   banner.png         the banner image (if any)
///   <asset files...>   the package files at the root
/// ```
/// This format is meant for sharing/redistribution and is re-importable via
/// [`import_archive`].
pub fn export_package(store: &LibraryStore, id: &str, dest: &Path) -> AppResult<()> {
    let file = fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    write_distribution(&mut zip, store, id, "", opts)?;
    zip.finish()?;
    Ok(())
}

/// Build a distribution `.ppf` directly from `inputs` (files and/or folders),
/// without going through the library store — used by the CLI. Produces the same
/// layout as [`export_package`] (manifest.json + assets at the root), so the
/// result imports back through [`import_archive`].
pub fn write_ppf_from_paths(
    dest: &Path,
    manifest: &crate::models::Manifest,
    inputs: &[std::path::PathBuf],
) -> AppResult<()> {
    let file = fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // manifest.json — omit tags (user-local); `install_as` carries the kind.
    let mut manifest_json = serde_json::to_value(manifest)?;
    if let Some(obj) = manifest_json.as_object_mut() {
        obj.remove("tags");
    }
    zip.start_file("manifest.json", opts)?;
    zip.write_all(serde_json::to_string_pretty(&manifest_json)?.as_bytes())?;

    for input in inputs {
        if !input.exists() {
            return Err(AppError::msg(format!(
                "no such file or folder: {}",
                input.display()
            )));
        }
        add_input(&mut zip, input, opts)?;
    }
    zip.finish()?;
    Ok(())
}

/// Add a file (at the root) or a folder (preserving its own name as the top
/// segment) to `zip`.
fn add_input(
    zip: &mut zip::ZipWriter<fs::File>,
    input: &Path,
    opts: SimpleFileOptions,
) -> AppResult<()> {
    if input.is_dir() {
        let base = input.parent().unwrap_or(Path::new(""));
        for entry in WalkDir::new(input).min_depth(1) {
            let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
            let rel = entry
                .path()
                .strip_prefix(base)
                .map_err(|e| AppError::msg(e.to_string()))?;
            let name = rel.to_string_lossy().replace('\\', "/");
            if entry.file_type().is_dir() {
                zip.add_directory(format!("{name}/"), opts)?;
            } else {
                zip.start_file(&name, opts)?;
                zip.write_all(&fs::read(entry.path())?)?;
            }
        }
    } else {
        let name = input
            .file_name()
            .ok_or_else(|| AppError::msg("invalid file path".to_string()))?
            .to_string_lossy()
            .to_string();
        zip.start_file(&name, opts)?;
        zip.write_all(&fs::read(input)?)?;
    }
    Ok(())
}

/// Write a package's distribution content (manifest without tags, banner, and
/// files) into `zip` under `prefix` (e.g. `""` or `"packages/Foo/"`).
fn write_distribution(
    zip: &mut zip::ZipWriter<fs::File>,
    store: &LibraryStore,
    id: &str,
    prefix: &str,
    opts: SimpleFileOptions,
) -> AppResult<()> {
    let pkg = store.get(id)?;

    // manifest.json — tags omitted (user-local metadata). Record the package's
    // actual kind as `install_as` so the Script vs ScriptUI (and every other)
    // distinction is preserved on re-import, even if the manifest didn't already
    // carry the override.
    let mut manifest_json = serde_json::to_value(&pkg.manifest)?;
    if let Some(obj) = manifest_json.as_object_mut() {
        obj.remove("tags");
        if pkg.kind != crate::models::PackageKind::Unknown {
            obj.insert("install_as".into(), serde_json::to_value(pkg.kind)?);
        }
    }
    zip.start_file(format!("{prefix}manifest.json"), opts)?;
    zip.write_all(serde_json::to_string_pretty(&manifest_json)?.as_bytes())?;

    // Banner image.
    if let Some(banner) = &pkg.banner {
        let bp = store.package_dir(id).join(banner);
        if bp.exists() {
            zip.start_file(format!("{prefix}{banner}"), opts)?;
            zip.write_all(&fs::read(&bp)?)?;
        }
    }

    // Asset files (skip any stray top-level manifest.json).
    let files_dir = store.files_dir(id);
    for entry in WalkDir::new(&files_dir).min_depth(1) {
        let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
        if entry.depth() == 1 && entry.file_name() == "manifest.json" {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(&files_dir)
            .map_err(|e| AppError::msg(e.to_string()))?;
        let name = format!("{prefix}{}", rel.to_string_lossy().replace('\\', "/"));
        if entry.file_type().is_dir() {
            zip.add_directory(format!("{name}/"), opts)?;
        } else if entry.file_name() == "manifest.json" {
            zip.start_file(&name, opts)?;
            zip.write_all(&strip_tags(entry.path())?)?;
        } else {
            zip.start_file(&name, opts)?;
            zip.write_all(&fs::read(entry.path())?)?;
        }
    }
    Ok(())
}

/// A safe, filesystem-friendly folder name derived from a package name.
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if r#"/\:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "package".to_string() } else { s }
}

/// Export a bundle (a series) as a single distribution `.zip`:
/// ```text
/// <BundleName>.zip
///   bundle.json          { name, description, members: [folder...] }
///   packages/<folder>/   each member as a distribution package
/// ```
pub fn export_bundle(store: &LibraryStore, id: &str, dest: &Path) -> AppResult<()> {
    let bundle = store.get_bundle(id)?;
    let lib = store.load()?;
    let file = fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut used = std::collections::HashSet::new();
    let mut members = Vec::new();
    for pid in &bundle.package_ids {
        let Some(pkg) = lib.packages.iter().find(|p| p.id == *pid) else {
            continue;
        };
        if !pkg.kind.bundleable() {
            continue;
        }
        let base = sanitize(&pkg.manifest.name);
        let mut folder = base.clone();
        let mut n = 2;
        while used.contains(&folder) {
            folder = format!("{base}-{n}");
            n += 1;
        }
        used.insert(folder.clone());
        write_distribution(&mut zip, store, pid, &format!("packages/{folder}/"), opts)?;
        members.push(folder);
    }

    // Bundle banner at the zip root, if any.
    let mut banner_name = serde_json::Value::Null;
    if let Some(banner) = &bundle.banner {
        let bp = store.bundle_dir(id).join(banner);
        if bp.exists() {
            zip.start_file(banner.as_str(), opts)?;
            zip.write_all(&fs::read(&bp)?)?;
            banner_name = serde_json::Value::String(banner.clone());
        }
    }

    let meta = serde_json::json!({
        "name": bundle.name,
        "description": bundle.description,
        "author": bundle.author,
        "homepage": bundle.homepage,
        "banner": banner_name,
        "members": members,
    });
    zip.start_file("bundle.json", opts)?;
    zip.write_all(serde_json::to_string_pretty(&meta)?.as_bytes())?;

    zip.finish()?;
    Ok(())
}

/// Read a `manifest.json` file and return its bytes with any `tags` field
/// removed, preserving all other fields. Non-JSON content is returned as-is.
fn strip_tags(path: &Path) -> AppResult<Vec<u8>> {
    let text = fs::read_to_string(path)?;
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(mut value) => {
            if let Some(obj) = value.as_object_mut() {
                obj.remove("tags");
            }
            Ok(serde_json::to_string_pretty(&value)?.into_bytes())
        }
        Err(_) => Ok(text.into_bytes()),
    }
}

/// Upper bounds guarding against decompression bombs when extracting an
/// untrusted `.zip`: total uncompressed output and entry count.
const MAX_EXTRACT_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
const MAX_EXTRACT_ENTRIES: usize = 100_000;

/// Extract every entry of a `.zip` into `dest` (skipping unsafe paths).
/// Shared by `.zxp` installation and archive import.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> AppResult<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > MAX_EXTRACT_ENTRIES {
        return Err(AppError::msg("アーカイブのエントリ数が多すぎます"));
    }
    let mut remaining = MAX_EXTRACT_BYTES;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(&out_path)?;
            // Cap total output so a highly-compressible archive can't fill the disk.
            let mut limited = std::io::Read::take(&mut entry, remaining + 1);
            let written = std::io::copy(&mut limited, &mut out)?;
            if written > remaining {
                return Err(AppError::msg("アーカイブの展開サイズが大きすぎます"));
            }
            remaining -= written;
        }
    }
    Ok(())
}

/// Import an extracted bundle zip: import each member package and create the
/// bundle grouping them.
fn import_bundle(store: &LibraryStore, staging: &Path, bundle_path: &Path) -> AppResult<Vec<Package>> {
    let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(bundle_path)?)?;
    let field = |k: &str| meta.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name = {
        let n = field("name");
        if n.is_empty() { "Bundle".to_string() } else { n }
    };
    let members = meta.get("members").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let mut ids = Vec::new();
    let mut imported = Vec::new();
    for m in members {
        let Some(folder) = m.as_str() else { continue };
        // Member folder names come from untrusted bundle.json — a `..`/absolute
        // value must not let `import_distribution` read outside the staging dir.
        if !is_safe_segment(folder) {
            continue;
        }
        let dir = staging.join("packages").join(folder);
        if dir.exists() {
            let pkg = crate::importer::import_distribution(store, &dir)?;
            ids.push(pkg.id.clone());
            imported.push(pkg);
        }
    }

    let id = uuid::Uuid::new_v4().to_string();

    // Restore the bundle banner, if present. The name is validated to a single
    // safe filename so a crafted `banner` can't traverse out of the staging or
    // bundle directory.
    let banner = meta.get("banner").and_then(|v| v.as_str());
    let banner = match banner.filter(|b| !b.is_empty() && is_safe_segment(b)) {
        Some(b) if staging.join(b).exists() => {
            let dir = store.bundle_dir(&id);
            fs::create_dir_all(&dir)?;
            fs::copy(staging.join(b), dir.join(b))?;
            Some(b.to_string())
        }
        _ => None,
    };

    let now = Utc::now().to_rfc3339();
    let bundle = Bundle {
        id,
        name,
        description: field("description"),
        author: field("author"),
        homepage: field("homepage"),
        banner,
        package_ids: ids,
        created_at: now.clone(),
        updated_at: now,
    };
    store.upsert_bundle(bundle)?;
    Ok(imported)
}

/// Import a Pluxel `.zip`, adding its packages with fresh ids. Auto-detects the
/// format: a bundle (`bundle.json`), a library backup (`library.json`) or a
/// single distribution package (`export_package` output).
pub fn import_archive(store: &LibraryStore, zip_path: &Path) -> AppResult<Vec<Package>> {
    // Stage extraction in a temp dir under the store root.
    let staging = store.root().join(".import-staging").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&staging)?;

    let result = (|| -> AppResult<Vec<Package>> {
        extract_zip(zip_path, &staging)?;

        // Bundle (series) zip → import members and create the bundle.
        let bundle_path = staging.join("bundle.json");
        if bundle_path.exists() {
            return import_bundle(store, &staging, &bundle_path);
        }

        // Distribution package (no library.json) → import as a single package.
        let index_path = staging.join(INDEX_ENTRY);
        if !index_path.exists() {
            let pkg = crate::importer::import_distribution(store, &staging)?;
            return Ok(vec![pkg]);
        }
        let index: Library = serde_json::from_str(&fs::read_to_string(&index_path)?)?;

        let now = Utc::now().to_rfc3339();
        let mut imported = Vec::new();
        let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for mut pkg in index.packages {
            let old_id = pkg.id.clone();
            // `old_id` comes from untrusted library.json; only a plain segment
            // may be joined onto the staging dir (no `..`/absolute traversal).
            if !is_safe_segment(&old_id) {
                continue;
            }
            let new_id = uuid::Uuid::new_v4().to_string();
            let src_dir = staging.join("packages").join(&old_id);
            if !src_dir.exists() {
                continue;
            }
            let dest_dir = store.package_dir(&new_id);
            crate::importer::copy_recursively(&src_dir, &dest_dir)?;

            pkg.id = new_id.clone();
            pkg.installs.clear(); // install paths belonged to the source machine
            // Drop any banner reference that isn't a plain filename (defends the
            // later read_banner path join against a crafted "../.." value).
            if !pkg.banner.as_deref().map(is_safe_segment).unwrap_or(true) {
                pkg.banner = None;
            }
            pkg.added_at = now.clone();
            pkg.updated_at = now.clone();
            store.write_manifest(&new_id, &pkg.manifest)?;
            store.upsert(pkg.clone())?;
            id_map.insert(old_id, new_id);
            imported.push(pkg);
        }

        // Restore bundles, remapping member ids and dropping any that are missing.
        for mut bundle in index.bundles {
            let old_bid = bundle.id.clone();
            bundle.package_ids = bundle
                .package_ids
                .iter()
                .filter_map(|id| id_map.get(id).cloned())
                .collect();
            if bundle.package_ids.is_empty() {
                continue;
            }
            let new_bid = uuid::Uuid::new_v4().to_string();
            // Restore the bundle banner file, if it was included in the backup.
            // Both the source bundle id and the banner name come from untrusted
            // library.json, so each must be a plain segment before being joined.
            bundle.banner = match bundle.banner.as_ref().filter(|s| !s.is_empty()) {
                Some(b)
                    if is_safe_segment(&old_bid)
                        && is_safe_segment(b)
                        && staging.join("bundles").join(&old_bid).join(b).exists() =>
                {
                    let dir = store.bundle_dir(&new_bid);
                    fs::create_dir_all(&dir)?;
                    fs::copy(staging.join("bundles").join(&old_bid).join(b), dir.join(b))?;
                    Some(b.clone())
                }
                _ => None,
            };
            bundle.id = new_bid;
            bundle.created_at = now.clone();
            bundle.updated_at = now.clone();
            store.upsert_bundle(bundle)?;
        }
        Ok(imported)
    })();

    // Always clean up only THIS import's staging dir (not the shared parent,
    // which a concurrent import may still be using).
    let _ = fs::remove_dir_all(&staging);
    result
}
