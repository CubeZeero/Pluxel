//! Local activity and immutable vault revisions. No installed target is touched.
use crate::{
    error::{AppError, AppResult},
    library::LibraryStore,
    models::Package,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

static HISTORY_LOCK: Mutex<()> = Mutex::new(());
static REVISION_LOCK: Mutex<()> = Mutex::new(());
const HISTORY_LIMIT: usize = 1000;
const REVISION_LIMIT: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: String,
    pub at: String,
    pub action: String,
    pub package_id: Option<String>,
    pub name: String,
    pub version: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub version: String,
    pub name: String,
    pub created_at: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize)]
struct Record {
    revision: Revision,
    package: Package,
    inventory: Vec<(String, u64, u64)>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn token(id: &str) -> AppResult<()> {
    if Uuid::parse_str(id)
        .map(|v| v.to_string() == id)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(AppError::msg("Invalid UUID"))
    }
}
fn plain_dir(path: &Path) -> AppResult<()> {
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(AppError::msg("Expected a real directory"));
    }
    Ok(())
}
fn ensure_dir(path: &Path) -> AppResult<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => plain_dir(path),
        Err(e) => Err(e.into()),
    }
}
fn read(path: &Path) -> AppResult<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(AppError::msg("Expected a regular file"));
    }
    Ok(fs::read(path)?)
}
fn atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    #[cfg(test)]
    if path.file_name().is_some_and(|n| n == "library.json")
        && tests::FAIL_INDEX_WRITE.with(|flag| flag.replace(false))
    {
        return Err(AppError::msg("Injected index write failure"));
    }
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        use std::io::Write;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(tmp);
    result
}

// Activity is descriptive text only. Discard messages containing credential
// markers rather than retaining a possibly incomplete redaction.
fn public_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if [
        "password",
        "passwd",
        "secret",
        "token",
        "authorization",
        "bearer ",
        "api_key",
        "api-key",
        "credential",
        "sk-",
        "://",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        "[redacted]".into()
    } else {
        text.chars().take(4096).collect()
    }
}
fn load_history(store: &LibraryStore) -> AppResult<Vec<ActivityEvent>> {
    let path = store.root().join("activity.json");
    let bytes = match read(&path) {
        Ok(b) => b,
        Err(AppError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    match serde_json::from_slice::<Vec<ActivityEvent>>(&bytes) {
        Ok(mut events) => {
            events.truncate(HISTORY_LIMIT);
            Ok(events)
        }
        Err(_) => {
            // Preserve damaged data for inspection, then allow logging to resume.
            fs::rename(
                path,
                store
                    .root()
                    .join(format!("activity.corrupt-{}.json", Uuid::new_v4())),
            )?;
            Ok(vec![])
        }
    }
}
pub fn append(
    store: &LibraryStore,
    action: &str,
    package: Option<&Package>,
    detail: &str,
) -> AppResult<()> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| AppError::msg("History lock poisoned"))?;
    if let Some(p) = package {
        token(&p.id)?;
    }
    let mut events = load_history(store)?;
    events.insert(
        0,
        ActivityEvent {
            id: Uuid::new_v4().to_string(),
            at: now(),
            action: public_text(action),
            package_id: package.map(|p| p.id.clone()),
            name: public_text(package.map(|p| p.manifest.name.as_str()).unwrap_or("")),
            version: public_text(package.map(|p| p.manifest.version.as_str()).unwrap_or("")),
            detail: public_text(detail),
        },
    );
    events.truncate(HISTORY_LIMIT);
    atomic(
        &store.root().join("activity.json"),
        &serde_json::to_vec_pretty(&events)?,
    )
}
pub fn history(store: &LibraryStore) -> AppResult<Vec<ActivityEvent>> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| AppError::msg("History lock poisoned"))?;
    load_history(store)
}

fn revision_root(store: &LibraryStore, id: &str) -> AppResult<PathBuf> {
    token(id)?;
    plain_dir(store.root())?;
    let base = store.root().join(".revisions");
    ensure_dir(&base)?;
    let dir = base.join(id);
    ensure_dir(&dir)?;
    Ok(dir)
}
fn relative(path: &str) -> AppResult<()> {
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path.split('/').any(|s| s == ".revisions" || s.is_empty())
    {
        return Err(AppError::msg("Unsafe package path"));
    }
    Ok(())
}
fn validate_package(p: &Package, id: &str, vault: &Path) -> AppResult<()> {
    if p.id != id {
        return Err(AppError::msg("Revision package mismatch"));
    }
    for f in &p.files {
        relative(&f.rel_path)?;
        let metadata = fs::symlink_metadata(vault.join("files").join(&f.rel_path))?;
        if !metadata.file_type().is_file() || metadata.len() != f.size {
            return Err(AppError::msg("Invalid package file size"));
        }
    }
    if let Some(b) = &p.banner {
        relative(b)?;
        if !fs::symlink_metadata(vault.join(b))?.file_type().is_file() {
            return Err(AppError::msg("Expected a regular banner file"));
        }
    }
    let _: crate::models::Manifest = serde_json::from_slice(&read(&vault.join("manifest.json"))?)?;
    Ok(())
}
// Do not follow links or accept devices, sockets, or archive-supplied histories.
fn copy_tree(src: &Path, dst: &Path) -> AppResult<()> {
    plain_dir(src)?;
    fs::create_dir(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_name() == ".revisions" {
            continue;
        }
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(AppError::msg("Unsafe vault node"));
        }
    }
    Ok(())
}
fn inventory(root: &Path) -> AppResult<Vec<(String, u64, u64)>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64, u64)>) -> AppResult<()> {
        plain_dir(dir)?;
        for e in fs::read_dir(dir)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                walk(root, &e.path(), out)?;
            } else {
                use std::io::Read;
                if !e.file_type()?.is_file() {
                    return Err(AppError::msg("Expected a regular file"));
                }
                let mut file = fs::File::open(e.path())?;
                let mut buffer = [0u8; 64 * 1024];
                let mut hash = 0xcbf29ce484222325u64;
                let mut length = 0u64;
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    length += count as u64;
                    for byte in &buffer[..count] {
                        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
                    }
                }
                out.push((
                    e.path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    length,
                    hash,
                ));
            }
        }
        Ok(())
    }
    let mut out = vec![];
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn snapshot_inner(store: &LibraryStore, pkg: &Package, reason: &str) -> AppResult<Revision> {
    let root = revision_root(store, &pkg.id)?;
    plain_dir(&store.packages_dir())?;
    let revision = Revision {
        id: Uuid::new_v4().to_string(),
        version: pkg.manifest.version.clone(),
        name: pkg.manifest.name.clone(),
        created_at: now(),
        reason: public_text(reason),
    };
    let stage = root.join(format!(".stage-{}", revision.id));
    fs::create_dir(&stage)?;
    let result = (|| {
        let vault = stage.join("vault");
        copy_tree(&store.package_dir(&pkg.id), &vault)?;
        validate_package(pkg, &pkg.id, &vault)?;
        let mut saved = pkg.clone();
        saved.installs.clear();
        let record = Record {
            revision: revision.clone(),
            package: saved,
            inventory: inventory(&vault)?,
        };
        atomic(
            &stage.join("revision.json"),
            &serde_json::to_vec_pretty(&record)?,
        )?;
        fs::rename(&stage, root.join(&revision.id))?;
        Ok(revision)
    })();
    let _ = fs::remove_dir_all(stage);
    result
}
pub fn snapshot(store: &LibraryStore, pkg: &Package, reason: &str) -> AppResult<Revision> {
    let _guard = REVISION_LOCK
        .lock()
        .map_err(|_| AppError::msg("Revision lock poisoned"))?;
    snapshot_inner(store, pkg, reason)
}
fn record(dir: &Path, id: &str, revision_id: &str) -> AppResult<Record> {
    plain_dir(dir)?;
    let r: Record = serde_json::from_slice(&read(&dir.join("revision.json"))?)?;
    if r.revision.id != revision_id || r.package.id != id {
        return Err(AppError::msg("Revision identity mismatch"));
    }
    Ok(r)
}
pub fn revisions(store: &LibraryStore, id: &str) -> AppResult<Vec<Revision>> {
    let _guard = REVISION_LOCK
        .lock()
        .map_err(|_| AppError::msg("Revision lock poisoned"))?;
    let root = revision_root(store, id)?;
    let mut out = vec![];
    for e in fs::read_dir(root)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if token(&name).is_err() {
            continue;
        }
        if let Ok(r) = record(&e.path(), id, &name) {
            out.push(r.revision);
        }
    }
    out.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    out.truncate(REVISION_LIMIT);
    Ok(out)
}

pub fn restore(store: &LibraryStore, id: &str, revision_id: &str) -> AppResult<Package> {
    restore_impl(store, id, revision_id, true)
}

/// Recover a failed mutation from its pre-mutation revision. The damaged
/// current vault is deliberately not snapshotted or inspected for completeness.
pub fn rollback(store: &LibraryStore, id: &str, revision_id: &str) -> AppResult<Package> {
    restore_impl(store, id, revision_id, false)
}

fn restore_impl(
    store: &LibraryStore,
    id: &str,
    revision_id: &str,
    snapshot_current: bool,
) -> AppResult<Package> {
    token(id)?;
    token(revision_id)?;
    let _guard = REVISION_LOCK
        .lock()
        .map_err(|_| AppError::msg("Revision lock poisoned"))?;
    let dir = revision_root(store, id)?.join(revision_id);
    let r = record(&dir, id, revision_id)?;
    let current = store.get(id)?;
    plain_dir(&store.packages_dir())?;
    let live_exists = match fs::symlink_metadata(store.package_dir(id)) {
        Ok(_) => {
            plain_dir(&store.package_dir(id))?;
            true
        }
        Err(e) if !snapshot_current && e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    let nonce = Uuid::new_v4();
    let stage = store.packages_dir().join(format!(".restore-{nonce}"));
    let backup = store.packages_dir().join(format!(".backup-{nonce}"));
    let result = (|| {
        copy_tree(&dir.join("vault"), &stage)?;
        if inventory(&stage)? != r.inventory {
            return Err(AppError::msg("Revision content is damaged"));
        }
        validate_package(&r.package, id, &stage)?;
        let mut restored = r.package;
        restored.installs = current.installs.clone();
        restored.manifest.tags = current.manifest.tags.clone();
        restored.updated_at = now();
        // Keep original manifest bytes whenever no tag merge is necessary.
        let mut manifest: crate::models::Manifest =
            serde_json::from_slice(&read(&stage.join("manifest.json"))?)?;
        if manifest.tags != restored.manifest.tags {
            manifest.tags = restored.manifest.tags.clone();
            atomic(
                &stage.join("manifest.json"),
                &serde_json::to_vec_pretty(&manifest)?,
            )?;
        }
        if snapshot_current {
            snapshot_inner(store, &current, "Before restore (redo)")?;
        }
        let mut library = store.load()?;
        let entry = library
            .packages
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::msg("Package disappeared"))?;
        *entry = restored.clone();
        // Serialize before changing any live directory.
        let index = serde_json::to_vec_pretty(&library)?;
        let live = store.package_dir(id);
        if live_exists {
            fs::rename(&live, &backup)?;
        }
        if let Err(e) = fs::rename(&stage, &live) {
            if live_exists {
                fs::rename(&backup, &live)?;
            }
            return Err(e.into());
        }
        if let Err(e) = atomic(&store.root().join("library.json"), &index) {
            fs::rename(&live, &stage)?;
            if live_exists {
                fs::rename(&backup, &live)?;
            }
            return Err(e);
        }
        let _ = fs::remove_dir_all(&backup);
        Ok(restored)
    })();
    let _ = fs::remove_dir_all(stage);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{InstallRecord, Manifest, PackageFile, PackageKind};

    thread_local! {
        pub(super) static FAIL_INDEX_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    struct Fixture(LibraryStore);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0.root());
        }
    }
    fn fixture() -> Fixture {
        Fixture(
            LibraryStore::new(
                std::env::temp_dir().join(format!("activity-test-{}", Uuid::new_v4())),
            )
            .unwrap(),
        )
    }
    fn package(s: &LibraryStore) -> Package {
        let p = Package {
            id: Uuid::new_v4().to_string(),
            kind: PackageKind::Script,
            manifest: Manifest {
                name: "Example".into(),
                version: "1".into(),
                tags: vec!["old".into()],
                ..Manifest::default()
            },
            banner: Some("banner.png".into()),
            files: vec![PackageFile {
                rel_path: "nested/test.jsx".into(),
                kind: PackageKind::Script,
                size: 4,
            }],
            installs: vec![],
            added_at: now(),
            updated_at: now(),
        };
        fs::create_dir_all(s.files_dir(&p.id).join("nested")).unwrap();
        fs::write(s.files_dir(&p.id).join("nested/test.jsx"), b"old\0").unwrap();
        fs::write(s.package_dir(&p.id).join("banner.png"), [0, 255, 1, 42]).unwrap();
        // Deliberately noncanonical JSON formatting must survive snapshotting.
        fs::write(
            s.package_dir(&p.id).join("manifest.json"),
            "{\"name\":\"Example\",\"version\":\"1\",\"tags\":[\"old\"]}\n",
        )
        .unwrap();
        s.upsert(p.clone()).unwrap();
        p
    }
    #[test]
    fn roundtrip_preserves_current_tags_installs_and_redo() {
        let f = fixture();
        let s = &f.0;
        let mut p = package(s);
        let original_manifest = read(&s.package_dir(&p.id).join("manifest.json")).unwrap();
        let old = snapshot(s, &p, "before update").unwrap();
        let vault = revision_root(s, &p.id).unwrap().join(&old.id).join("vault");
        assert_eq!(
            read(&vault.join("manifest.json")).unwrap(),
            original_manifest
        );
        p.manifest.version = "2".into();
        p.manifest.tags = vec!["current".into()];
        let target = s.root().join("installed.jsx");
        fs::write(&target, b"installed unchanged").unwrap();
        p.installs.push(InstallRecord {
            id: Uuid::new_v4().to_string(),
            label: "custom".into(),
            kind: PackageKind::Script,
            target_dir: s.root().to_string_lossy().into(),
            ae: None,
            paths: vec![target.to_string_lossy().into()],
            installed_at: now(),
        });
        fs::write(s.files_dir(&p.id).join("nested/test.jsx"), b"new!").unwrap();
        fs::write(s.package_dir(&p.id).join("banner.png"), b"new banner").unwrap();
        fs::write(s.package_dir(&p.id).join("extra"), b"new extra").unwrap();
        s.write_manifest(&p.id, &p.manifest).unwrap();
        s.upsert(p.clone()).unwrap();
        let restored = restore(s, &p.id, &old.id).unwrap();
        assert_eq!(restored.manifest.version, "1");
        assert_eq!(restored.manifest.tags, p.manifest.tags);
        assert_eq!(
            serde_json::to_value(&restored.installs).unwrap(),
            serde_json::to_value(&p.installs).unwrap()
        );
        assert_eq!(
            read(&s.files_dir(&p.id).join("nested/test.jsx")).unwrap(),
            b"old\0"
        );
        assert_eq!(
            read(&s.package_dir(&p.id).join("banner.png")).unwrap(),
            [0, 255, 1, 42]
        );
        assert!(!s.package_dir(&p.id).join("extra").exists());
        assert_eq!(read(&target).unwrap(), b"installed unchanged");
        let redo = revisions(s, &p.id)
            .unwrap()
            .into_iter()
            .find(|r| r.id != old.id)
            .unwrap();
        let again = restore(s, &p.id, &redo.id).unwrap();
        assert_eq!(again.manifest.version, "2");
        assert_eq!(
            read(&s.package_dir(&p.id).join("extra")).unwrap(),
            b"new extra"
        );
        assert_eq!(revisions(s, &p.id).unwrap().len(), 3);
    }
    #[test]
    fn empty_version_and_corrupt_revision_leave_current_intact() {
        let f = fixture();
        let s = &f.0;
        let mut p = package(s);
        p.manifest.version.clear();
        s.write_manifest(&p.id, &p.manifest).unwrap();
        s.upsert(p.clone()).unwrap();
        let r = snapshot(s, &p, "unversioned").unwrap();
        assert_eq!(r.version, "");
        assert_eq!(restore(s, &p.id, &r.id).unwrap().manifest.version, "");
        let index = read(&s.root().join("library.json")).unwrap();
        let original = read(&s.files_dir(&p.id).join("nested/test.jsx")).unwrap();
        let dir = revision_root(s, &p.id).unwrap().join(&r.id);
        fs::write(dir.join("vault/files/nested/test.jsx"), b"bad!").unwrap();
        assert!(restore(s, &p.id, &r.id).is_err());
        assert_eq!(read(&s.root().join("library.json")).unwrap(), index);
        assert_eq!(
            read(&s.files_dir(&p.id).join("nested/test.jsx")).unwrap(),
            original
        );
        fs::write(dir.join("revision.json"), b"{").unwrap();
        assert!(restore(s, &p.id, &r.id).is_err());
        assert!(revisions(s, &p.id).unwrap().iter().all(|v| v.id != r.id));
    }
    #[test]
    fn rejects_paths_and_ignores_nested_snapshots() {
        let f = fixture();
        let s = &f.0;
        let mut p = package(s);
        for bad in ["../x", "/tmp", "", "not-a-uuid", "..\\x"] {
            assert!(revisions(s, bad).is_err());
            assert!(restore(s, &p.id, bad).is_err());
            assert!(restore(s, bad, &Uuid::new_v4().to_string()).is_err());
        }
        fs::create_dir(s.package_dir(&p.id).join(".revisions")).unwrap();
        fs::write(s.package_dir(&p.id).join(".revisions/evil"), b"ignored").unwrap();
        let r = snapshot(s, &p, "test").unwrap();
        assert!(!revision_root(s, &p.id)
            .unwrap()
            .join(r.id)
            .join("vault/.revisions")
            .exists());
        p.files[0].rel_path = "../../outside".into();
        assert!(snapshot(s, &p, "test").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_vault_and_revision_root() {
        use std::os::unix::fs::symlink;
        let f = fixture();
        let s = &f.0;
        let p = package(s);
        symlink(s.root(), s.package_dir(&p.id).join("escape")).unwrap();
        assert!(snapshot(s, &p, "test").is_err());
        fs::remove_file(s.package_dir(&p.id).join("escape")).unwrap();
        let r = snapshot(s, &p, "test").unwrap();
        let vault = revision_root(s, &p.id).unwrap().join(&r.id).join("vault");
        fs::remove_file(vault.join("banner.png")).unwrap();
        symlink(s.root().join("library.json"), vault.join("banner.png")).unwrap();
        assert!(restore(s, &p.id, &r.id).is_err());
        fs::remove_dir_all(s.root().join(".revisions")).unwrap();
        symlink(s.packages_dir(), s.root().join(".revisions")).unwrap();
        assert!(snapshot(s, &p, "test").is_err());
    }
    #[test]
    fn history_bound_order_redaction_and_recovery() {
        let f = fixture();
        let s = &f.0;
        for i in 0..1005 {
            append(s, "edit", None, &i.to_string()).unwrap();
        }
        let events = history(s).unwrap();
        assert_eq!(events.len(), 1000);
        assert_eq!(events.first().unwrap().detail, "1004");
        assert_eq!(events.last().unwrap().detail, "5");
        assert!(events.windows(2).all(|w| w[0].at >= w[1].at));
        append(s, "edit", None, "Authorization: Bearer private").unwrap();
        assert!(
            !String::from_utf8(read(&s.root().join("activity.json")).unwrap())
                .unwrap()
                .contains("private")
        );
        fs::write(s.root().join("activity.json"), b"broken").unwrap();
        assert!(history(s).unwrap().is_empty());
        append(s, "recovered", None, "ok").unwrap();
        assert_eq!(history(s).unwrap().len(), 1);
        assert!(fs::read_dir(s.root()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("activity.corrupt-")));
    }
    #[test]
    fn rollback_recovers_partial_mutation_without_snapshotting_damage() {
        let f = fixture();
        let s = &f.0;
        let mut p = package(s);
        let r = snapshot(s, &p, "before mutation").unwrap();
        p.manifest.version = "broken new version".into();
        p.manifest.tags = vec!["keep user tag".into()];
        p.installs.push(InstallRecord {
            id: Uuid::new_v4().to_string(),
            label: "keep install".into(),
            kind: PackageKind::Script,
            target_dir: "/unused".into(),
            ae: None,
            paths: vec![],
            installed_at: now(),
        });
        s.upsert(p.clone()).unwrap();
        fs::remove_dir_all(s.files_dir(&p.id)).unwrap();
        fs::write(s.package_dir(&p.id).join("manifest.json"), b"{partial").unwrap();
        assert!(restore(s, &p.id, &r.id).is_err());
        let restored = rollback(s, &p.id, &r.id).unwrap();
        assert_eq!(restored.manifest.version, "1");
        assert_eq!(restored.manifest.tags, p.manifest.tags);
        assert_eq!(restored.installs[0].id, p.installs[0].id);
        let manifest: Manifest =
            serde_json::from_slice(&read(&s.package_dir(&p.id).join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest.version, "1");
        assert_eq!(manifest.tags, p.manifest.tags);
        assert_eq!(s.get(&p.id).unwrap().manifest.version, "1");
        assert_eq!(revisions(s, &p.id).unwrap().len(), 1);
        // A replacement may have removed the entire package directory.
        fs::remove_dir_all(s.package_dir(&p.id)).unwrap();
        rollback(s, &p.id, &r.id).unwrap();
        assert_eq!(
            read(&s.files_dir(&p.id).join("nested/test.jsx")).unwrap(),
            b"old\0"
        );
        assert_eq!(revisions(s, &p.id).unwrap().len(), 1);
    }

    #[test]
    fn failed_index_write_rolls_back_vault_and_metadata() {
        let f = fixture();
        let s = &f.0;
        let mut p = package(s);
        let r = snapshot(s, &p, "before change").unwrap();
        p.manifest.version = "2".into();
        s.write_manifest(&p.id, &p.manifest).unwrap();
        s.upsert(p.clone()).unwrap();
        fs::write(s.files_dir(&p.id).join("nested/test.jsx"), b"new!").unwrap();
        let before_vault = inventory(&s.package_dir(&p.id)).unwrap();
        let before_index = read(&s.root().join("library.json")).unwrap();
        FAIL_INDEX_WRITE.with(|flag| flag.set(true));
        assert!(restore(s, &p.id, &r.id).is_err());
        assert_eq!(inventory(&s.package_dir(&p.id)).unwrap(), before_vault);
        assert_eq!(read(&s.root().join("library.json")).unwrap(), before_index);
        assert_eq!(s.get(&p.id).unwrap().manifest.version, "2");
        FAIL_INDEX_WRITE.with(|flag| flag.set(true));
        assert!(rollback(s, &p.id, &r.id).is_err());
        assert_eq!(inventory(&s.package_dir(&p.id)).unwrap(), before_vault);
        assert_eq!(read(&s.root().join("library.json")).unwrap(), before_index);
    }

    #[test]
    fn failed_metadata_load_does_not_replace_current_vault() {
        let f = fixture();
        let s = &f.0;
        let p = package(s);
        let r = snapshot(s, &p, "test").unwrap();
        fs::write(s.files_dir(&p.id).join("nested/test.jsx"), b"new!").unwrap();
        fs::write(s.root().join("library.json"), b"{broken").unwrap();
        assert!(rollback(s, &p.id, &r.id).is_err());
        assert_eq!(
            read(&s.files_dir(&p.id).join("nested/test.jsx")).unwrap(),
            b"new!"
        );
        assert_eq!(read(&s.root().join("library.json")).unwrap(), b"{broken");
    }

    #[test]
    fn concurrent_history_updates_are_not_lost() {
        let f = fixture();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let s = &f.0;
                scope.spawn(move || {
                    for _ in 0..10 {
                        append(s, "edit", None, "ok").unwrap();
                    }
                });
            }
        });
        assert_eq!(history(&f.0).unwrap().len(), 80);
    }
}
