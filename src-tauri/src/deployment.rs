//! Preflighted, recoverable multi-package deployment. Callers resolve AE params first.
use crate::{
    commands::InstallParams,
    error::{AppError, AppResult},
    importer::copy_recursively,
    library::LibraryStore,
    models::{InstallRecord, Package, PackageKind},
};
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

static DEPLOYMENT_LOCK: Mutex<()> = Mutex::new(());

fn key(p: &Path) -> PathBuf {
    // Resolve standard filesystem aliases even when the final child is new.
    let mut existing = p;
    let mut suffix = Vec::new();
    let resolved = loop {
        if let Ok(mut resolved) = existing.canonicalize() {
            for name in suffix.iter().rev() {
                resolved.push(name);
            }
            break resolved;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                suffix.push(name.to_os_string());
                existing = parent;
            }
            _ => break p.to_path_buf(),
        }
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        PathBuf::from(resolved.to_string_lossy().to_lowercase())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        resolved
    }
}
fn overlaps(a: &Path, b: &Path) -> bool {
    let (a, b) = (key(a), key(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// Never follow links, including links nested in plugin bundles.
pub(crate) fn validate_tree(path: &Path) -> AppResult<()> {
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
        let ty = entry.file_type();
        if ty.is_symlink() || !(ty.is_file() || ty.is_dir()) {
            return Err(AppError::msg(format!("Unsafe link or special file: {}. Replace it with a regular file/directory before installing.", entry.path().display())));
        }
    }
    Ok(())
}

pub(crate) fn validate_target(path: &Path) -> AppResult<()> {
    if !path.is_absolute()
        || path.parent().is_none()
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(AppError::msg(format!(
            "Unsafe deployment target: {}",
            path.display()
        )));
    }
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    if [
        "/bin",
        "/sbin",
        "/usr",
        "/etc",
        "/system",
        "/boot",
        "/library/launchdaemons",
        "/library/launchagents",
        "c:/windows",
    ]
    .iter()
    .any(|p| normalized == *p || normalized.starts_with(&format!("{p}/")))
        || [
            "/applications",
            "/users",
            "/library",
            "/private",
            "/tmp",
            "/var",
            "c:/",
            "c:/program files",
        ]
        .contains(&normalized.as_str())
    {
        return Err(AppError::msg(format!(
            "Unsafe deployment root: {}",
            path.display()
        )));
    }
    for parent in path.ancestors() {
        match fs::symlink_metadata(parent) {
            Ok(meta) if meta.file_type().is_symlink() => {
                // Standard macOS system aliases are resolved by the OS, never
                // arbitrary user-controlled links inside a destination.
                #[cfg(target_os = "macos")]
                if [Path::new("/tmp"), Path::new("/var"), Path::new("/etc")].contains(&parent) {
                    continue;
                }
                return Err(AppError::msg(format!(
                    "Destination contains a symbolic link: {}",
                    parent.display()
                )));
            }
            Ok(meta) if parent != path && !meta.is_dir() => {
                return Err(AppError::msg(format!(
                    "Destination parent is not a directory: {}",
                    parent.display()
                )))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    if path.symlink_metadata().is_ok() {
        validate_tree(path)?;
    }
    Ok(())
}

fn remove(path: &Path) -> AppResult<()> {
    validate_target(path)?;
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

// Compare snapshots without following links or loading whole plugin binaries.
fn unchanged(path: &Path, backup: Option<&Path>) -> AppResult<bool> {
    use std::io::Read;
    let Some(backup) = backup else {
        return match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(e) => Err(e.into()),
            Ok(_) => Ok(false),
        };
    };
    if !path.exists() {
        return Ok(false);
    }
    validate_tree(path)?;
    let inventory = |root: &Path| -> AppResult<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in walkdir::WalkDir::new(root).follow_links(false) {
            let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
            paths.push(entry.path().strip_prefix(root).unwrap().to_path_buf());
        }
        paths.sort();
        Ok(paths)
    };
    let paths = inventory(backup)?;
    if inventory(path)? != paths {
        return Ok(false);
    }
    for relative in paths {
        // Joining an empty component appends a separator. For a file root that
        // becomes `file/`, which fails with ENOTDIR and falsely requests recovery.
        let (a, b) = if relative.as_os_str().is_empty() {
            (path.to_path_buf(), backup.to_path_buf())
        } else {
            (path.join(&relative), backup.join(&relative))
        };
        let (ma, mb) = (fs::symlink_metadata(&a)?, fs::symlink_metadata(&b)?);
        if ma.is_dir() != mb.is_dir() || ma.is_file() != mb.is_file() {
            return Ok(false);
        }
        if ma.is_file() {
            if ma.len() != mb.len() {
                return Ok(false);
            }
            let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
            let (mut ab, mut bb) = ([0u8; 65536], [0u8; 65536]);
            loop {
                let n = a.read(&mut ab)?;
                if n == 0 {
                    break;
                }
                b.read_exact(&mut bb[..n])?;
                if ab[..n] != bb[..n] {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

struct Plan {
    pkg: Package,
    record: InstallRecord,
    copies: Vec<(PathBuf, PathBuf)>,
    update: bool,
}

/// Shared CEP targets may still be used by another surviving install record
/// of this same package. An all-AE update replaces all of those records.
fn held_elsewhere(
    library: &crate::models::Library,
    plans: &[Plan],
    owner: &str,
    path: &Path,
) -> bool {
    library.packages.iter().any(|pkg| {
        pkg.installs.iter().any(|record| {
            let survives = pkg.id != owner
                || !plans.iter().any(|plan| {
                    plan.pkg.id == pkg.id
                        && key(Path::new(&plan.record.target_dir))
                            == key(Path::new(&record.target_dir))
                });
            survives
                && record
                    .paths
                    .iter()
                    .any(|old| overlaps(Path::new(old), path))
        })
    })
}

/// Deploy all requests as one file/index transaction. No target writes or ZXP
/// extraction happen until the complete selection has passed conflict checks.
pub fn apply(
    store: &LibraryStore,
    requests: &[(String, InstallParams)],
) -> AppResult<Vec<Package>> {
    let _guard = DEPLOYMENT_LOCK
        .lock()
        .map_err(|_| AppError::msg("Deployment lock poisoned"))?;
    if requests.is_empty() {
        return Ok(vec![]);
    }
    let mut library = store.load()?;
    let mut plans = Vec::new();
    let mut checks = Vec::new();
    let elevated = requests.iter().any(|(_, p)| p.elevated.unwrap_or(false));
    let approved = requests.iter().all(|(_, p)| p.allow_overwrite);
    for (id, params) in requests {
        if !crate::library::is_safe_segment(id) {
            return Err(AppError::msg("Unsafe package id"));
        }
        let pkg = library
            .packages
            .iter()
            .find(|p| p.id == *id)
            .cloned()
            .ok_or_else(|| AppError::msg("Package not found"))?;
        if !pkg.kind.bundleable() {
            return Err(AppError::msg("Use the package's standalone installer"));
        }
        if pkg.kind == PackageKind::Plugin {
            let has_mac = pkg.files.iter().any(|f| {
                f.rel_path
                    .split('/')
                    .any(|p| p.to_ascii_lowercase().ends_with(".plugin"))
            });
            let has_windows = pkg
                .files
                .iter()
                .any(|f| f.rel_path.to_ascii_lowercase().ends_with(".aex"));
            if (cfg!(target_os = "macos") && has_windows && !has_mac)
                || (cfg!(target_os = "windows") && has_mac && !has_windows)
            {
                return Err(AppError::msg("このエフェクトは別のOS向けです。保管はできますが、このOSにはインストールできません。"));
            }
        }
        if let Some(sub) = params
            .effect_subdir
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            if !crate::library::is_safe_segment(sub) {
                return Err(AppError::msg("Unsafe effect subfolder"));
            }
        }
        let custom = params
            .custom_dir
            .as_deref()
            .filter(|s| !s.trim().is_empty());
        if elevated {
            if let Some(dir) = custom {
                crate::paths::reject_privileged_dir(dir)?;
            }
        }
        let primary = params.kind.unwrap_or(pkg.kind);
        let target = match custom {
            Some(dir) => PathBuf::from(dir),
            None => crate::installer::route(
                params
                    .installation
                    .as_ref()
                    .ok_or_else(|| AppError::msg("No installation target"))?,
                primary,
                primary,
                params.effect_subdir.as_deref(),
            ),
        };
        validate_target(&target)?;
        let paths = crate::commands::planned_paths(store, &pkg, params)?;
        let mut copies = Vec::new();
        for src in crate::installer::installable_entries(&store.files_dir(id))? {
            validate_target(&src)?;
            validate_tree(&src)?;
            let dir = match custom {
                Some(d) => PathBuf::from(d),
                None => crate::installer::route(
                    params.installation.as_ref().unwrap(),
                    crate::installer::entry_kind(&src),
                    primary,
                    params.effect_subdir.as_deref(),
                ),
            };
            let name = if crate::installer::entry_kind(&src) == PackageKind::Zxp {
                src.file_stem()
            } else {
                src.file_name()
            }
            .ok_or_else(|| AppError::msg("Invalid source name"))?;
            let dest = dir.join(name);
            validate_target(&dest)?;
            if elevated {
                crate::paths::reject_elevated_destination(&dest.to_string_lossy())?;
            }
            // A target must never overwrite the library, staging, or a source.
            if overlaps(&dest, store.root()) {
                return Err(AppError::msg("Deployment target overlaps the library"));
            }
            copies.push((src, dest));
        }
        let target_dir = target.to_string_lossy().into_owned();
        let update = pkg
            .installs
            .iter()
            .any(|r| key(Path::new(&r.target_dir)) == key(&target));
        let record = InstallRecord {
            id: uuid::Uuid::new_v4().to_string(),
            label: params
                .label
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    custom
                        .map(|d| format!("カスタム: {d}"))
                        .unwrap_or_else(|| params.installation.as_ref().unwrap().name.clone())
                }),
            kind: primary,
            target_dir,
            ae: if custom.is_some() {
                None
            } else {
                params.installation.clone()
            },
            paths: paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            installed_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Some(existing) = plans.iter().find(|p: &&Plan| {
            p.pkg.id == pkg.id && key(Path::new(&p.record.target_dir)) == key(&target)
        }) {
            if existing.copies == copies {
                continue;
            }
            return Err(AppError::msg("PLUXEL_CONFLICT:[{\"reason\":\"selection\",\"path\":\"inconsistent duplicate target\"}]"));
        }
        checks.push((pkg.clone(), paths));
        plans.push(Plan {
            pkg,
            record,
            copies,
            update,
        });
    }
    // Obsolete owned paths are destructive changes too: a modified or
    // unverified old file needs the same explicit approval as an overwrite.
    for (plan, (_, paths)) in plans.iter().zip(checks.iter_mut()) {
        for old in plan
            .pkg
            .installs
            .iter()
            .filter(|r| key(Path::new(&r.target_dir)) == key(Path::new(&plan.record.target_dir)))
            .flat_map(|r| &r.paths)
        {
            let old = PathBuf::from(old);
            if plans
                .iter()
                .flat_map(|p| &p.copies)
                .any(|(_, d)| overlaps(d, &old))
            {
                continue;
            }
            if held_elsewhere(&library, &plans, &plan.pkg.id, &old) {
                continue;
            }
            if !paths.contains(&old) {
                paths.push(old);
            }
        }
    }
    let conflicts = crate::maintenance::conflicts(store, &checks)?;
    if conflicts.iter().any(|c| c.reason == "selection") || (!approved && !conflicts.is_empty()) {
        return Err(AppError::msg(format!(
            "PLUXEL_CONFLICT:{}",
            serde_json::to_string(&conflicts)?
        )));
    }
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    for plan in &plans {
        for pair in &plan.copies {
            if let Some(old) = copies.iter().find(|(_, d)| overlaps(d, &pair.1)) {
                if key(&old.1) == key(&pair.1) && old.0 == pair.0 {
                    continue;
                }
                return Err(AppError::msg("PLUXEL_CONFLICT:[{\"reason\":\"selection\",\"path\":\"overlapping package entries\"}]"));
            }
            copies.push(pair.clone());
        }
    }
    let mut removes = Vec::new();
    for plan in &plans {
        for old in plan
            .pkg
            .installs
            .iter()
            .filter(|r| key(Path::new(&r.target_dir)) == key(Path::new(&plan.record.target_dir)))
            .flat_map(|r| &r.paths)
        {
            let path = PathBuf::from(old);
            if copies.iter().any(|(_, d)| overlaps(d, &path))
                || held_elsewhere(&library, &plans, &plan.pkg.id, &path)
            {
                continue;
            }
            validate_target(&path)?;
            if overlaps(&path, store.root()) {
                return Err(AppError::msg("Obsolete path overlaps library"));
            }
            if elevated {
                crate::paths::reject_elevated_destination(old)?;
            }
            if !removes.contains(&path) {
                removes.push(path);
            }
        }
    }
    // Collapse nested obsolete paths so snapshots/restoration are independent.
    let all_removes = removes.clone();
    removes.retain(|p| {
        !all_removes
            .iter()
            .any(|q| p != q && key(p).starts_with(key(q)))
    });
    let stage = store
        .root()
        .join(format!(".deployment-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage)?;
    let mut backups = Vec::new();
    let mut parents = HashSet::new();
    let prepare = (|| -> AppResult<()> {
        for (i, (src, _)) in copies.iter_mut().enumerate() {
            let staged = stage.join(format!("source-{i}"));
            if crate::installer::entry_kind(src) == PackageKind::Zxp {
                fs::create_dir(&staged)?;
                crate::archive::extract_zip(src, &staged)?;
            } else {
                crate::installer::copy_installable(src, &staged)?;
            }
            validate_tree(&staged)?;
            *src = staged;
        }
        for (i, dest) in copies
            .iter()
            .map(|(_, d)| d)
            .chain(removes.iter())
            .enumerate()
        {
            validate_target(dest)?;
            let backup = stage.join(format!("backup-{i}"));
            let exists = dest.symlink_metadata().is_ok();
            if exists {
                copy_recursively(dest, &backup)?;
            }
            backups.push((dest.clone(), if exists { Some(backup) } else { None }));
            for p in dest.ancestors().skip(1).take_while(|p| !p.exists()) {
                parents.insert(p.to_path_buf());
            }
        }
        Ok(())
    })();
    if let Err(e) = prepare {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    let result = (|| -> AppResult<()> {
        if elevated {
            let clear: Vec<_> = copies
                .iter()
                .map(|(_, d)| d.clone())
                .chain(removes.iter().cloned())
                .collect();
            crate::installer::elevated_apply(&copies, &clear)?;
        } else {
            for (src, dest) in &copies {
                validate_target(dest)?;
                remove(dest)?;
                copy_recursively(src, dest)?;
                #[cfg(test)]
                FAIL_AFTER_COPY.with(|flag| {
                    if flag.replace(false) {
                        Err(AppError::msg("Injected copy failure"))
                    } else {
                        Ok(())
                    }
                })?;
            }
            for path in &removes {
                remove(path)?;
            }
        }
        for plan in &plans {
            let pkg = library
                .packages
                .iter_mut()
                .find(|p| p.id == plan.pkg.id)
                .unwrap();
            pkg.installs.retain(|r| {
                key(Path::new(&r.target_dir)) != key(Path::new(&plan.record.target_dir))
            });
            pkg.installs.push(plan.record.clone());
        }
        // Fold approved transfers into the same atomic index commit.
        if approved {
            for plan in &plans {
                for pkg in library.packages.iter_mut().filter(|p| p.id != plan.pkg.id) {
                    for r in &mut pkg.installs {
                        r.paths.retain(|old| {
                            !plan
                                .record
                                .paths
                                .iter()
                                .any(|new| overlaps(Path::new(old), Path::new(new)))
                        });
                    }
                    pkg.installs
                        .retain(|r| r.kind == PackageKind::Installer || !r.paths.is_empty());
                }
            }
        }
        store.save(&library)
    })();
    if let Err(error) = result {
        let recovery = (|| -> AppResult<()> {
            if backups
                .iter()
                .all(|(d, b)| unchanged(d, b.as_deref()).unwrap_or(false))
            {
                return Ok(());
            }
            #[cfg(test)]
            FAIL_RECOVERY.with(|flag| {
                if flag.replace(false) {
                    Err(AppError::msg("Injected recovery failure"))
                } else {
                    Ok(())
                }
            })?;
            if elevated {
                let restore: Vec<_> = backups
                    .iter()
                    .filter_map(|(d, b)| b.as_ref().map(|b| (b.clone(), d.clone())))
                    .collect();
                let clear: Vec<_> = backups.iter().map(|(d, _)| d.clone()).collect();
                // Clear partial directory merges before restoring the snapshots.
                crate::installer::elevated_apply(&restore, &clear)?;
            } else {
                let mut failures = Vec::new();
                for (dest, backup) in &backups {
                    let r = remove(dest).and_then(|_| match backup {
                        Some(src) => copy_recursively(src, dest),
                        None => Ok(()),
                    });
                    if let Err(e) = r {
                        failures.push(format!("{}: {e}", dest.display()));
                    }
                }
                if !failures.is_empty() {
                    return Err(AppError::msg(failures.join("; ")));
                }
            }
            let mut parents: Vec<_> = parents.into_iter().collect();
            parents.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
            for parent in parents {
                let _ = fs::remove_dir(parent);
            }
            Ok(())
        })();
        return match recovery {
            Ok(()) => { let _ = fs::remove_dir_all(&stage); Err(error) }
            Err(e) => Err(AppError::msg(format!("Deployment failed: {error}. Recovery failed: {e}. Recovery snapshots retained at {}", stage.display()))),
        };
    }
    let _ = fs::remove_dir_all(&stage);
    // The index is committed. Report ancillary failures explicitly as committed
    // installs rather than suggesting that deployment was rolled back.
    for plan in &plans {
        let pkg = library
            .packages
            .iter()
            .find(|p| p.id == plan.pkg.id)
            .unwrap();
        if crate::maintenance::remember(store, pkg, &plan.record).is_err() {
            crate::warnings::push("maintenance.warningBaseline");
        }
        if crate::activity::append(
            store,
            if plan.update { "update" } else { "install" },
            Some(pkg),
            &plan.record.label,
        )
        .is_err()
        {
            crate::warnings::push("maintenance.warningHistory");
        }
    }
    let mut seen = HashSet::new();
    Ok(plans
        .iter()
        .filter(|p| seen.insert(p.pkg.id.clone()))
        .map(|p| {
            library
                .packages
                .iter()
                .find(|v| v.id == p.pkg.id)
                .unwrap()
                .clone()
        })
        .collect())
}

#[cfg(test)]
thread_local! { static FAIL_AFTER_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
thread_local! { static FAIL_RECOVERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Manifest;

    struct Fixture {
        root: PathBuf,
        store: LibraryStore,
        dest: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("deployment-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let store = LibraryStore::new(root.join("library")).unwrap();
            let dest = root.join("target");
            Self { root, store, dest }
        }
        fn package(&self, name: &str) -> Package {
            let pkg = Package {
                id: uuid::Uuid::new_v4().to_string(),
                kind: PackageKind::Script,
                manifest: Manifest {
                    name: name.into(),
                    ..Manifest::default()
                },
                banner: None,
                files: vec![],
                installs: vec![],
                added_at: String::new(),
                updated_at: String::new(),
            };
            fs::create_dir_all(self.store.files_dir(&pkg.id)).unwrap();
            fs::write(self.store.files_dir(&pkg.id).join("test.jsx"), name).unwrap();
            self.store.upsert(pkg.clone()).unwrap();
            pkg
        }
        fn params(&self, approved: bool) -> InstallParams {
            InstallParams {
                installation: None,
                kind: None,
                custom_dir: Some(self.dest.to_string_lossy().into()),
                label: None,
                elevated: Some(false),
                effect_subdir: None,
                allow_overwrite: approved,
            }
        }
        fn install(&self, pkg: &Package, approved: bool) -> AppResult<Vec<Package>> {
            apply(&self.store, &[(pkg.id.clone(), self.params(approved))])
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn custom_target_without_ae_and_directory_contents() {
        let f = Fixture::new();
        let p = f.package("new");
        fs::create_dir_all(f.store.files_dir(&p.id).join("bundle.plugin/Contents")).unwrap();
        fs::write(
            f.store.files_dir(&p.id).join("bundle.plugin/Contents/data"),
            "bundle",
        )
        .unwrap();
        fs::write(f.store.files_dir(&p.id).join("manual.pdf"), "skip").unwrap();
        let result = f.install(&p, false).unwrap();
        assert_eq!(result[0].installs.len(), 1);
        assert_eq!(
            f.dest.join("bundle.plugin/Contents/data").exists(),
            !cfg!(target_os = "windows")
        );
        assert!(!f.dest.join("manual.pdf").exists());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn opposite_platform_effects_and_nested_manuals_are_not_installed() {
        let f = Fixture::new();
        let pkg = f.package("mixed");
        let source = f.store.files_dir(&pkg.id).join("assets");
        fs::create_dir_all(source.join("Mac.plugin/Contents")).unwrap();
        fs::write(source.join("Mac.plugin/Contents/binary"), b"binary").unwrap();
        fs::write(
            source.join("Mac.plugin/Contents/resource.pdf"),
            b"runtime resource",
        )
        .unwrap();
        fs::write(source.join("Windows.aex"), b"aex").unwrap();
        fs::write(source.join("manual.pdf"), b"companion doc").unwrap();
        fs::write(source.join("preset.ffx"), b"preset").unwrap();
        f.install(&pkg, false).unwrap();
        assert!(!f.dest.join("assets/manual.pdf").exists());
        assert!(f.dest.join("assets/preset.ffx").exists());
        assert_eq!(
            f.dest.join("assets/Mac.plugin").exists(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            f.dest.join("assets/Windows.aex").exists(),
            cfg!(target_os = "windows")
        );
        if cfg!(target_os = "macos") {
            assert!(f
                .dest
                .join("assets/Mac.plugin/Contents/resource.pdf")
                .exists());
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn opposite_platform_only_effect_fails_before_any_target_write() {
        let f = Fixture::new();
        let mut pkg = f.package("wrong OS");
        fs::remove_file(f.store.files_dir(&pkg.id).join("test.jsx")).unwrap();
        let name = if cfg!(target_os = "macos") {
            "Only.aex"
        } else {
            "Only.plugin/Contents/binary"
        };
        let file = f.store.files_dir(&pkg.id).join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"binary").unwrap();
        pkg.kind = PackageKind::Plugin;
        pkg.files = vec![crate::models::PackageFile {
            rel_path: name.into(),
            kind: PackageKind::Plugin,
            size: 6,
        }];
        f.store.upsert(pkg.clone()).unwrap();
        assert!(f.install(&pkg, false).is_err());
        assert!(!f.dest.exists());
    }
    #[test]
    fn conflict_leaves_bytes_unchanged() {
        let f = Fixture::new();
        let p = f.package("new");
        fs::create_dir(&f.dest).unwrap();
        fs::write(f.dest.join("test.jsx"), "old").unwrap();
        let index = fs::read(f.store.root().join("library.json")).unwrap();
        assert!(f
            .install(&p, false)
            .unwrap_err()
            .to_string()
            .starts_with("PLUXEL_CONFLICT:"));
        assert_eq!(fs::read(f.dest.join("test.jsx")).unwrap(), b"old");
        assert_eq!(
            fs::read(f.store.root().join("library.json")).unwrap(),
            index
        );
    }
    #[test]
    fn approved_overwrite_moves_ownership() {
        let f = Fixture::new();
        let a = f.package("a");
        let b = f.package("b");
        f.install(&a, false).unwrap();
        f.install(&b, true).unwrap();
        assert!(f.store.get(&a.id).unwrap().installs.is_empty());
        assert_eq!(f.store.get(&b.id).unwrap().installs.len(), 1);
        assert_eq!(fs::read(f.dest.join("test.jsx")).unwrap(), b"b");
    }
    #[test]
    fn missing_and_changed_baselines_need_approval() {
        let f = Fixture::new();
        let p = f.package("a");
        f.install(&p, false).unwrap();
        f.install(&p, false).unwrap(); // unchanged same-package baseline
        fs::write(f.dest.join("test.jsx"), "user edit").unwrap();
        assert!(f.install(&p, false).is_err());
        assert_eq!(fs::read(f.dest.join("test.jsx")).unwrap(), b"user edit");
        f.install(&p, true).unwrap();
        fs::remove_file(f.store.root().join("integrity.json")).unwrap();
        assert!(f.install(&p, false).is_err());
    }
    #[test]
    fn batch_collision_rejected_even_when_approved() {
        let f = Fixture::new();
        let a = f.package("a");
        let b = f.package("b");
        let err = apply(
            &f.store,
            &[
                (a.id.clone(), f.params(true)),
                (b.id.clone(), f.params(true)),
            ],
        )
        .unwrap_err();
        assert!(err.to_string().starts_with("PLUXEL_CONFLICT:"));
        assert!(!f.dest.exists());
    }
    #[test]
    fn obsolete_files_removed_on_update_and_other_targets_preserved() {
        let f = Fixture::new();
        let p = f.package("a");
        f.install(&p, false).unwrap();
        let mut other = f.params(false);
        other.custom_dir = Some(f.root.join("other").to_string_lossy().into());
        apply(&f.store, &[(p.id.clone(), other)]).unwrap();
        fs::rename(
            f.store.files_dir(&p.id).join("test.jsx"),
            f.store.files_dir(&p.id).join("new.jsx"),
        )
        .unwrap();
        f.install(&p, false).unwrap();
        assert!(!f.dest.join("test.jsx").exists());
        assert!(f.dest.join("new.jsx").exists());
        assert!(f.root.join("other/test.jsx").exists());
        assert_eq!(f.store.get(&p.id).unwrap().installs.len(), 2);
    }

    #[test]
    fn shared_cep_is_kept_until_its_last_install_record_is_updated() {
        use std::io::Write;
        let f = Fixture::new();
        let pkg = f.package("shared");
        let files = f.store.files_dir(&pkg.id);
        let zxp = |name: &str| {
            let mut zip = zip::ZipWriter::new(fs::File::create(files.join(name)).unwrap());
            zip.start_file("manifest.xml", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"extension").unwrap();
            zip.finish().unwrap();
        };
        zxp("Old.zxp");
        let ae = |name: &str| crate::models::AeInstallation {
            name: name.into(),
            version: name.into(),
            root: f.root.join(name).to_string_lossy().into(),
            scripts_dir: f.root.join(name).join("Scripts").to_string_lossy().into(),
            scriptui_dir: f.root.join(name).join("Panels").to_string_lossy().into(),
            plugins_dir: f.root.join(name).join("Plugins").to_string_lossy().into(),
            cep_extensions_dir: f.root.join("CEP").to_string_lossy().into(),
        };
        let mut a = f.params(false);
        a.custom_dir = None;
        a.installation = Some(ae("A"));
        let mut b = a.clone();
        b.installation = Some(ae("B"));
        apply(
            &f.store,
            &[(pkg.id.clone(), a.clone()), (pkg.id.clone(), b.clone())],
        )
        .unwrap();
        fs::remove_file(files.join("Old.zxp")).unwrap();
        zxp("New.zxp");
        apply(&f.store, &[(pkg.id.clone(), a)]).unwrap();
        assert!(f.root.join("CEP/Old/manifest.xml").exists());
        assert!(f.root.join("CEP/New/manifest.xml").exists());
        apply(&f.store, &[(pkg.id.clone(), b)]).unwrap();
        assert!(!f.root.join("CEP/Old").exists());
        assert!(f.root.join("CEP/New/manifest.xml").exists());
    }
    #[test]
    fn mid_copy_failure_restores_bytes_and_index() {
        let f = Fixture::new();
        let p = f.package("old");
        f.install(&p, false).unwrap();
        let index = fs::read(f.store.root().join("library.json")).unwrap();
        fs::write(f.store.files_dir(&p.id).join("test.jsx"), "new").unwrap();
        FAIL_AFTER_COPY.with(|flag| flag.set(true));
        assert!(f.install(&p, false).is_err());
        assert_eq!(fs::read(f.dest.join("test.jsx")).unwrap(), b"old");
        assert_eq!(
            fs::read(f.store.root().join("library.json")).unwrap(),
            index
        );
    }
    #[test]
    fn directory_replacement_and_rollback_are_exact() {
        let f = Fixture::new();
        let p = f.package("old");
        let source = f.store.files_dir(&p.id).join("assets");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("old-only"), "old").unwrap();
        f.install(&p, false).unwrap();
        fs::remove_file(source.join("old-only")).unwrap();
        fs::write(source.join("new-only"), "new").unwrap();
        // Inject failure after every copied destination by selecting only the
        // bundle, so the new child is present when recovery begins.
        fs::remove_file(f.store.files_dir(&p.id).join("test.jsx")).unwrap();
        let index = fs::read(f.store.root().join("library.json")).unwrap();
        FAIL_AFTER_COPY.with(|flag| flag.set(true));
        assert!(f.install(&p, false).is_err());
        assert!(f.dest.join("assets/old-only").exists());
        assert!(!f.dest.join("assets/new-only").exists());
        assert_eq!(
            fs::read(f.store.root().join("library.json")).unwrap(),
            index
        );
        f.install(&p, false).unwrap();
        assert!(!f.dest.join("assets/old-only").exists());
        assert!(f.dest.join("assets/new-only").exists());
        assert!(!f.dest.join("test.jsx").exists());
    }
    #[test]
    fn failed_recovery_retains_snapshot_and_old_index() {
        let f = Fixture::new();
        let p = f.package("old");
        f.install(&p, false).unwrap();
        let index = fs::read(f.store.root().join("library.json")).unwrap();
        fs::write(f.store.files_dir(&p.id).join("test.jsx"), "new").unwrap();
        FAIL_AFTER_COPY.with(|flag| flag.set(true));
        FAIL_RECOVERY.with(|flag| flag.set(true));
        let error = f.install(&p, false).unwrap_err().to_string();
        assert!(error.contains("Recovery failed"));
        assert_eq!(
            fs::read(f.store.root().join("library.json")).unwrap(),
            index
        );
        let stage = fs::read_dir(f.store.root())
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.file_name().to_string_lossy().starts_with(".deployment-"))
            .unwrap()
            .path();
        assert!(error.contains(&stage.to_string_lossy().to_string()));
        assert_eq!(fs::read(stage.join("backup-0")).unwrap(), b"old");
    }
    #[test]
    fn snapshot_comparison_handles_file_roots_without_trailing_separator() {
        let f = Fixture::new();
        let original = f.root.join("original.jsxbin");
        let backup = f.root.join("backup");
        fs::write(&original, b"unchanged").unwrap();
        fs::copy(&original, &backup).unwrap();
        assert!(unchanged(&original, Some(&backup)).unwrap());
        fs::write(&original, b"different").unwrap();
        assert!(!unchanged(&original, Some(&backup)).unwrap());
    }
    #[test]
    fn snapshot_comparison_detects_unchanged_and_new_children() {
        let f = Fixture::new();
        let snapshot = f.root.join("snapshot");
        fs::create_dir(&snapshot).unwrap();
        fs::write(snapshot.join("file"), "old").unwrap();
        copy_recursively(&snapshot, &f.dest).unwrap();
        assert!(unchanged(&f.dest, Some(&snapshot)).unwrap());
        fs::write(f.dest.join("new-child"), "new").unwrap();
        assert!(!unchanged(&f.dest, Some(&snapshot)).unwrap());
        assert!(unchanged(&f.root.join("missing"), None).unwrap());
    }
    #[test]
    fn duplicate_same_package_destination_is_deduplicated() {
        let f = Fixture::new();
        let p = f.package("a");
        let result = apply(
            &f.store,
            &[
                (p.id.clone(), f.params(false)),
                (p.id.clone(), f.params(false)),
            ],
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].installs.len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn symlink_subtree_is_rejected() {
        let f = Fixture::new();
        let p = f.package("a");
        fs::create_dir_all(f.dest.join("bundle.plugin")).unwrap();
        std::os::unix::fs::symlink(&f.root, f.dest.join("bundle.plugin/escape")).unwrap();
        fs::create_dir(f.store.files_dir(&p.id).join("bundle.plugin")).unwrap();
        assert!(f.install(&p, true).is_err());
        assert!(!f.dest.join("test.jsx").exists());
    }
}
