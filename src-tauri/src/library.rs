use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::models::{Bundle, Library, Manifest, Package};

/// True when `s` is a single, safe path component: no separators, no `..`,
/// no root/drive prefix, not empty. Used to validate any path fragment that
/// originates from untrusted input (imported archive JSON, IPC args) before it
/// is joined onto a base directory — preventing path traversal.
pub fn is_safe_segment(s: &str) -> bool {
    if s.is_empty()
        || s.contains('/')
        || s.contains('\\')
        || s.contains(':')
        || s.chars().any(char::is_control)
    {
        return false;
    }
    let mut comps = Path::new(s).components();
    matches!(comps.next(), Some(Component::Normal(_))) && comps.next().is_none()
}

#[cfg(test)]
mod security_crud {
    use super::*;
    use crate::models::PackageKind;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("library-security-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn store(&self) -> LibraryStore {
            LibraryStore::new(self.0.join("vault")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn bundle(id: &str) -> Bundle {
        Bundle {
            id: id.into(),
            name: "Bundle".into(),
            description: String::new(),
            author: String::new(),
            homepage: String::new(),
            banner: None,
            package_ids: vec![],
            created_at: "created".into(),
            updated_at: "updated".into(),
        }
    }
    fn package(id: &str) -> Package {
        Package {
            id: id.into(),
            kind: PackageKind::Script,
            manifest: Manifest::default(),
            banner: None,
            files: vec![],
            installs: vec![],
            added_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn unsafe_ids_cannot_read_write_or_remove_even_if_present_in_index() {
        let f = Fixture::new();
        let store = f.store();
        let outside = f.0.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        let absolute = outside.to_string_lossy().into_owned();
        for id in [
            "",
            ".",
            "..",
            "../outside",
            "a/b",
            "a\\b",
            "C:\\outside",
            "C:outside",
            "\\\\server\\share",
            "bad\n",
            absolute.as_str(),
        ] {
            // A legacy or damaged index must not bypass the CRUD boundary.
            store
                .save(&Library {
                    packages: vec![package(id)],
                    bundles: vec![bundle(id)],
                })
                .unwrap();
            let before = fs::read(store.index_path()).unwrap();
            assert!(store.get(id).is_err(), "package get: {id:?}");
            assert!(store.upsert(package(id)).is_err(), "package upsert: {id:?}");
            assert!(store.remove(id).is_err(), "package remove: {id:?}");
            assert!(
                store.write_manifest(id, &Manifest::default()).is_err(),
                "manifest: {id:?}"
            );
            assert!(store.get_bundle(id).is_err(), "bundle get: {id:?}");
            assert!(
                store.upsert_bundle(bundle(id)).is_err(),
                "bundle upsert: {id:?}"
            );
            assert!(store.remove_bundle(id).is_err(), "bundle remove: {id:?}");
            assert_eq!(fs::read(store.index_path()).unwrap(), before);
            assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
        }
    }

    #[test]
    fn removing_unknown_ids_preserves_unindexed_directories_and_index() {
        let f = Fixture::new();
        let store = f.store();
        store.upsert_bundle(bundle("bid")).unwrap();
        let before = fs::read(store.index_path()).unwrap();
        for dir in [store.bundle_dir("unknown"), store.package_dir("unknown")] {
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("sentinel"), b"keep").unwrap();
        }
        assert!(store.remove_bundle("unknown").is_err());
        assert!(store.remove("unknown").is_err());
        assert_eq!(fs::read(store.index_path()).unwrap(), before);
        assert!(store.bundle_dir("unknown").join("sentinel").exists());
        assert!(store.package_dir("unknown").join("sentinel").exists());
    }

    #[test]
    fn simple_non_uuid_ids_support_crud_and_package_membership_cleanup() {
        let f = Fixture::new();
        let store = f.store();
        store.upsert(package("pid")).unwrap();
        let mut b = bundle("bid");
        b.package_ids.push("pid".into());
        store.upsert_bundle(b).unwrap();
        assert_eq!(store.get("pid").unwrap().id, "pid");
        assert_eq!(store.get_bundle("bid").unwrap().package_ids, vec!["pid"]);
        fs::create_dir_all(store.package_dir("pid")).unwrap();
        store.write_manifest("pid", &Manifest::default()).unwrap();
        store.remove("pid").unwrap();
        assert!(!store.package_dir("pid").exists());
        assert!(store.get_bundle("bid").unwrap().package_ids.is_empty());
        fs::create_dir_all(store.bundle_dir("bid")).unwrap();
        store.remove_bundle("bid").unwrap();
        assert!(!store.bundle_dir("bid").exists());
        assert!(store.load().unwrap().bundles.is_empty());
    }
}

fn validate_id(id: &str) -> AppResult<()> {
    if !is_safe_segment(id) {
        return Err(AppError::msg("Unsafe library id"));
    }
    Ok(())
}

/// Filesystem-backed library store rooted at the Pluxel data directory.
///
/// Layout:
/// ```text
/// <root>/
///   library.json            index of all packages
///   packages/
///     <package-id>/
///       manifest.json        package metadata
///       banner.png|jpg       optional banner
///       files/               the actual .jsx/.plugin/.zxp assets
/// ```
pub struct LibraryStore {
    root: PathBuf,
}

impl LibraryStore {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(root.join("packages"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    pub fn package_dir(&self, id: &str) -> PathBuf {
        self.packages_dir().join(id)
    }

    /// Directory holding a package's actual asset files.
    pub fn files_dir(&self, id: &str) -> PathBuf {
        self.package_dir(id).join("files")
    }

    /// Load the library index, tolerating a missing/empty file (returns empty).
    pub fn load(&self) -> AppResult<Library> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Library::default());
        }
        let data = fs::read_to_string(&path)?;
        if data.trim().is_empty() {
            return Ok(Library::default());
        }
        Ok(serde_json::from_str(&data)?)
    }

    /// Persist the library index atomically (write temp, then rename).
    pub fn save(&self, lib: &Library) -> AppResult<()> {
        let path = self.index_path();
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(lib)?;
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Insert or replace a package in the index by id.
    pub fn upsert(&self, package: Package) -> AppResult<()> {
        validate_id(&package.id)?;
        let mut lib = self.load()?;
        if let Some(existing) = lib.packages.iter_mut().find(|p| p.id == package.id) {
            *existing = package;
        } else {
            lib.packages.push(package);
        }
        self.save(&lib)
    }

    pub fn get(&self, id: &str) -> AppResult<Package> {
        validate_id(id)?;
        self.load()?
            .packages
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::msg(format!("Package not found: {id}")))
    }

    /// Remove a package from the index and delete its directory.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        validate_id(id)?;
        let mut lib = self.load()?;
        let before = lib.packages.len();
        lib.packages.retain(|p| p.id != id);
        if lib.packages.len() == before {
            return Err(AppError::msg(format!("Package not found: {id}")));
        }
        for bundle in &mut lib.bundles {
            bundle.package_ids.retain(|pid| pid != id);
        }
        self.save(&lib)?;
        let dir = self.package_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Persist a package's manifest.json (kept in sync with the index entry).
    pub fn write_manifest(&self, id: &str, manifest: &Manifest) -> AppResult<()> {
        validate_id(id)?;
        let path = self.package_dir(id).join("manifest.json");
        fs::write(path, serde_json::to_string_pretty(manifest)?)?;
        Ok(())
    }

    // ---- Bundles ----

    pub fn bundles_dir(&self) -> PathBuf {
        self.root.join("bundles")
    }

    /// Directory holding a bundle's own assets (e.g. its banner).
    pub fn bundle_dir(&self, id: &str) -> PathBuf {
        self.bundles_dir().join(id)
    }

    pub fn get_bundle(&self, id: &str) -> AppResult<Bundle> {
        validate_id(id)?;
        self.load()?
            .bundles
            .into_iter()
            .find(|b| b.id == id)
            .ok_or_else(|| AppError::msg(format!("Bundle not found: {id}")))
    }

    /// Insert or replace a bundle in the index by id.
    pub fn upsert_bundle(&self, bundle: Bundle) -> AppResult<()> {
        validate_id(&bundle.id)?;
        let mut lib = self.load()?;
        if let Some(existing) = lib.bundles.iter_mut().find(|b| b.id == bundle.id) {
            *existing = bundle;
        } else {
            lib.bundles.push(bundle);
        }
        self.save(&lib)
    }

    pub fn remove_bundle(&self, id: &str) -> AppResult<()> {
        validate_id(id)?;
        let mut lib = self.load()?;
        if !lib.bundles.iter().any(|b| b.id == id) {
            return Err(AppError::msg(format!("Bundle not found: {id}")));
        }
        lib.bundles.retain(|b| b.id != id);
        self.save(&lib)?;
        let dir = self.bundle_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}
