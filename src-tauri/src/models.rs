use serde::{Deserialize, Serialize};

/// The kind of asset a package holds. Determines where it installs inside an
/// After Effects installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageKind {
    /// `.jsx` / `.js` script → Scripts folder.
    Script,
    /// ScriptUI panel (`.jsx` intended for the Window menu) → Scripts/ScriptUI Panels.
    ScriptUiPanel,
    /// `.plugin` / `.aex` compiled effect → Plug-ins folder.
    Plugin,
    /// `.zxp` extension (a signed zip) → CEP extensions folder.
    Zxp,
    /// A native installer (`.pkg` / `.exe` / `.msi`) that is run, not copied.
    Installer,
    /// Anything we could not classify. Installs are user-directed.
    Unknown,
}

impl PackageKind {
    /// Best-effort classification from a file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "jsx" | "js" => PackageKind::Script,
            "jsxbin" => PackageKind::Script,
            "plugin" | "aex" => PackageKind::Plugin,
            "zxp" => PackageKind::Zxp,
            "pkg" | "exe" | "msi" => PackageKind::Installer,
            _ => PackageKind::Unknown,
        }
    }
}

/// A single file belonging to a package, stored relative to the package's
/// `files/` directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageFile {
    /// Path relative to the package `files/` directory (POSIX separators).
    pub rel_path: String,
    pub kind: PackageKind,
    pub size: u64,
}

/// User-editable, aescripts-style metadata for a package. Persisted as
/// `manifest.json` inside each package directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Preferred install target override, e.g. force a `.jsx` into ScriptUI Panels.
    #[serde(default)]
    pub install_as: Option<PackageKind>,
}

/// A record of one install of a package into a target directory, kept so the
/// install can be updated (reinstalled) or uninstalled later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRecord {
    pub id: String,
    /// Human-readable target, e.g. "After Effects 2024".
    pub label: String,
    pub kind: PackageKind,
    pub target_dir: String,
    /// The AE installation this went to (absent for custom-folder installs);
    /// kept so an update can re-run the same extension-based routing.
    #[serde(default)]
    pub ae: Option<AeInstallation>,
    /// Absolute paths that were written, for removal on uninstall.
    pub paths: Vec<String>,
    pub installed_at: String,
}

/// A stored package: metadata plus the set of files it owns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub kind: PackageKind,
    pub manifest: Manifest,
    /// Banner image file name relative to the package dir (e.g. `banner.png`), if any.
    #[serde(default)]
    pub banner: Option<String>,
    pub files: Vec<PackageFile>,
    /// Where this package has been installed (for update / uninstall).
    #[serde(default)]
    pub installs: Vec<InstallRecord>,
    pub added_at: String,
    pub updated_at: String,
}

impl PackageKind {
    /// Whether packages of this kind may be grouped into a bundle / batch
    /// installed: everything that is copy-installed into AE (scripts, ScriptUI
    /// panels, effect plugins, `.zxp`). Native installers are run individually.
    pub fn bundleable(self) -> bool {
        !matches!(self, PackageKind::Installer | PackageKind::Unknown)
    }
}

/// A named group of packages (a "series") — effect plugins / scripts that are
/// installed and distributed together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    /// Banner file name relative to the bundle dir (e.g. `banner.png`), if any.
    #[serde(default)]
    pub banner: Option<String>,
    /// Member package ids, in order.
    #[serde(default)]
    pub package_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The on-disk library index (`library.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub packages: Vec<Package>,
    #[serde(default)]
    pub bundles: Vec<Bundle>,
}

/// A detected After Effects installation and its resolved install targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeInstallation {
    /// Display name, e.g. "After Effects 2024".
    pub name: String,
    /// Year/version label parsed from the folder, e.g. "2024".
    pub version: String,
    /// Root application folder.
    pub root: String,
    /// Scripts folder (may not exist yet).
    pub scripts_dir: String,
    /// Scripts/ScriptUI Panels folder.
    pub scriptui_dir: String,
    /// Plug-ins folder.
    pub plugins_dir: String,
    /// CEP extensions folder for `.zxp` (shared across AE versions, per-user).
    pub cep_extensions_dir: String,
}
