use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::ae;
use crate::error::{AppError, AppResult};
#[cfg(test)]
use crate::importer::copy_recursively;
#[cfg(test)]
use crate::library::LibraryStore;
#[cfg(test)]
use crate::models::Package;
use crate::models::{AeInstallation, PackageKind};

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

/// Documentation/manual formats (PDF, office docs) that ride along inside a
/// package and its `.ppf` for distribution but are never copied into After
/// Effects on install — they would only clutter the Scripts/Plug-ins folders.
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "rtf", "odt", "pages", "ppt", "pptx", "xls", "xlsx", "chm", "epub",
];

/// True for a top-level documentation file that should be skipped when
/// installing into an After Effects folder.
fn is_doc_file(path: &Path) -> bool {
    DOC_EXTS.contains(&ext_lower(path).as_str())
}

fn excluded_asset(path: &Path) -> bool {
    let ext = ext_lower(path);
    is_doc_file(path)
        || (cfg!(target_os = "macos") && ext == "aex")
        || (cfg!(target_os = "windows") && ext == "plugin")
}

fn has_installable_content(path: &Path) -> AppResult<bool> {
    if excluded_asset(path) {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::msg(
            "リンクを含む同梱フォルダには対応していません",
        ));
    }
    if !metadata.is_dir() || ext_lower(path) == "plugin" {
        return Ok(true);
    }
    let mut children = fs::read_dir(path)?.peekable();
    if children.peek().is_none() {
        return Ok(true);
    }
    for entry in children {
        if has_installable_content(&entry?.path())? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Filter companion manuals and opposite-platform effects recursively. Native
/// plugin bundles remain opaque so their signed internal resources are intact.
pub(crate) fn copy_installable(src: &Path, dst: &Path) -> AppResult<()> {
    if !has_installable_content(src)? {
        return Ok(());
    }
    if !src.is_dir() || ext_lower(src) == "plugin" {
        return crate::importer::copy_recursively(src, dst);
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        copy_installable(&entry.path(), &dst.join(entry.file_name()))?;
    }
    Ok(())
}

/// Share the same companion-document policy across normal and elevated copies.
pub(crate) fn installable_entries(files_dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(files_dir)? {
        let path = entry?.path();
        if has_installable_content(&path)? {
            paths.push(path);
        }
    }
    Ok(paths)
}

/// Extract a `.zxp` (a zip archive) into `<target>/<stem>/` and return the
/// created extension directory.
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub fn install_to(
    store: &LibraryStore,
    package: &Package,
    target_dir: &Path,
) -> AppResult<Vec<String>> {
    let files_dir = store.files_dir(&package.id);
    let mut written = Vec::new();
    for path in installable_entries(&files_dir)? {
        written.push(install_entry(&path, target_dir)?);
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
#[cfg(test)]
pub fn install_to_ae(
    store: &LibraryStore,
    package: &Package,
    install: &AeInstallation,
    primary: PackageKind,
    effect_subdir: Option<&str>,
) -> AppResult<Vec<String>> {
    let files_dir = store.files_dir(&package.id);
    let mut written = Vec::new();
    for path in installable_entries(&files_dir)? {
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
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&apple)
        .output()?;
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
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
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
/// Apply a deployment with one authentication prompt. All removals happen
/// before copies, allowing exact replacement and clear+restore rollback.
pub fn elevated_apply(pairs: &[(PathBuf, PathBuf)], removes: &[PathBuf]) -> AppResult<()> {
    // Check the final child path as well as its parent custom directory.
    for (_, dest) in pairs {
        crate::paths::reject_elevated_destination(&dest.to_string_lossy())?;
        crate::deployment::validate_target(dest)?;
    }
    for path in removes {
        crate::paths::reject_elevated_destination(&path.to_string_lossy())?;
        crate::deployment::validate_target(path)?;
    }
    for (src, _) in pairs {
        crate::deployment::validate_tree(src)?;
    }
    if pairs.is_empty() && removes.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        if crate::helper::try_apply(pairs, removes)? {
            return Ok(());
        }
        return run_admin(&macos_apply_script(pairs, removes), "書き込み");
    }
    #[cfg(target_os = "windows")]
    return run_admin(&windows_apply_script(pairs, removes), "書き込み");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err(AppError::msg(
        "この OS では管理者権限インストールに未対応です",
    ))
}

/// Pure payload builder; the same script can be exercised without osascript.
#[cfg(target_os = "macos")]
pub(crate) fn macos_apply_script(pairs: &[(PathBuf, PathBuf)], removes: &[PathBuf]) -> String {
    // Recheck after authentication, before the first destructive command.
    // In particular, a link introduced while the consent dialog was open
    // must not redirect an administrator copy/removal.
    let mut cmds = vec!["pluxel_check() { p=$1; while [ \"$p\" != / ]; do case \"$p\" in /tmp|/var|/etc) ;; *) [ ! -L \"$p\" ] || return 1 ;; esac; p=$(/usr/bin/dirname \"$p\"); done; if [ -e \"$1\" ]; then bad=$(/usr/bin/find \"$1\" ! -type f ! -type d -print) || return 1; [ -z \"$bad\" ] || return 1; fi; }".to_string()];
    for path in pairs.iter().flat_map(|(s, d)| [s, d]).chain(removes.iter()) {
        cmds.push(format!(
            "pluxel_check {}",
            sh_quote(&path.to_string_lossy())
        ));
    }
    cmds.extend(
        removes
            .iter()
            .map(|p| format!("/bin/rm -rf {}", sh_quote(&p.to_string_lossy()))),
    );
    cmds.extend(
        pairs
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
            .collect::<Vec<_>>(),
    );
    cmds.join(" && ")
}

/// Pure payload builder; contains no UAC or process-launch commands.
#[cfg(target_os = "windows")]
pub(crate) fn windows_apply_script(pairs: &[(PathBuf, PathBuf)], removes: &[PathBuf]) -> String {
    let mut script = String::from("function Test-PluxelPath($value) { $p=$value; while ($p) { if (Test-Path -LiteralPath $p) { $item=Get-Item -Force -LiteralPath $p; if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Symbolic link or reparse point in deployment path' } }; $p=Split-Path -Parent $p }; if (Test-Path -LiteralPath $value) { Get-ChildItem -Force -Recurse -LiteralPath $value | ForEach-Object { if ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Symbolic link or reparse point in deployment subtree' } } } }\n");
    for path in pairs.iter().flat_map(|(s, d)| [s, d]).chain(removes.iter()) {
        script.push_str(&format!(
            "Test-PluxelPath {}\n",
            ps_quote(&path.to_string_lossy())
        ));
    }
    for p in removes {
        script.push_str(&format!(
            "if (Test-Path -LiteralPath {0}) {{ Remove-Item -Recurse -Force -LiteralPath {0} }}\n",
            ps_quote(&p.to_string_lossy())
        ));
    }
    script.push_str(&pairs
            .iter()
            .map(|(src, dest)| {
                let dir = dest.parent().unwrap_or(dest.as_path());
                format!(
                    "New-Item -ItemType Directory -Force -Path {0} | Out-Null; if ((Get-Item -LiteralPath {1}).PSIsContainer) {{ New-Item -ItemType Directory -Force -Path {2} | Out-Null; Get-ChildItem -Force -LiteralPath {1} | ForEach-Object {{ Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination (Join-Path {2} $_.Name) }} }} else {{ Copy-Item -Force -LiteralPath {1} -Destination {2} }}",
                    ps_quote(&dir.to_string_lossy()),
                    ps_quote(&src.to_string_lossy()),
                    ps_quote(&dest.to_string_lossy()),
                )
            })
            .collect::<Vec<_>>()
            .join("\n"));
    format!("$ErrorActionPreference='Stop';\n{script}")
}

/// Collect (src → dest) copy pairs for a set of entries, extracting `.zxp`
/// normally (its CEP target is user-writable and needs no elevation).
#[cfg(test)]
pub(crate) fn plan_copies(
    files_dir: &Path,
    resolve_target: impl Fn(&Path) -> PathBuf,
) -> AppResult<(Vec<(PathBuf, PathBuf)>, Vec<String>)> {
    let mut copies = Vec::new();
    let mut written = Vec::new();
    for path in installable_entries(files_dir)? {
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

/// Remove installed paths with elevated privileges (for files owned by root
/// after an elevated install). Prompts once for authentication.
pub fn elevated_remove(paths: &[PathBuf]) -> AppResult<()> {
    if paths.is_empty() {
        return Ok(());
    }

    elevated_apply(&[], paths)
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod tests {
    use super::*;
    use std::process::{Command, Output};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("pluxel-payload-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            // macOS /var is a standard alias; Windows canonicalize introduces
            // a verbatim prefix that Windows PowerShell 5.1 providers handle
            // inconsistently. Test ordinary absolute user-facing paths there.
            #[cfg(target_os = "macos")]
            let root = root.canonicalize().unwrap();
            Self(root)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
        // Deliberately execute only the generated payload, never elevated_apply
        // or run_admin. These subprocesses inherit the test user's privileges.
        fn run(&self, pairs: &[(PathBuf, PathBuf)], removes: &[PathBuf]) -> Output {
            #[cfg(target_os = "macos")]
            let script = macos_apply_script(pairs, removes);
            #[cfg(target_os = "windows")]
            let script = windows_apply_script(pairs, removes);
            self.run_script(&script)
        }
        fn run_script(&self, script: &str) -> Output {
            #[cfg(target_os = "macos")]
            let mut command = {
                let mut cmd = Command::new("/bin/sh");
                cmd.args(["-c", script]);
                cmd
            };
            #[cfg(target_os = "windows")]
            let mut command = {
                use base64::Engine;
                let bytes: Vec<u8> = script
                    .encode_utf16()
                    .flat_map(|u| u.to_le_bytes())
                    .collect();
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                let mut cmd = Command::new("powershell.exe");
                cmd.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded]);
                cmd
            };
            command
                .current_dir(&self.0)
                .env("PLUXEL_PAYLOAD_TEST", "EXPANDED")
                .output()
                .unwrap()
        }
        fn success(&self, pairs: &[(PathBuf, PathBuf)], removes: &[PathBuf]) {
            let output = self.run(pairs, removes);
            assert!(
                output.status.success(),
                "payload failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn generated_payload_exact_directory_replace_hidden_children_and_restore() {
        let f = Fixture::new();
        let source = f.path("source staging");
        let backup = f.path("backup staging");
        let dest = f.path("new parent/exact.plugin");
        let obsolete = f.path("obsolete folder");
        fs::create_dir_all(source.join("nested/empty")).unwrap();
        fs::create_dir(source.join(".hidden-directory")).unwrap();
        fs::write(source.join(".hidden-directory/child"), b"hidden child").unwrap();
        fs::write(source.join(".hidden-file"), b"hidden file").unwrap();
        fs::write(source.join("nested/new-only"), b"new").unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old-only"), b"old").unwrap();
        copy_recursively(&dest, &backup).unwrap();
        fs::create_dir(&obsolete).unwrap();
        fs::write(obsolete.join("obsolete-child"), b"remove").unwrap();
        #[cfg(target_os = "windows")]
        {
            // Windows hidden attributes, not just Unix dot names.
            let script = format!("$ErrorActionPreference='Stop'; foreach ($p in @({}, {})) {{ $item=Get-Item -Force -LiteralPath $p; $item.Attributes=$item.Attributes -bor [IO.FileAttributes]::Hidden }}", ps_quote(&source.join(".hidden-directory").to_string_lossy()), ps_quote(&source.join(".hidden-file").to_string_lossy()));
            assert!(f.run_script(&script).status.success());
        }
        f.success(
            &[(source.clone(), dest.clone())],
            &[dest.clone(), obsolete.clone()],
        );
        assert!(!obsolete.exists());
        assert!(!dest.join("old-only").exists());
        assert_eq!(fs::read(dest.join("nested/new-only")).unwrap(), b"new");
        assert_eq!(fs::read(dest.join(".hidden-file")).unwrap(), b"hidden file");
        assert_eq!(
            fs::read(dest.join(".hidden-directory/child")).unwrap(),
            b"hidden child"
        );
        assert!(dest.join("nested/empty").is_dir());
        assert!(
            !dest.join("source staging").exists(),
            "must copy contents to the exact target"
        );
        // The recovery payload must clear new children before restoring old bytes.
        f.success(&[(backup, dest.clone())], std::slice::from_ref(&dest));
        assert_eq!(fs::read(dest.join("old-only")).unwrap(), b"old");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
        assert!(source.join("nested/new-only").exists());
    }

    #[test]
    fn generated_payload_paths_are_literal_and_creates_missing_parents() {
        let f = Fixture::new();
        let name = "日本語 space's $PLUXEL_PAYLOAD_TEST $(touch INJECTED) [literal]";
        let source = f.path(&format!("source {name}.jsx"));
        let dest = f.path(&format!("parent {name}/destination {name}.jsx"));
        let obsolete = f.path(&format!("obsolete {name}"));
        fs::write(&source, b"literal bytes\0\xff").unwrap();
        fs::write(&obsolete, b"old").unwrap();
        f.success(
            &[(source.clone(), dest.clone())],
            std::slice::from_ref(&obsolete),
        );
        assert_eq!(fs::read(&dest).unwrap(), b"literal bytes\0\xff");
        assert!(source.exists());
        assert!(!obsolete.exists());
        assert!(!f.path("INJECTED").exists());
        f.success(&[], std::slice::from_ref(&dest));
        assert!(!dest.exists());
        f.success(&[], &[]);
    }

    // A junction needs no Windows symlink privilege or UAC. All link targets
    // remain inside this fixture, even if the guard were to regress.
    fn directory_link(f: &Fixture, link: &Path, target: &Path) {
        #[cfg(target_os = "macos")]
        {
            let _ = f;
            std::os::unix::fs::symlink(target, link).unwrap();
        }
        #[cfg(target_os = "windows")]
        {
            let script = format!("$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path {} -Value {} | Out-Null", ps_quote(&link.to_string_lossy()), ps_quote(&target.to_string_lossy()));
            let out = f.run_script(&script);
            assert!(
                out.status.success(),
                "junction creation failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn generated_payload_rejects_links_before_any_removal_or_copy() {
        let f = Fixture::new();
        let source = f.path("source");
        let dest = f.path("destination");
        let obsolete = f.path("obsolete");
        let outside = f.path("outside");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("new"), b"new").unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("old"), b"old").unwrap();
        fs::write(&obsolete, b"keep obsolete").unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        let link = dest.join("linked-child");
        directory_link(&f, &link, &outside);
        let out = f.run(
            &[(source.clone(), dest.clone())],
            &[obsolete.clone(), dest.clone()],
        );
        assert!(!out.status.success());
        assert_eq!(fs::read(&obsolete).unwrap(), b"keep obsolete");
        assert_eq!(fs::read(dest.join("old")).unwrap(), b"old");
        assert!(!dest.join("new").exists());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
        // Remove only the link node, then test a link in the source subtree.
        #[cfg(target_os = "macos")]
        fs::remove_file(&link).unwrap();
        #[cfg(target_os = "windows")]
        fs::remove_dir(&link).unwrap();
        let source_link = source.join("linked-child");
        directory_link(&f, &source_link, &outside);
        let out = f.run(&[(source, dest.clone())], &[obsolete.clone(), dest.clone()]);
        assert!(!out.status.success());
        assert!(obsolete.exists());
        assert_eq!(fs::read(dest.join("old")).unwrap(), b"old");
    }

    #[test]
    fn generated_payload_rejects_linked_parent_before_writes() {
        let f = Fixture::new();
        let source = f.path("source.jsx");
        let outside = f.path("outside");
        let parent = f.path("linked-parent");
        let obsolete = f.path("obsolete");
        fs::write(&source, b"new").unwrap();
        fs::write(&obsolete, b"old").unwrap();
        fs::create_dir(&outside).unwrap();
        directory_link(&f, &parent, &outside);
        let out = f.run(
            &[(source, parent.join("new.jsx"))],
            std::slice::from_ref(&obsolete),
        );
        assert!(!out.status.success());
        assert!(obsolete.exists());
        assert!(!outside.join("new.jsx").exists());
    }
}
