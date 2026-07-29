use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::models::{Bundle, Library, Manifest, Package};

/// True when `s` is a single, safe path component: no separators, no `..`,
/// no root/drive prefix, not empty. Used to validate any path fragment that
/// originates from untrusted input (imported archive JSON, IPC args) before it
/// is joined onto a base directory — preventing path traversal.
pub fn is_safe_segment(s: &str) -> bool {
    if s.is_empty() || s.contains('/') || s.contains('\\') {
        return false;
    }
    let mut comps = Path::new(s).components();
    matches!(comps.next(), Some(Component::Normal(_))) && comps.next().is_none()
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
        let mut lib = self.load()?;
        if let Some(existing) = lib.packages.iter_mut().find(|p| p.id == package.id) {
            *existing = package;
        } else {
            lib.packages.push(package);
        }
        self.save(&lib)
    }

    pub fn get(&self, id: &str) -> AppResult<Package> {
        self.load()?
            .packages
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::msg(format!("Package not found: {id}")))
    }

    /// Remove a package from the index and delete its directory.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        let mut lib = self.load()?;
        let before = lib.packages.len();
        lib.packages.retain(|p| p.id != id);
        if lib.packages.len() == before {
            return Err(AppError::msg(format!("Package not found: {id}")));
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
        self.load()?
            .bundles
            .into_iter()
            .find(|b| b.id == id)
            .ok_or_else(|| AppError::msg(format!("Bundle not found: {id}")))
    }

    /// Insert or replace a bundle in the index by id.
    pub fn upsert_bundle(&self, bundle: Bundle) -> AppResult<()> {
        let mut lib = self.load()?;
        if let Some(existing) = lib.bundles.iter_mut().find(|b| b.id == bundle.id) {
            *existing = bundle;
        } else {
            lib.bundles.push(bundle);
        }
        self.save(&lib)
    }

    pub fn remove_bundle(&self, id: &str) -> AppResult<()> {
        let mut lib = self.load()?;
        lib.bundles.retain(|b| b.id != id);
        self.save(&lib)?;
        let dir = self.bundle_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}
