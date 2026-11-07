use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};

/// Resolve existing parent links even if the final folder does not yet exist.
fn resolved_destination(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::msg(
            "管理者権限のインストール先には絶対パスを指定してください（.. は使用できません）",
        ));
    }
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match existing.canonicalize() {
            Ok(mut resolved) => {
                for component in suffix.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Do not treat dangling symlinks as ordinary missing folders.
                if existing.symlink_metadata().is_ok() {
                    return Err(AppError::msg("インストール先のリンクを解決できません"));
                }
                suffix.push(
                    existing
                        .file_name()
                        .ok_or_else(|| AppError::msg("インストール先が不正です"))?
                        .to_os_string(),
                );
                existing = existing
                    .parent()
                    .ok_or_else(|| AppError::msg("インストール先が不正です"))?;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub(crate) fn reject_privileged_dir(dir: &str) -> AppResult<()> {
    validate_destination(dir, true)
}

/// Detected AE installations may live under Program Files. Only the custom
/// directory guard bans that whole root; final copies still block OS paths.
pub(crate) fn reject_elevated_destination(dir: &str) -> AppResult<()> {
    validate_destination(dir, false)
}

fn validate_destination(dir: &str, custom: bool) -> AppResult<()> {
    let path = resolved_destination(Path::new(dir))?;
    let norm = path.to_string_lossy().replace('\\', "/");
    // Windows canonical paths carry an extended-length prefix.
    let low = norm
        .trim_start_matches("//?/")
        .trim_end_matches('/')
        .to_ascii_lowercase();
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
    let blocked: &[&str] = if custom {
        &["c:/windows", "c:/program files", "c:/program files (x86)"]
    } else {
        &["c:/windows"]
    };
    #[cfg(not(target_os = "windows"))]
    let _ = custom;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let blocked: &[&str] = &["/bin", "/sbin", "/usr", "/etc", "/boot"];
    if path.parent().is_none()
        || blocked
            .iter()
            .any(|b| low == *b || low.starts_with(&format!("{b}/")))
    {
        return Err(AppError::msg(
            "このフォルダへの管理者権限インストールは許可されていません。",
        ));
    }
    Ok(())
}
