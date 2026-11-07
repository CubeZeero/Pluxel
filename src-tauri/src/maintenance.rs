use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::library::LibraryStore;
use crate::models::{InstallRecord, Package, PackageKind};

#[derive(Clone, Serialize, Deserialize)]
struct Fingerprint {
    path: String,
    digest: Option<String>,
}
type Baselines = BTreeMap<String, Vec<Fingerprint>>;

#[derive(Debug, Serialize)]
pub struct IntegrityEntry {
    pub package_id: String,
    pub package_name: String,
    pub record_id: String,
    pub label: String,
    pub path: String,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallConflict {
    pub path: String,
    pub package_name: String,
    pub other_name: String,
    pub reason: String,
}

/// Streaming hashes include sorted relative names and directory markers, so
/// renames, extra files and empty-directory changes are detectable too.
fn digest(path: &Path) -> AppResult<String> {
    let mut hash = Sha256::new();
    let metadata = path.symlink_metadata()?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::msg("リンク先は検証対象外です"));
    }
    let mut paths = if metadata.is_dir() {
        let mut entries = Vec::new();
        for entry in walkdir::WalkDir::new(path).max_depth(128) {
            let entry = entry.map_err(|e| AppError::msg(e.to_string()))?;
            if entries.len() >= 100_000 {
                return Err(AppError::msg("検証対象のファイル数が多すぎます"));
            }
            entries.push(entry.path().to_path_buf());
        }
        entries
    } else {
        vec![path.to_path_buf()]
    };
    paths.sort();
    let mut buffer = [0u8; 64 * 1024];
    for p in paths {
        let meta = p.symlink_metadata()?;
        if meta.file_type().is_symlink() {
            return Err(AppError::msg("リンク先は検証対象外です"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update((meta.permissions().mode() & 0o777).to_le_bytes());
        }
        let relative = if metadata.is_dir() {
            p.strip_prefix(path).unwrap_or(&p)
        } else {
            Path::new("")
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update([if meta.is_dir() { 1 } else { 0 }]);
        if meta.is_file() {
            hash.update(meta.len().to_le_bytes());
            let mut file = fs::File::open(&p)?;
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
        } else if !meta.is_dir() {
            return Err(AppError::msg("通常のファイルではありません"));
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn baselines(store: &LibraryStore) -> AppResult<Baselines> {
    let path = store.root().join("integrity.json");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn remember(store: &LibraryStore, pkg: &Package, record: &InstallRecord) -> AppResult<()> {
    let mut values = baselines(store)?;
    let library = store.load()?;
    let valid: std::collections::HashSet<_> = library
        .packages
        .iter()
        .flat_map(|p| p.installs.iter().map(|r| r.id.clone()))
        .collect();
    values.retain(|key, _| valid.contains(key));
    values.insert(
        record.id.clone(),
        record
            .paths
            .iter()
            .map(|p| Fingerprint {
                path: p.clone(),
                digest: digest(Path::new(p)).ok(),
            })
            .collect(),
    );
    let tmp = store
        .root()
        .join(format!(".integrity-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&tmp, serde_json::to_vec(&values)?)?;
    let result = fs::rename(&tmp, store.root().join("integrity.json"));
    let _ = fs::remove_file(tmp);
    result?;
    let _ = pkg;
    Ok(())
}

pub fn check(store: &LibraryStore, id: Option<&str>) -> AppResult<Vec<IntegrityEntry>> {
    let values = baselines(store)?;
    let lib = store.load()?;
    if let Some(id) = id {
        store.get(id)?;
    }
    let mut entries = Vec::new();
    for pkg in lib
        .packages
        .iter()
        .filter(|p| id.is_none_or(|id| p.id == id))
    {
        for record in &pkg.installs {
            if record.kind == PackageKind::Installer {
                entries.push(IntegrityEntry {
                    package_id: pkg.id.clone(),
                    package_name: pkg.manifest.name.clone(),
                    record_id: record.id.clone(),
                    label: record.label.clone(),
                    path: record.target_dir.clone(),
                    status: "unsupported".into(),
                    detail: String::new(),
                });
            }
            for path in &record.paths {
                let expected = values
                    .get(&record.id)
                    .and_then(|rows| rows.iter().find(|row| row.path == *path))
                    .and_then(|row| row.digest.as_ref());
                let (status, detail) = match digest(Path::new(path)) {
                    Err(AppError::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                        ("missing", String::new())
                    }
                    Err(e) => ("unreadable", e.to_string()),
                    Ok(actual) => match expected {
                        None => ("unverified", String::new()),
                        Some(v) if v == &actual => ("ok", String::new()),
                        _ => ("changed", String::new()),
                    },
                };
                entries.push(IntegrityEntry {
                    package_id: pkg.id.clone(),
                    package_name: pkg.manifest.name.clone(),
                    record_id: record.id.clone(),
                    label: record.label.clone(),
                    path: path.clone(),
                    status: status.into(),
                    detail,
                });
            }
        }
    }
    Ok(entries)
}

fn key(path: &Path) -> PathBuf {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // AE is normally installed on case-insensitive volumes. Conservative
    // matching prevents an ownership bypass through letter case.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return PathBuf::from(path.to_string_lossy().to_lowercase());
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    path
}
fn overlaps(a: &Path, b: &Path) -> bool {
    let (a, b) = (key(a), key(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// This is a read-only preflight: in particular, it never extracts a ZXP.
pub fn conflicts(
    store: &LibraryStore,
    plans: &[(Package, Vec<PathBuf>)],
) -> AppResult<Vec<InstallConflict>> {
    let lib = store.load()?;
    let baseline = baselines(store)?;
    let mut found = Vec::new();
    let mut selected: Vec<(String, String, PathBuf)> = Vec::new();
    for (pkg, paths) in plans {
        for path in paths {
            if let Some((_, name, _)) = selected
                .iter()
                .find(|(id, _, p)| *id != pkg.id && overlaps(path, p))
            {
                found.push(InstallConflict {
                    path: path.display().to_string(),
                    package_name: pkg.manifest.name.clone(),
                    other_name: name.clone(),
                    reason: "selection".into(),
                });
            }
            selected.push((pkg.id.clone(), pkg.manifest.name.clone(), path.clone()));
            let owners: Vec<_> = lib
                .packages
                .iter()
                .filter(|p| {
                    p.id != pkg.id
                        && p.installs
                            .iter()
                            .any(|r| r.paths.iter().any(|p| overlaps(path, Path::new(p))))
                })
                .collect();
            if !owners.is_empty() {
                for owner in owners {
                    found.push(InstallConflict {
                        path: path.display().to_string(),
                        package_name: pkg.manifest.name.clone(),
                        other_name: owner.manifest.name.clone(),
                        reason: "owned".into(),
                    });
                }
            } else if path.symlink_metadata().is_ok() {
                let own = pkg.installs.iter().find_map(|r| {
                    r.paths
                        .iter()
                        .find(|p| key(Path::new(p)) == key(path))
                        .map(|p| (r, p))
                });
                let reason = if let Some((r, p)) = own {
                    let expected = baseline
                        .get(&r.id)
                        .and_then(|rows| rows.iter().find(|row| row.path == *p))
                        .and_then(|row| row.digest.as_ref());
                    match (expected, digest(path)) {
                        (Some(a), Ok(b)) if *a == b => None,
                        _ => Some("modified"),
                    }
                } else {
                    Some("existing")
                };
                if let Some(reason) = reason {
                    found.push(InstallConflict {
                        path: path.display().to_string(),
                        package_name: pkg.manifest.name.clone(),
                        other_name: String::new(),
                        reason: reason.into(),
                    });
                }
            }
        }
    }
    Ok(found)
}

pub fn ensure_uninstall_owned(
    store: &LibraryStore,
    pkg: &Package,
    record: &InstallRecord,
) -> AppResult<()> {
    let lib = store.load()?;
    for other in lib.packages.iter().filter(|p| p.id != pkg.id) {
        if other.installs.iter().flat_map(|r| &r.paths).any(|old| {
            record
                .paths
                .iter()
                .any(|p| overlaps(Path::new(old), Path::new(p)))
        }) {
            return Err(AppError::msg(format!(
                "別のパッケージも使用中のため削除できません: {}",
                other.manifest.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Manifest;
    fn fixture() -> (LibraryStore, Package, PathBuf) {
        let root = std::env::temp_dir().join(format!("pluxel-integrity-{}", uuid::Uuid::new_v4()));
        let store = LibraryStore::new(root.join("vault")).unwrap();
        let path = root.join("installed");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("entry.jsx"), b"original").unwrap();
        let pkg = Package {
            id: uuid::Uuid::new_v4().to_string(),
            kind: PackageKind::Script,
            manifest: Manifest {
                name: "Integrity".into(),
                ..Default::default()
            },
            banner: None,
            files: vec![],
            installs: vec![InstallRecord {
                id: uuid::Uuid::new_v4().to_string(),
                label: "Test".into(),
                kind: PackageKind::Script,
                target_dir: root.to_string_lossy().into(),
                ae: None,
                paths: vec![path.to_string_lossy().into()],
                installed_at: String::new(),
            }],
            added_at: String::new(),
            updated_at: String::new(),
        };
        store.upsert(pkg.clone()).unwrap();
        (store, pkg, path)
    }
    #[test]
    fn detects_unverified_changed_missing_and_added_children() {
        let (store, pkg, path) = fixture();
        assert_eq!(check(&store, None).unwrap()[0].status, "unverified");
        remember(&store, &pkg, &pkg.installs[0]).unwrap();
        assert_eq!(check(&store, Some(&pkg.id)).unwrap()[0].status, "ok");
        fs::write(path.join("entry.jsx"), b"changed!").unwrap();
        assert_eq!(check(&store, None).unwrap()[0].status, "changed");
        fs::write(path.join("entry.jsx"), b"original").unwrap();
        fs::write(path.join("extra.ffx"), b"extra").unwrap();
        assert_eq!(check(&store, None).unwrap()[0].status, "changed");
        fs::remove_file(path.join("extra.ffx")).unwrap();
        assert_eq!(check(&store, None).unwrap()[0].status, "ok");
        fs::remove_dir_all(&path).unwrap();
        assert_eq!(check(&store, None).unwrap()[0].status, "missing");
        let _ = fs::remove_dir_all(store.root().parent().unwrap());
    }
    #[test]
    fn shared_ownership_blocks_uninstall_and_unknown_ids_fail() {
        let (store, pkg, path) = fixture();
        let mut other = pkg.clone();
        other.id = uuid::Uuid::new_v4().to_string();
        store.upsert(other).unwrap();
        assert!(ensure_uninstall_owned(&store, &pkg, &pkg.installs[0]).is_err());
        assert!(check(&store, Some("not-a-real-id")).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
