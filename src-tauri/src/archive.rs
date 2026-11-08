use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

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

fn file_options(opts: SimpleFileOptions, path: &Path) -> AppResult<SimpleFileOptions> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return Ok(opts.unix_permissions(fs::metadata(path)?.permissions().mode() & 0o777));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(opts)
    }
}

// Resolve aliases and missing suffixes before creating even the temporary output.
fn resolved_path(path: &Path) -> AppResult<PathBuf> {
    let absolute = std::path::absolute(path)?;
    // Resolve each existing prefix first so `link/../file` has filesystem semantics.
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) => resolved.push(component),
            Component::CurDir => (),
            Component::ParentDir => {
                resolved.pop();
            }
            _ => {
                resolved.push(component);
                match resolved.canonicalize() {
                    Ok(path) => resolved = path,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        if resolved.symlink_metadata().is_ok() {
                            return Err(AppError::msg("Unresolvable export path link"));
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    Ok(resolved)
}

// The directory entry being replaced matters too: a symlink at `dest` may
// resolve outside an input while its sibling temporary would be inside it.
fn named_destination(dest: &Path) -> AppResult<PathBuf> {
    let absolute = std::path::absolute(dest)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| AppError::msg("Invalid export path"))?;
    Ok(resolved_path(parent)?.join(
        absolute
            .file_name()
            .ok_or_else(|| AppError::msg("Invalid export name"))?,
    ))
}

fn same_file(a: &Path, b: &Path) -> AppResult<bool> {
    if resolved_path(a)? == resolved_path(b)? {
        return Ok(true);
    }
    #[cfg(unix)]
    if let (Ok(a), Ok(b)) = (fs::metadata(a), fs::metadata(b)) {
        use std::os::unix::fs::MetadataExt;
        return Ok(a.dev() == b.dev() && a.ino() == b.ino());
    }
    #[cfg(windows)]
    if a.is_file() && b.is_file() {
        use std::os::windows::io::AsRawHandle;
        // BY_HANDLE_FILE_INFORMATION consists of thirteen DWORDs. Compare
        // volume serial and file index to recognize hard links on Windows too.
        #[link(name = "kernel32")]
        extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut u32) -> i32;
        }
        let identity = |path: &Path| -> AppResult<(u32, u32, u32)> {
            let file = fs::File::open(path)?;
            let mut info = [0u32; 13];
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok((info[7], info[11], info[12]))
        };
        return Ok(identity(a)? == identity(b)?);
    }
    Ok(false)
}

fn reject_input_overlap(dest: &Path, inputs: &[PathBuf]) -> AppResult<()> {
    let output = resolved_path(dest)?;
    let named = named_destination(dest)?;
    for input in inputs {
        let metadata = fs::metadata(input)?;
        if same_file(dest, input)?
            || (metadata.is_dir()
                && (output.starts_with(resolved_path(input)?)
                    || named.starts_with(resolved_path(input)?)))
        {
            return Err(AppError::msg("Export destination overlaps an input"));
        }
        // An existing destination may be a hard link to a nested source file.
        if metadata.is_dir() && dest.exists() {
            for entry in WalkDir::new(input) {
                let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
                if !entry.file_type().is_dir() && same_file(dest, entry.path())? {
                    return Err(AppError::msg("Export destination aliases an input file"));
                }
            }
        }
    }
    Ok(())
}

fn reject_library_overlap(store: &LibraryStore, dest: &Path) -> AppResult<()> {
    let protected = [
        store.root().join("library.json"),
        store.packages_dir(),
        store.bundles_dir(),
        store.root().join(".revisions"),
    ];
    let output = resolved_path(dest)?;
    let named = named_destination(dest)?;
    for path in protected {
        if output.starts_with(resolved_path(&path)?) || named.starts_with(resolved_path(&path)?) {
            return Err(AppError::msg("Export destination overlaps library payload"));
        }
        if path.exists() {
            reject_input_overlap(dest, &[path])?;
        }
    }
    Ok(())
}

// All exporters publish only a complete archive. A failure leaves an existing
// destination untouched, and the exclusively-created sibling is always cleaned up.
fn atomic_zip<T>(
    dest: &Path,
    write: impl FnOnce(&mut zip::ZipWriter<fs::File>) -> AppResult<T>,
) -> AppResult<T> {
    // Resolve the parent, but replace the named destination rather than following
    // its final symlink when publishing.
    let absolute = std::path::absolute(dest)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| AppError::msg("Invalid export path"))?;
    let parent = parent.canonicalize()?;
    let dest = parent.join(
        absolute
            .file_name()
            .ok_or_else(|| AppError::msg("Invalid export name"))?,
    );
    let temp = parent.join(format!(".pluxel-export-{}.tmp", uuid::Uuid::new_v4()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        let mut zip = zip::ZipWriter::new(file);
        let result = write(&mut zip)?;
        let file = zip.finish()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &dest)?;
        Ok(result)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

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

    reject_library_overlap(store, dest)?;
    atomic_zip(dest, |zip| {
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // Write the index describing what's inside (bundles referencing included
        // packages are carried along; ids are remapped on import).
        let selected_ids: std::collections::HashSet<&String> =
            selected.iter().map(|p| &p.id).collect();
        let bundles = lib
            .bundles
            .iter()
            .filter(|b| b.package_ids.iter().any(|id| selected_ids.contains(id)))
            .cloned()
            .collect();
        let index = Library {
            packages: selected.clone(),
            bundles,
        };
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
                    zip.start_file(&name, file_options(opts, entry.path())?)?;
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

        Ok(selected.len())
    })
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
    reject_library_overlap(store, dest)?;
    atomic_zip(dest, |zip| {
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        write_distribution(zip, store, id, "", opts)
    })
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
    write_ppf_inputs(dest, manifest, inputs, None)
}

/// An explicitly selected banner is stored at the distribution root regardless
/// of its source name. Reject ambiguous root banners rather than relying on
/// filesystem enumeration order during import.
pub fn write_ppf_with_banner(
    dest: &Path,
    manifest: &crate::models::Manifest,
    inputs: &[PathBuf],
    banner: &Path,
) -> AppResult<()> {
    write_ppf_inputs(dest, manifest, inputs, Some(banner))
}

fn write_ppf_inputs(
    dest: &Path,
    manifest: &crate::models::Manifest,
    inputs: &[PathBuf],
    banner: Option<&Path>,
) -> AppResult<()> {
    reject_input_overlap(dest, inputs)?;
    let selected_banner = match banner {
        Some(path) => {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !path.is_file() || !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
                return Err(AppError::msg(
                    "--banner requires an existing PNG, JPG, JPEG or WebP file",
                ));
            }
            reject_input_overlap(dest, &[path.to_path_buf()])?;
            let canonical = path.canonicalize()?;
            for input in inputs.iter().filter(|p| p.is_file()) {
                let stem = input
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let extension = input
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ["banner", "preview", "thumbnail"].contains(&stem.as_str())
                    && ["png", "jpg", "jpeg", "webp"].contains(&extension.as_str())
                    && input.canonicalize()? != canonical
                {
                    return Err(AppError::msg("Conflicting banner input: remove banner/preview/thumbnail images from positional inputs when using --banner"));
                }
            }
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(32 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.is_empty() || bytes.len() > 32 * 1024 * 1024 {
                return Err(AppError::msg(
                    "--banner image must be non-empty and at most 32 MiB",
                ));
            }
            Some((canonical, format!("banner.{ext}"), bytes))
        }
        None => None,
    };
    atomic_zip(dest, |zip| {
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // manifest.json — omit tags (user-local); `install_as` carries the kind.
        let mut manifest_json = serde_json::to_value(manifest)?;
        if let Some(obj) = manifest_json.as_object_mut() {
            obj.remove("tags");
        }
        zip.start_file("manifest.json", opts)?;
        zip.write_all(serde_json::to_string_pretty(&manifest_json)?.as_bytes())?;

        if let Some((_, name, bytes)) = &selected_banner {
            zip.start_file(name, opts)?;
            zip.write_all(bytes)?;
        }

        for input in inputs {
            if let Some((banner_path, _, _)) = &selected_banner {
                if input.is_file() && input.canonicalize()? == *banner_path {
                    continue;
                }
            }
            if !input.exists() {
                return Err(AppError::msg(format!(
                    "no such file or folder: {}",
                    input.display()
                )));
            }
            add_input(zip, input, opts)?;
        }
        Ok(())
    })
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
                zip.start_file(&name, file_options(opts, entry.path())?)?;
                let bytes = if entry.file_name() == "manifest.json" {
                    strip_tags(entry.path())?
                } else {
                    fs::read(entry.path())?
                };
                zip.write_all(&bytes)?;
            }
        }
    } else {
        let name = input
            .file_name()
            .ok_or_else(|| AppError::msg("invalid file path".to_string()))?
            .to_string_lossy()
            .to_string();
        zip.start_file(&name, file_options(opts, input)?)?;
        let bytes = if input
            .file_name()
            .is_some_and(|name| name == "manifest.json")
        {
            strip_tags(input)?
        } else {
            fs::read(input)?
        };
        zip.write_all(&bytes)?;
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
            zip.start_file(&name, file_options(opts, entry.path())?)?;
            zip.write_all(&strip_tags(entry.path())?)?;
        } else {
            zip.start_file(&name, file_options(opts, entry.path())?)?;
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
    if s.is_empty() {
        "package".to_string()
    } else {
        s
    }
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
    reject_library_overlap(store, dest)?;
    atomic_zip(dest, |zip| {
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

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
            write_distribution(zip, store, pid, &format!("packages/{folder}/"), opts)?;
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

        Ok(())
    })
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
            #[cfg(unix)]
            let mode = entry.unix_mode().unwrap_or(0o644) & 0o111 | 0o644;
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Keep plugin executability, never propagate privileged bits.
                out.set_permissions(fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}

/// Import an extracted bundle zip: import each member package and create the
/// bundle grouping them.
fn import_bundle(
    store: &LibraryStore,
    staging: &Path,
    bundle_path: &Path,
) -> AppResult<Vec<Package>> {
    let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(bundle_path)?)?;
    let field = |k: &str| {
        meta.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let name = {
        let n = field("name");
        if n.is_empty() {
            "Bundle".to_string()
        } else {
            n
        }
    };
    let members = meta
        .get("members")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

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
    let staging = store
        .root()
        .join(".import-staging")
        .join(uuid::Uuid::new_v4().to_string());
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
        let mut id_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
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

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::models::{Manifest, PackageKind};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("archive-export-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
        fn clean_temps(&self) {
            assert!(!fs::read_dir(&self.0).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".pluxel-export-")));
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn manifest() -> Manifest {
        Manifest {
            name: "Test".into(),
            install_as: Some(PackageKind::Script),
            ..Manifest::default()
        }
    }

    #[test]
    fn same_input_and_hard_link_destinations_preserve_source_bytes() {
        let f = Fixture::new();
        let source = f.path("Tool.jsx");
        fs::write(&source, b"original script").unwrap();
        assert!(write_ppf_from_paths(&source, &manifest(), &[source.clone()]).is_err());
        let alias = f.path("alias.ppf");
        fs::hard_link(&source, &alias).unwrap();
        assert!(write_ppf_from_paths(&alias, &manifest(), &[source.clone()]).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"original script");
        assert_eq!(fs::read(&alias).unwrap(), b"original script");
        f.clean_temps();
    }

    #[test]
    fn directory_self_output_is_rejected_before_any_creation() {
        let f = Fixture::new();
        let input = f.path("input");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("Tool.jsx"), b"script").unwrap();
        for existing in [false, true] {
            let dest = input.join("output.ppf");
            if existing {
                fs::write(&dest, b"previous archive").unwrap();
            }
            assert!(write_ppf_from_paths(&dest, &manifest(), &[input.clone()]).is_err());
            if existing {
                assert_eq!(fs::read(&dest).unwrap(), b"previous archive");
            } else {
                assert!(!dest.exists());
            }
            assert_eq!(
                fs::read_dir(&input).unwrap().count(),
                if existing { 2 } else { 1 }
            );
        }
        let alias = f.path("nested-hard-link.ppf");
        fs::hard_link(input.join("Tool.jsx"), &alias).unwrap();
        assert!(write_ppf_from_paths(&alias, &manifest(), &[input]).is_err());
        assert_eq!(fs::read(alias).unwrap(), b"script");
        f.clean_temps();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_aliases_cannot_hide_directory_self_output() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let input = f.path("input");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("Tool.jsx"), b"script").unwrap();
        let alias = f.path("alias");
        symlink(&input, &alias).unwrap();
        assert!(
            write_ppf_from_paths(&alias.join("new.ppf"), &manifest(), &[input.clone()]).is_err()
        );
        let outside = f.path("outside.ppf");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, input.join("output.ppf")).unwrap();
        assert!(
            write_ppf_from_paths(&input.join("output.ppf"), &manifest(), &[input.clone()]).is_err()
        );
        assert_eq!(fs::read(outside).unwrap(), b"outside");
        assert_eq!(fs::read_dir(input).unwrap().count(), 2);
    }

    #[test]
    fn failed_cli_export_preserves_existing_destination_and_removes_temp() {
        let f = Fixture::new();
        let input = f.path("input");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("manifest.json"), [0xff]).unwrap();
        let dest = f.path("output.ppf");
        fs::write(&dest, b"previous archive").unwrap();
        // Invalid UTF-8 in the nested manifest fails after ZIP writing begins.
        assert!(write_ppf_from_paths(&dest, &manifest(), &[input]).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"previous archive");
        f.clean_temps();
        let source = f.path("Tool.jsx");
        fs::write(&source, b"script").unwrap();
        write_ppf_from_paths(&dest, &manifest(), &[source]).unwrap();
        assert!(zip::ZipArchive::new(fs::File::open(dest).unwrap()).is_ok());
        f.clean_temps();
    }

    #[test]
    fn all_library_exports_preserve_old_destination_on_write_failure() {
        let f = Fixture::new();
        let store = LibraryStore::new(f.path("library")).unwrap();
        let source = f.path("Tool.jsx");
        fs::write(&source, b"script").unwrap();
        let mut pkg = crate::importer::import_path(&store, &source).unwrap();
        pkg.banner = Some("bad-banner.png".into());
        fs::create_dir(store.package_dir(&pkg.id).join("bad-banner.png")).unwrap();
        store.upsert(pkg.clone()).unwrap();
        let bundle = Bundle {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Bundle".into(),
            description: String::new(),
            author: String::new(),
            homepage: String::new(),
            banner: Some("bad-banner.png".into()),
            package_ids: vec![pkg.id.clone()],
            created_at: String::new(),
            updated_at: String::new(),
        };
        fs::create_dir_all(store.bundle_dir(&bundle.id).join("bad-banner.png")).unwrap();
        store.upsert_bundle(bundle.clone()).unwrap();
        let dest = f.path("output.ppf");
        fs::write(&dest, b"previous archive").unwrap();
        assert!(export_package(&store, &pkg.id, &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"previous archive");
        assert!(export_bundle(&store, &bundle.id, &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"previous archive");
        assert!(export_library(&store, &[], &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"previous archive");
        f.clean_temps();
    }

    #[test]
    fn all_library_exports_reject_payload_destinations() {
        let f = Fixture::new();
        let store = LibraryStore::new(f.path("library")).unwrap();
        let source = f.path("Tool.jsx");
        fs::write(&source, b"script").unwrap();
        let pkg = crate::importer::import_path(&store, &source).unwrap();
        let bundle = Bundle {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Bundle".into(),
            description: String::new(),
            author: String::new(),
            homepage: String::new(),
            banner: None,
            package_ids: vec![pkg.id.clone()],
            created_at: String::new(),
            updated_at: String::new(),
        };
        store.upsert_bundle(bundle.clone()).unwrap();
        let index = fs::read(store.root().join("library.json")).unwrap();
        let stored = store.files_dir(&pkg.id).join("Tool.jsx");
        let alias = f.path("alias.ppf");
        fs::hard_link(&stored, &alias).unwrap();
        for dest in [
            stored.clone(),
            store.files_dir(&pkg.id).join("new.ppf"),
            store.root().join("library.json"),
            store.bundle_dir(&bundle.id).join("new.ppf"),
            alias,
        ] {
            assert!(export_package(&store, &pkg.id, &dest).is_err());
            assert!(export_bundle(&store, &bundle.id, &dest).is_err());
            assert!(export_library(&store, &[], &dest).is_err());
        }
        assert_eq!(fs::read(stored).unwrap(), b"script");
        assert_eq!(fs::read(store.root().join("library.json")).unwrap(), index);
        f.clean_temps();
    }
}
