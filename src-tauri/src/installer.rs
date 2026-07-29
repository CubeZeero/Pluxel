use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::ae;
use crate::error::{AppError, AppResult};
use crate::importer::copy_recursively;
use crate::library::LibraryStore;
use crate::models::{AeInstallation, Package, PackageKind};

/// Process-lifetime cache of the admin password used for elevated installs, so
/// the user authenticates once per launch (macOS) instead of on every op. Held
/// only in memory — never written to disk — and cleared on quit/lock.
static ADMIN_PW: OnceLock<Mutex<Option<String>>> = OnceLock::new();
fn admin_pw_cell() -> &'static Mutex<Option<String>> {
    ADMIN_PW.get_or_init(|| Mutex::new(None))
}

/// Store (or clear) the session admin password.
pub fn set_session_password(pw: Option<String>) {
    if let Ok(mut g) = admin_pw_cell().lock() {
        *g = pw;
    }
}

/// Whether a session admin password is currently cached.
pub fn has_session_password() -> bool {
    admin_pw_cell().lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Validate an admin password by running a harmless elevated no-op with it, and
/// cache it on success. macOS only; on other platforms this is a no-op (Windows
/// elevation is UAC-consent based and cannot be cached).
#[cfg(target_os = "macos")]
pub fn unlock_session(password: &str) -> AppResult<()> {
    let user = std::env::var("USER").unwrap_or_default();
    let apple = format!(
        "do shell script \"/usr/bin/true\" user name \"{}\" password \"{}\" with administrator privileges",
        as_applescript(&user),
        as_applescript(password),
    );
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&apple)
        .output()?;
    if out.status.success() {
        set_session_password(Some(password.to_string()));
        Ok(())
    } else {
        Err(AppError::msg("管理者パスワードが正しくありません。"))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn unlock_session(_password: &str) -> AppResult<()> {
    Ok(())
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Classify a single stored entry (file or bundle folder) by its extension.
pub(crate) fn entry_kind(path: &Path) -> PackageKind {
    PackageKind::from_extension(&ext_lower(path))
}

/// Extract a `.zxp` (a zip archive) into `<target>/<stem>/` and return the
/// created extension directory.
fn install_zxp(zxp: &Path, target: &Path) -> AppResult<String> {
    let stem = zxp
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "extension".to_string());
    let dest = target.join(&stem);
    fs::create_dir_all(&dest)?;
    crate::archive::extract_zip(zxp, &dest)?;
    Ok(dest.to_string_lossy().to_string())
}

/// Build a clear, actionable write error with the offending path and a hint
/// about permissions (Permission denied / os error 13 is common when a target
/// folder such as an app's Plug-ins directory requires elevated privileges).
fn write_err(path: &Path, e: impl std::fmt::Display) -> AppError {
    let s = e.to_string();
    let hint = if s.contains("Permission denied") || s.contains("os error 13") {
        "書き込み権限がありません。管理者権限が必要な場所の可能性があります。別のフォルダを指定するか、フォルダの権限を確認してください。"
    } else {
        "書き込めませんでした。"
    };
    AppError::msg(format!("{}\n{}\n({e})", hint, path.display()))
}

/// Install one stored entry into `target_dir`: `.zxp` is extracted, everything
/// else (files, `.plugin` bundles, companion folders) is copied verbatim.
fn install_entry(src: &Path, target_dir: &Path) -> AppResult<String> {
    fs::create_dir_all(target_dir).map_err(|e| write_err(target_dir, e))?;
    if ext_lower(src) == "zxp" {
        install_zxp(src, target_dir).map_err(|e| write_err(target_dir, e))
    } else {
        let name = src.file_name().unwrap_or_default();
        let dest = target_dir.join(name);
        copy_recursively(src, &dest).map_err(|e| write_err(&dest, e))?;
        Ok(dest.to_string_lossy().to_string())
    }
}

/// Install every stored entry into a single `target_dir` (used for a
/// user-chosen custom folder). Returns the absolute paths written.
pub fn install_to(store: &LibraryStore, package: &Package, target_dir: &Path) -> AppResult<Vec<String>> {
    let files_dir = store.files_dir(&package.id);
    let mut written = Vec::new();
    for entry in fs::read_dir(&files_dir)? {
        written.push(install_entry(&entry?.path(), target_dir)?);
    }
    Ok(written)
}

/// Resolve the AE folder an entry of `kind` belongs in, honoring a `primary`
/// override (e.g. a script package marked as a ScriptUI Panel) and routing
/// unrecognized companion files alongside the primary asset. `effect_subdir`
/// (when set) nests effect plugins into a `Plug-ins/<subdir>/` subfolder.
pub(crate) fn route(
    install: &AeInstallation,
    kind: PackageKind,
    primary: PackageKind,
    effect_subdir: Option<&str>,
) -> std::path::PathBuf {
    let resolved = match kind {
        PackageKind::Zxp => PackageKind::Zxp,
        PackageKind::Plugin => PackageKind::Plugin,
        PackageKind::ScriptUiPanel => PackageKind::ScriptUiPanel,
        // A .jsx follows the primary override when it targets ScriptUI Panels.
        PackageKind::Script if primary == PackageKind::ScriptUiPanel => PackageKind::ScriptUiPanel,
        PackageKind::Script => PackageKind::Script,
        // Installers aren't copied into AE; treated as companion if encountered.
        PackageKind::Installer => primary,
        // Companion files (.ffx, images, generic folders) follow the primary.
        PackageKind::Unknown => primary,
    };
    let dir = ae::target_dir_for(install, resolved);
    if resolved == PackageKind::Plugin {
        if let Some(sub) = effect_subdir.filter(|s| !s.trim().is_empty()) {
            return dir.join(sub);
        }
    }
    dir
}

/// Install into an After Effects installation, routing each stored entry to the
/// correct folder **by file extension** (`.jsx`→Scripts, `.plugin`/`.aex`→
/// Plug-ins, `.zxp`→CEP), with companion files following the primary asset.
/// Returns every absolute path written, across all folders.
pub fn install_to_ae(
    store: &LibraryStore,
    package: &Package,
    install: &AeInstallation,
    primary: PackageKind,
    effect_subdir: Option<&str>,
) -> AppResult<Vec<String>> {
    let files_dir = store.files_dir(&package.id);
    let mut written = Vec::new();
    for entry in fs::read_dir(&files_dir)? {
        let path = entry?.path();
        let target = route(install, entry_kind(&path), primary, effect_subdir);
        written.push(install_entry(&path, &target)?);
    }
    Ok(written)
}

/* ---------- Elevated (administrator) install ---------- */

/// Shell-single-quote a path for use inside `osascript` `do shell script`.
#[cfg(target_os = "macos")]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Escape a string for embedding in an AppleScript double-quoted literal.
#[cfg(target_os = "macos")]
fn as_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Run a shell command as administrator via `osascript`. If a session password
/// is cached it is passed to AppleScript so no dialog appears; otherwise the OS
/// shows its standard authentication prompt.
/// `action` is the localized noun used in error messages ("書き込み"/"削除").
#[cfg(target_os = "macos")]
fn run_admin(shell: &str, action: &str) -> AppResult<()> {
    // A single-line shell command is expected; control chars would break the
    // AppleScript string literal (and only ever come from a malformed filename).
    if shell.bytes().any(|b| b == b'\n' || b == b'\r' || b == 0) {
        return Err(AppError::msg("ファイル名に不正な文字が含まれています"));
    }
    let cached = admin_pw_cell().lock().ok().and_then(|g| g.clone());
    let apple = match &cached {
        Some(pw) => format!(
            "do shell script \"{}\" user name \"{}\" password \"{}\" with administrator privileges",
            as_applescript(shell),
            as_applescript(&std::env::var("USER").unwrap_or_default()),
            as_applescript(pw),
        ),
        None => format!(
            "do shell script \"{}\" with administrator privileges",
            as_applescript(shell),
        ),
    };
    let out = std::process::Command::new("osascript").arg("-e").arg(&apple).output()?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let msg = err.trim();
    // A cached password that no longer authenticates (e.g. the user changed it)
    // → drop it so the next attempt re-prompts.
    if cached.is_some() && (msg.contains("-60007") || msg.to_lowercase().contains("authenticat")) {
        set_session_password(None);
    }
    Err(AppError::msg(
        if msg.contains("-128") || msg.to_lowercase().contains("cancel") {
            "認証がキャンセルされました。".to_string()
        } else {
            format!("管理者権限での{action}に失敗しました: {msg}")
        },
    ))
}

/// Run a PowerShell script elevated via UAC (single prompt). The script is
/// passed inline as a base64 `-EncodedCommand` rather than written to a temp
/// `.ps1` first, which avoids a write-then-elevate-a-file TOCTOU window.
#[cfg(target_os = "windows")]
fn run_admin(script: &str, action: &str) -> AppResult<()> {
    use base64::Engine;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&utf16);
    // The elevated PowerShell must be launched hidden FROM THE START, or its
    // console flashes before `-WindowStyle Hidden` takes effect. `Start-Process`
    // can't combine `-Verb RunAs` with `-WindowStyle`, but .NET's
    // ProcessStartInfo can (Verb=runas + WindowStyle=Hidden → SW_HIDE at launch)
    // and lets us WaitForExit. The outer helper runs with CREATE_NO_WINDOW.
    let arg = format!(
        "$ErrorActionPreference='Stop'; \
         $psi=New-Object System.Diagnostics.ProcessStartInfo; \
         $psi.FileName='powershell.exe'; \
         $psi.Arguments='-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand {encoded}'; \
         $psi.Verb='runas'; $psi.WindowStyle='Hidden'; $psi.UseShellExecute=$true; \
         $p=[System.Diagnostics.Process]::Start($psi); $p.WaitForExit(); exit $p.ExitCode"
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &arg])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let msg = err.trim();
    if msg.to_lowercase().contains("cancel") {
        return Err(AppError::msg("認証がキャンセルされました。".to_string()));
    }
    Err(AppError::msg(format!(
        "管理者権限での{action}に失敗しました: {msg}"
    )))
}

#[cfg(target_os = "windows")]
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Copy (src → dest) pairs with elevated privileges, prompting the user once
/// for authentication (password / Touch ID on macOS, UAC on Windows).
pub(crate) fn elevated_copy(pairs: &[(PathBuf, PathBuf)]) -> AppResult<()> {
    if pairs.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let cmds: Vec<String> = pairs
            .iter()
            .map(|(src, dest)| {
                let dir = dest.parent().unwrap_or(dest.as_path());
                format!(
                    "/bin/mkdir -p {} && /usr/bin/ditto {} {}",
                    sh_quote(&dir.to_string_lossy()),
                    sh_quote(&src.to_string_lossy()),
                    sh_quote(&dest.to_string_lossy()),
                )
            })
            .collect();
        run_admin(&cmds.join(" && "), "書き込み")
    }
    #[cfg(target_os = "windows")]
    {
        let script = pairs
            .iter()
            .map(|(src, dest)| {
                let dir = dest.parent().unwrap_or(dest.as_path());
                format!(
                    "New-Item -ItemType Directory -Force -Path {} | Out-Null; Copy-Item -Recurse -Force -LiteralPath {} -Destination {}",
                    ps_quote(&dir.to_string_lossy()),
                    ps_quote(&src.to_string_lossy()),
                    ps_quote(&dest.to_string_lossy()),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        run_admin(&script, "書き込み")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = pairs;
        Err(AppError::msg("この OS では管理者権限インストールに未対応です"))
    }
}

/// Collect (src → dest) copy pairs for a set of entries, extracting `.zxp`
/// normally (its CEP target is user-writable and needs no elevation).
pub(crate) fn plan_copies(
    files_dir: &Path,
    resolve_target: impl Fn(&Path) -> PathBuf,
) -> AppResult<(Vec<(PathBuf, PathBuf)>, Vec<String>)> {
    let mut copies = Vec::new();
    let mut written = Vec::new();
    for entry in fs::read_dir(files_dir)? {
        let path = entry?.path();
        let target = resolve_target(&path);
        if ext_lower(&path) == "zxp" {
            written.push(install_zxp(&path, &target)?);
        } else {
            let name = path.file_name().unwrap_or_default();
            let dest = target.join(name);
            copies.push((path.clone(), dest.clone()));
            written.push(dest.to_string_lossy().to_string());
        }
    }
    Ok((copies, written))
}

/// Elevated variant of [`install_to_ae`].
pub fn install_to_ae_elevated(
    store: &LibraryStore,
    package: &Package,
    install: &AeInstallation,
    primary: PackageKind,
    effect_subdir: Option<&str>,
) -> AppResult<Vec<String>> {
    let files_dir = store.files_dir(&package.id);
    let (copies, written) =
        plan_copies(&files_dir, |p| route(install, entry_kind(p), primary, effect_subdir))?;
    elevated_copy(&copies)?;
    Ok(written)
}

/// Elevated variant of [`install_to`] (custom folder).
pub fn install_to_elevated(
    store: &LibraryStore,
    package: &Package,
    target_dir: &Path,
) -> AppResult<Vec<String>> {
    let files_dir = store.files_dir(&package.id);
    let (copies, written) = plan_copies(&files_dir, |_| target_dir.to_path_buf())?;
    elevated_copy(&copies)?;
    Ok(written)
}

/// Remove installed paths with elevated privileges (for files owned by root
/// after an elevated install). Prompts once for authentication.
pub fn elevated_remove(paths: &[PathBuf]) -> AppResult<()> {
    if paths.is_empty() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let quoted: Vec<String> = paths.iter().map(|p| sh_quote(&p.to_string_lossy())).collect();
        run_admin(&format!("/bin/rm -rf {}", quoted.join(" ")), "削除")
    }
    #[cfg(target_os = "windows")]
    {
        let script = paths
            .iter()
            .map(|p| {
                format!(
                    "if (Test-Path -LiteralPath {0}) {{ Remove-Item -Recurse -Force -LiteralPath {0} }}",
                    ps_quote(&p.to_string_lossy())
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        run_admin(&script, "削除")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = paths;
        Err(AppError::msg("この OS では管理者権限での削除に未対応です"))
    }
}
