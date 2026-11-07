//! Isolated import transactions. The caller wires this module into the application.
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::library::LibraryStore;
use crate::models::{Bundle, Library, Package};

const MAX_INPUTS: usize = 128;
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
// Serializes preview operations, including stale cleanup and index publication.
// Other library writers must be serialized by the command layer as well.
static OPERATIONS: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize)]
pub struct PreparedImport {
    pub token: String,
    pub packages: Vec<Package>,
    pub bundles: Vec<Bundle>,
}

fn invalid() -> AppError {
    AppError::msg("Invalid or unavailable import preview")
}

fn uuid_segment(value: &str) -> AppResult<()> {
    let id = Uuid::parse_str(value).map_err(|_| invalid())?;
    if id.to_string() != value {
        return Err(invalid());
    }
    Ok(())
}

/// Reject links (including dangling links and linked ancestors) and special files.
/// Like the existing library APIs this assumes local directories are not being
/// concurrently replaced by an adversarial process with filesystem write access.
fn checked(path: &Path) -> AppResult<fs::Metadata> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, Component::ParentDir) {
            return Err(invalid());
        }
        prefix.push(component);
        // A Windows drive/UNC prefix is not an absolute filesystem node until
        // the following RootDir component has been appended.
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let meta = fs::symlink_metadata(&prefix)?;
        if meta.file_type().is_symlink() || (!meta.is_dir() && !meta.is_file()) {
            return Err(invalid());
        }
    }
    Ok(fs::symlink_metadata(path)?)
}

fn tree(path: &Path) -> AppResult<()> {
    let meta = checked(path)?;
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            tree(&entry?.path())?;
        }
    }
    Ok(())
}

/// Copy into exclusively owned destinations; never follow destination links.
fn copy(src: &Path, dst: &Path) -> AppResult<()> {
    if checked(src)?.is_dir() {
        fs::create_dir(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        let mut input = fs::File::open(src)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(dst)?;
        std::io::copy(&mut input, &mut output)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Preserve executable bits needed by .plugin binaries, never
            // propagate setuid/setgid/sticky permissions from an input.
            let mode = fs::metadata(src)?.permissions().mode() & 0o777;
            output.set_permissions(fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn directory(path: &Path) -> AppResult<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if checked(path)?.is_dir() {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        Err(e) => Err(e.into()),
    }
}

fn base(store: &LibraryStore, create: bool) -> AppResult<PathBuf> {
    checked(store.root())?;
    let path = store.root().join(".import-previews");
    if create {
        directory(&path)?;
    } else if !checked(&path)?.is_dir() {
        return Err(invalid());
    }
    Ok(path)
}

fn session(store: &LibraryStore, token: &str) -> AppResult<PathBuf> {
    uuid_segment(token)?;
    let parent = base(store, false)?;
    let path = parent.join(token);
    if !checked(&path)?.is_dir()
        || path.canonicalize()?.parent() != Some(parent.canonicalize()?.as_path())
    {
        return Err(invalid());
    }
    Ok(path)
}

fn cleanup_stale(parent: &Path) -> AppResult<()> {
    // Bound housekeeping work and never traverse or delete linked entries.
    for entry in fs::read_dir(parent)?.take(1024) {
        let entry = entry?;
        if uuid_segment(&entry.file_name().to_string_lossy()).is_err() {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.is_dir()
            && meta
                .modified()
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map(|age| age > MAX_AGE)
                .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

pub fn prepare(store: &LibraryStore, paths: &[String]) -> AppResult<PreparedImport> {
    let _lock = OPERATIONS.lock().map_err(|_| invalid())?;
    if paths.is_empty() || paths.len() > MAX_INPUTS {
        return Err(AppError::msg("Select between 1 and 128 import inputs"));
    }
    let parent = base(store, true)?;
    cleanup_stale(&parent)?;
    let token = Uuid::new_v4().to_string();
    let root = parent.join(&token);
    fs::create_dir(&root)?;
    let result = (|| {
        let staging = LibraryStore::new(root.clone())?;
        let inputs = root.join(".inputs");
        fs::create_dir(&inputs)?;
        for (i, path) in paths.iter().enumerate() {
            let selected = Path::new(path);
            // Follow ordinary ancestor aliases (macOS /var and /tmp), while
            // still rejecting a selected symlink and any links in its content.
            if fs::symlink_metadata(selected)?.file_type().is_symlink() {
                return Err(invalid());
            }
            let source = selected.canonicalize()?;
            tree(&source)?;
            // Prevent recursively copying the preview store into itself.
            if source.is_dir() && root.canonicalize()?.starts_with(source.canonicalize()?) {
                return Err(invalid());
            }
            let slot = inputs.join(i.to_string());
            fs::create_dir(&slot)?;
            let snapshot = slot.join(source.file_name().ok_or_else(invalid)?);
            copy(&source, &snapshot)?;
            match snapshot
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "ppf" | "zip" if snapshot.is_file() => {
                    crate::archive::import_archive(&staging, &snapshot)?;
                }
                _ => {
                    crate::importer::import_path(&staging, &snapshot)?;
                }
            }
        }
        fs::remove_dir_all(inputs)?;
        tree(&root)?;
        let mut library = staging.load()?;
        // A backup's declared list is untrusted. Preview every actual stored
        // payload file, including entries omitted from library.json.
        for pkg in &mut library.packages {
            pkg.files = crate::importer::collect_files(&staging.files_dir(&pkg.id))?;
        }
        staging.save(&library)?;
        if library.packages.is_empty() {
            return Err(invalid());
        }
        // A marker distinguishes complete previews from interrupted preparation.
        fs::write(root.join(".ready"), b"1")?;
        Ok(PreparedImport {
            token,
            packages: library.packages,
            bundles: library.bundles,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&root);
    }
    result
}

fn relative(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || !Path::new(value)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid());
    }
    Ok(())
}

fn validate(library: &Library, staging: &LibraryStore) -> AppResult<()> {
    let mut ids = HashSet::new();
    for p in &library.packages {
        uuid_segment(&p.id)?;
        if !ids.insert(&p.id) || !checked(&staging.package_dir(&p.id))?.is_dir() {
            return Err(invalid());
        }
        let mut actual = crate::importer::collect_files(&staging.files_dir(&p.id))?
            .into_iter()
            .map(|f| (f.rel_path, f.size, f.kind))
            .collect::<Vec<_>>();
        let mut declared = p
            .files
            .iter()
            .map(|f| (f.rel_path.clone(), f.size, f.kind))
            .collect::<Vec<_>>();
        actual.sort_by(|a, b| a.0.cmp(&b.0));
        declared.sort_by(|a, b| a.0.cmp(&b.0));
        if actual != declared {
            return Err(AppError::msg(
                "プレビュー後に同梱ファイルが変更されています。もう一度読み込んでください。",
            ));
        }
        for file in &p.files {
            relative(&file.rel_path)?;
            if !checked(&staging.files_dir(&p.id).join(&file.rel_path))?.is_file() {
                return Err(invalid());
            }
        }
        if let Some(b) = &p.banner {
            if !crate::library::is_safe_segment(b) {
                return Err(invalid());
            }
            if !checked(&staging.package_dir(&p.id).join(b))?.is_file() {
                return Err(invalid());
            }
        }
    }
    let mut bundles = HashSet::new();
    for b in &library.bundles {
        uuid_segment(&b.id)?;
        if !bundles.insert(&b.id) || b.package_ids.iter().any(|id| !ids.contains(id)) {
            return Err(invalid());
        }
        if let Some(banner) = &b.banner {
            if !crate::library::is_safe_segment(banner) {
                return Err(invalid());
            }
            if !checked(&staging.bundle_dir(&b.id).join(banner))?.is_file() {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub fn commit(store: &LibraryStore, token: &str) -> AppResult<Vec<Package>> {
    let _lock = OPERATIONS.lock().map_err(|_| invalid())?;
    let root = session(store, token)?;
    tree(&root)?;
    if fs::read(root.join(".ready"))? != b"1" {
        return Err(invalid());
    }
    let staging = LibraryStore::new(root.clone())?;
    let mut imported = staging.load()?;
    validate(&imported, &staging)?;
    if imported.packages.is_empty() {
        return Err(invalid());
    }
    checked(&store.packages_dir())?;
    let index = store.root().join("library.json");
    if index.symlink_metadata().is_ok() {
        checked(&index)?;
    }
    let mut main = store.load()?;
    let mut created = Vec::new();
    let tmp = store
        .root()
        .join(format!(".preview-index-{}.tmp", Uuid::new_v4()));
    // Consume readiness before publication. Even if post-commit deletion fails,
    // this session can never be published a second time.
    fs::rename(root.join(".ready"), root.join(".committing"))?;
    let result = (|| {
        let mut mapping = HashMap::new();
        for p in &mut imported.packages {
            let old = p.id.clone();
            p.id = Uuid::new_v4().to_string();
            let dst = store.package_dir(&p.id);
            // Reserve first, so rollback only removes directories we own.
            fs::create_dir(&dst)?;
            created.push(dst.clone());
            for entry in fs::read_dir(staging.package_dir(&old))? {
                let entry = entry?;
                copy(&entry.path(), &dst.join(entry.file_name()))?;
            }
            p.installs.clear();
            store.write_manifest(&p.id, &p.manifest)?;
            mapping.insert(old, p.id.clone());
        }
        if !imported.bundles.is_empty() {
            directory(&store.bundles_dir())?;
        }
        for b in &mut imported.bundles {
            let old = b.id.clone();
            b.id = Uuid::new_v4().to_string();
            b.package_ids = b.package_ids.iter().map(|id| mapping[id].clone()).collect();
            if let Some(banner) = &b.banner {
                let dst = store.bundle_dir(&b.id);
                fs::create_dir(&dst)?;
                created.push(dst.clone());
                copy(&staging.bundle_dir(&old).join(banner), &dst.join(banner))?;
            }
        }
        main.packages.extend(imported.packages.clone());
        main.bundles.extend(imported.bundles.clone());
        let mut output = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        output.write_all(&serde_json::to_vec_pretty(&main)?)?;
        output.sync_all()?;
        drop(output);
        fs::rename(&tmp, &index)?;
        Ok(imported.packages)
    })();
    if result.is_err() {
        for dir in created.iter().rev() {
            let _ = fs::remove_dir_all(dir);
        }
        let _ = fs::remove_file(tmp);
        let _ = fs::rename(root.join(".committing"), root.join(".ready"));
    } else {
        // Publication succeeded: cleanup failure must not report a failed commit.
        let _ = fs::remove_dir_all(root);
    }
    result
}

pub fn cancel(store: &LibraryStore, token: &str) -> AppResult<()> {
    let _lock = OPERATIONS.lock().map_err(|_| invalid())?;
    let root = session(store, token)?;
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{InstallRecord, PackageKind};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("preview-test-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn store(&self, name: &str) -> LibraryStore {
            LibraryStore::new(self.0.join(name)).unwrap()
        }
        fn input(&self, name: &str, content: &[u8]) -> String {
            let path = self.0.join(name);
            fs::write(&path, content).unwrap();
            path.to_str().unwrap().to_owned()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn plugin_executable_permissions_survive_preview_and_commit() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let store = f.store("main");
        let plugin = f.0.join("Test.plugin");
        let binary = plugin.join("Contents/MacOS/Test");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, b"binary").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o4755)).unwrap();
        let p = prepare(&store, &[plugin.to_string_lossy().into()]).unwrap();
        let packages = commit(&store, &p.token).unwrap();
        let installed = store
            .files_dir(&packages[0].id)
            .join("Test.plugin/Contents/MacOS/Test");
        assert_eq!(
            fs::metadata(installed).unwrap().permissions().mode() & 0o7777,
            0o755
        );
    }

    #[test]
    fn preview_and_cancel_preserve_main() {
        let f = Fixture::new();
        let store = f.store("main");
        let source = f.input("existing.jsx", b"old");
        crate::importer::import_path(&store, Path::new(&source)).unwrap();
        let before = fs::read(store.root().join("library.json")).unwrap();
        let dirs = fs::read_dir(store.packages_dir()).unwrap().count();
        let p = prepare(&store, &[f.input("new.jsx", b"new")]).unwrap();
        assert_eq!(p.packages.len(), 1);
        assert!(p.bundles.is_empty());
        assert!(serde_json::to_value(&p).unwrap().get("token").is_some());
        assert_eq!(fs::read(store.root().join("library.json")).unwrap(), before);
        assert_eq!(fs::read_dir(store.packages_dir()).unwrap().count(), dirs);
        cancel(&store, &p.token).unwrap();
        assert!(!store
            .root()
            .join(".import-previews")
            .join(&p.token)
            .exists());
        assert!(commit(&store, &p.token).is_err());
        assert_eq!(fs::read(store.root().join("library.json")).unwrap(), before);
    }

    #[test]
    fn preview_shows_undeclared_payload_and_rejects_changes_after_preview() {
        let f = Fixture::new();
        let source = f.store("source");
        let target = f.store("target");
        let input = f.input("safe.jsx", b"safe");
        let pkg = crate::importer::import_path(&source, Path::new(&input)).unwrap();
        // Deliberately leave the index stale, as a crafted backup could.
        fs::write(source.files_dir(&pkg.id).join("hidden.jsx"), b"hidden").unwrap();
        let backup = f.0.join("backup.ppf");
        crate::archive::export_library(&source, &[], &backup).unwrap();
        let prepared = prepare(&target, &[backup.to_string_lossy().into()]).unwrap();
        assert!(prepared.packages[0]
            .files
            .iter()
            .any(|file| file.rel_path == "hidden.jsx"));
        let staged = target
            .root()
            .join(".import-previews")
            .join(&prepared.token)
            .join("packages")
            .join(&prepared.packages[0].id)
            .join("files");
        fs::write(staged.join("added-after-preview.jsx"), b"changed").unwrap();
        assert!(commit(&target, &prepared.token).is_err());
        assert!(target.load().unwrap().packages.is_empty());
        cancel(&target, &prepared.token).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accepts_standard_temporary_directory_alias_as_input() {
        let f = Fixture::new();
        let store = f.store("main");
        let actual = f.input("alias.jsx", b"alias");
        let temp = std::env::temp_dir();
        let alias = temp.join(
            Path::new(&actual)
                .strip_prefix(temp.canonicalize().unwrap())
                .unwrap(),
        );
        let prepared = prepare(&store, &[alias.to_string_lossy().into()]).unwrap();
        assert_eq!(prepared.packages.len(), 1);
        cancel(&store, &prepared.token).unwrap();
    }

    #[test]
    fn mixed_invalid_inputs_roll_back_and_count_is_bounded() {
        let f = Fixture::new();
        let store = f.store("main");
        let valid = f.input("valid.jsx", b"valid");
        let invalid = f.input("invalid.zip", b"not a zip");
        assert!(prepare(&store, &[valid.clone(), invalid]).is_err());
        assert!(!store.root().join("library.json").exists());
        assert_eq!(fs::read_dir(store.packages_dir()).unwrap().count(), 0);
        assert_eq!(
            fs::read_dir(store.root().join(".import-previews"))
                .unwrap()
                .count(),
            0
        );
        assert!(prepare(&store, &[]).is_err());
        assert!(prepare(&store, &vec![valid; MAX_INPUTS + 1]).is_err());
    }

    #[test]
    fn commit_uses_snapshot_after_source_changes_and_deletion() {
        let f = Fixture::new();
        let store = f.store("main");
        let a = f.input("a.jsx", b"original a");
        let b = f.input("b.jsx", b"original b");
        let p = prepare(&store, &[a.clone(), b.clone()]).unwrap();
        fs::write(&a, b"changed").unwrap();
        fs::remove_file(&b).unwrap();
        let committed = commit(&store, &p.token).unwrap();
        assert_eq!(committed.len(), 2);
        for (pkg, expected) in committed.iter().zip([b"original a", b"original b"]) {
            assert_eq!(
                fs::read(store.files_dir(&pkg.id).join(&pkg.files[0].rel_path)).unwrap(),
                expected
            );
            assert!(pkg.installs.is_empty());
        }
        assert!(commit(&store, &p.token).is_err());
        assert_eq!(store.load().unwrap().packages.len(), 2);
    }

    #[test]
    fn commit_backup_remaps_bundle_members_and_preserves_metadata_and_banners() {
        let f = Fixture::new();
        let source = f.store("source");
        let mut ids = Vec::new();
        for name in ["a.jsx", "b.jsx"] {
            let path = f.input(name, name.as_bytes());
            let mut pkg = crate::importer::import_path(&source, Path::new(&path)).unwrap();
            pkg.manifest.author = "Author".into();
            pkg.installs.push(InstallRecord {
                id: "install".into(),
                label: "old machine".into(),
                kind: PackageKind::Script,
                target_dir: "/old".into(),
                ae: None,
                paths: vec!["/old/a.jsx".into()],
                installed_at: "then".into(),
            });
            source.upsert(pkg.clone()).unwrap();
            ids.push(pkg.id);
        }
        let bid = Uuid::new_v4().to_string();
        fs::create_dir_all(source.bundle_dir(&bid)).unwrap();
        fs::write(source.bundle_dir(&bid).join("banner.png"), b"banner").unwrap();
        source
            .upsert_bundle(Bundle {
                id: bid,
                name: "Series".into(),
                description: "Description".into(),
                author: "Author".into(),
                homepage: "home".into(),
                banner: Some("banner.png".into()),
                package_ids: ids,
                created_at: "then".into(),
                updated_at: "then".into(),
            })
            .unwrap();
        let archive = f.0.join("backup.ppf");
        crate::archive::export_library(&source, &[], &archive).unwrap();
        let store = f.store("main");
        let p = prepare(&store, &[archive.to_str().unwrap().into()]).unwrap();
        fs::remove_file(archive).unwrap();
        let packages = commit(&store, &p.token).unwrap();
        let lib = store.load().unwrap();
        assert_eq!(lib.bundles.len(), 1);
        let bundle = &lib.bundles[0];
        assert_eq!(
            bundle.package_ids,
            packages.iter().map(|p| p.id.clone()).collect::<Vec<_>>()
        );
        assert_ne!(bundle.id, p.bundles[0].id);
        assert_eq!(bundle.description, "Description");
        assert_eq!(
            fs::read(store.bundle_dir(&bundle.id).join("banner.png")).unwrap(),
            b"banner"
        );
        for (pkg, staged) in packages.iter().zip(&p.packages) {
            assert_ne!(pkg.id, staged.id);
            assert!(pkg.installs.is_empty());
            assert_eq!(pkg.manifest.author, "Author");
        }
    }

    #[test]
    fn rejects_tampered_tokens_and_metadata_without_main_writes() {
        let f = Fixture::new();
        let store = f.store("main");
        let p = prepare(&store, &[f.input("a.jsx", b"a")]).unwrap();
        for token in [
            "..",
            "../packages",
            "/tmp",
            "not-a-uuid",
            &format!("{}/../{}", p.token, p.token),
        ] {
            assert!(commit(&store, token).is_err());
            assert!(cancel(&store, token).is_err());
        }
        let root = session(&store, &p.token).unwrap();
        let staging = LibraryStore::new(root).unwrap();
        let mut lib = staging.load().unwrap();
        lib.packages[0].files[0].rel_path = "../../outside".into();
        staging.save(&lib).unwrap();
        assert!(commit(&store, &p.token).is_err());
        assert!(!store.root().join("library.json").exists());
        cancel(&store, &p.token).unwrap();
    }

    #[test]
    fn failed_publication_removes_promoted_directories_and_preserves_index() {
        let f = Fixture::new();
        let store = f.store("main");
        let p = prepare(&store, &[f.input("a.jsx", b"a")]).unwrap();
        // A non-directory bundles destination fails after package promotion.
        let staging = LibraryStore::new(session(&store, &p.token).unwrap()).unwrap();
        let mut lib = staging.load().unwrap();
        lib.bundles.push(Bundle {
            id: Uuid::new_v4().to_string(),
            name: "bundle".into(),
            description: String::new(),
            author: String::new(),
            homepage: String::new(),
            banner: None,
            package_ids: vec![lib.packages[0].id.clone()],
            created_at: String::new(),
            updated_at: String::new(),
        });
        staging.save(&lib).unwrap();
        store.save(&Library::default()).unwrap();
        let before = fs::read(store.root().join("library.json")).unwrap();
        fs::write(store.bundles_dir(), b"blocked").unwrap();
        assert!(commit(&store, &p.token).is_err());
        assert_eq!(fs::read_dir(store.packages_dir()).unwrap().count(), 0);
        assert_eq!(fs::read(store.root().join("library.json")).unwrap(), before);
        assert!(session(&store, &p.token).is_ok());
        fs::remove_file(store.bundles_dir()).unwrap();
        commit(&store, &p.token).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_sources_sessions_and_promotion() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let store = f.store("main");
        let original = f.input("a.jsx", b"a");
        let linked = f.0.join("linked.jsx");
        symlink(&original, &linked).unwrap();
        assert!(prepare(&store, &[linked.to_str().unwrap().into()]).is_err());
        let p = prepare(&store, &[original]).unwrap();
        let root = session(&store, &p.token).unwrap();
        let asset = root
            .join("packages")
            .join(&p.packages[0].id)
            .join("files/a.jsx");
        fs::remove_file(&asset).unwrap();
        symlink(f.0.join("a.jsx"), &asset).unwrap();
        assert!(commit(&store, &p.token).is_err());
        cancel(&store, &p.token).unwrap();
        let fake = Uuid::new_v4().to_string();
        symlink(&f.0, store.root().join(".import-previews").join(&fake)).unwrap();
        assert!(commit(&store, &fake).is_err());
        assert!(cancel(&store, &fake).is_err());
        assert!(f.0.join("a.jsx").exists());
    }

    #[test]
    fn zip_bundle_and_ppf_distribution_import_together() {
        let f = Fixture::new();
        let source = f.store("source");
        let path = f.input("a.jsx", b"asset");
        let pkg = crate::importer::import_path(&source, Path::new(&path)).unwrap();
        let bundle = Bundle {
            id: Uuid::new_v4().to_string(),
            name: "Series".into(),
            description: "desc".into(),
            author: String::new(),
            homepage: String::new(),
            banner: None,
            package_ids: vec![pkg.id.clone()],
            created_at: String::new(),
            updated_at: String::new(),
        };
        source.upsert_bundle(bundle.clone()).unwrap();
        let zip = f.0.join("series.zip");
        crate::archive::export_bundle(&source, &bundle.id, &zip).unwrap();
        let ppf = f.0.join("single.ppf");
        crate::archive::export_package(&source, &pkg.id, &ppf).unwrap();
        let store = f.store("main");
        let p = prepare(
            &store,
            &[zip.to_str().unwrap().into(), ppf.to_str().unwrap().into()],
        )
        .unwrap();
        assert_eq!(p.packages.len(), 2);
        assert_eq!(p.bundles.len(), 1);
        // Exercise commit's own install clearing, independently of archive import.
        let staging = LibraryStore::new(session(&store, &p.token).unwrap()).unwrap();
        let mut lib = staging.load().unwrap();
        lib.packages[0].installs.push(InstallRecord {
            id: "old".into(),
            label: "old".into(),
            kind: PackageKind::Script,
            target_dir: "/old".into(),
            ae: None,
            paths: vec![],
            installed_at: String::new(),
        });
        staging.save(&lib).unwrap();
        fs::remove_file(zip).unwrap();
        fs::remove_file(ppf).unwrap();
        let committed = commit(&store, &p.token).unwrap();
        assert!(committed.iter().all(|p| p.installs.is_empty()));
        let bundle = &store.load().unwrap().bundles[0];
        assert_eq!(bundle.package_ids, vec![committed[0].id.clone()]);
    }

    #[test]
    fn stale_cleanup_removes_only_expired_uuid_directories() {
        let f = Fixture::new();
        let store = f.store("main");
        let parent = base(&store, true).unwrap();
        let stale = parent.join(Uuid::new_v4().to_string());
        let recent = parent.join(Uuid::new_v4().to_string());
        let unrelated = parent.join("unrelated");
        for dir in [&stale, &recent, &unrelated] {
            fs::create_dir(dir).unwrap();
        }
        let old = SystemTime::now() - MAX_AGE - Duration::from_secs(60);
        for dir in [&stale, &unrelated] {
            #[cfg(windows)]
            let file = {
                use std::os::windows::fs::OpenOptionsExt;
                // Directory handles need BACKUP_SEMANTICS; request only the
                // attribute permission used by SetFileTime.
                fs::OpenOptions::new()
                    .access_mode(0x100)
                    .custom_flags(0x02000000)
                    .open(dir)
                    .unwrap()
            };
            #[cfg(not(windows))]
            let file = fs::File::open(dir).unwrap();
            file.set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        let p = prepare(&store, &[f.input("a.jsx", b"a")]).unwrap();
        assert!(!stale.exists());
        assert!(recent.exists());
        assert!(unrelated.exists());
        cancel(&store, &p.token).unwrap();
    }
}
