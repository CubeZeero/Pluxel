use std::path::{Path, PathBuf};

use crate::models::{AeInstallation, PackageKind};

/// Per-user CEP extensions directory where `.zxp` extensions are unpacked.
/// Shared across After Effects versions.
fn cep_extensions_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        // ~/Library/Application Support/Adobe/CEP/extensions
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Application Support/Adobe/CEP/extensions")
    }
    #[cfg(target_os = "windows")]
    {
        // %APPDATA%\Adobe\CEP\extensions
        dirs::config_dir()
            .unwrap_or_default()
            .join("Adobe")
            .join("CEP")
            .join("extensions")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        dirs::home_dir().unwrap_or_default().join(".adobe/CEP/extensions")
    }
}

/// Parse a version/year label out of an AE folder name such as
/// "Adobe After Effects 2024" -> "2024".
fn parse_version(folder_name: &str) -> String {
    folder_name
        .rsplit(' ')
        .next()
        .filter(|s| s.chars().any(|c| c.is_ascii_digit()))
        .unwrap_or(folder_name)
        .to_string()
}

/// Build an [`AeInstallation`] from a discovered AE root folder, resolving the
/// platform-specific Scripts / Plug-ins locations.
fn installation_from_root(root: &Path) -> Option<AeInstallation> {
    let folder_name = root.file_name()?.to_string_lossy().to_string();
    let version = parse_version(&folder_name);

    // On macOS the Scripts/Plug-ins folders live at the top of the AE folder.
    // On Windows they live under "Support Files".
    #[cfg(target_os = "windows")]
    let base = root.join("Support Files");
    #[cfg(not(target_os = "windows"))]
    let base = root.to_path_buf();

    let scripts_dir = base.join("Scripts");
    let scriptui_dir = scripts_dir.join("ScriptUI Panels");
    let plugins_dir = base.join("Plug-ins");

    Some(AeInstallation {
        name: folder_name,
        version,
        root: root.to_string_lossy().to_string(),
        scripts_dir: scripts_dir.to_string_lossy().to_string(),
        scriptui_dir: scriptui_dir.to_string_lossy().to_string(),
        plugins_dir: plugins_dir.to_string_lossy().to_string(),
        cep_extensions_dir: cep_extensions_dir().to_string_lossy().to_string(),
    })
}

/// Root directories to scan for "Adobe After Effects *" installations.
fn search_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/Applications")]
    }
    #[cfg(target_os = "windows")]
    {
        let mut roots = Vec::new();
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(p) = std::env::var(var) {
                roots.push(PathBuf::from(p).join("Adobe"));
            }
        }
        if roots.is_empty() {
            roots.push(PathBuf::from(r"C:\Program Files\Adobe"));
        }
        roots
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// Discover installed After Effects versions and their install targets.
pub fn detect_installations() -> Vec<AeInstallation> {
    let mut out = Vec::new();
    for search_root in search_roots() {
        let Ok(entries) = std::fs::read_dir(&search_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("Adobe After Effects") {
                if let Some(install) = installation_from_root(&path) {
                    out.push(install);
                }
            }
        }
    }
    // Newest first (folder names sort naturally by year).
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

/// Resolve the target directory inside an installation for a given package kind.
pub fn target_dir_for(install: &AeInstallation, kind: PackageKind) -> PathBuf {
    match kind {
        PackageKind::Script | PackageKind::Unknown => PathBuf::from(&install.scripts_dir),
        PackageKind::ScriptUiPanel => PathBuf::from(&install.scriptui_dir),
        PackageKind::Plugin => PathBuf::from(&install.plugins_dir),
        PackageKind::Zxp => PathBuf::from(&install.cep_extensions_dir),
        // Installers are launched, not copied; Scripts is a harmless fallback.
        PackageKind::Installer => PathBuf::from(&install.scripts_dir),
    }
}
