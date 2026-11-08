//! Command-line `.ppf` packaging: `pluxel package --name … --out … <files>`.
//!
//! Runs headless (no window) and reuses the same distribution format the app
//! imports, so a generated `.ppf` round-trips through "Add package".

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::archive;
use crate::models::{Manifest, PackageKind};

const HELP: &str = "\
Pluxel — create a .ppf package from the command line

USAGE:
    pluxel package [OPTIONS] <FILE|FOLDER>...

OPTIONS:
    --name <NAME>          Package name (required)
    --version <VERSION>    Version string (e.g. 1.0.0)
    --author <AUTHOR>      Author
    --description <TEXT>   Description
    --homepage <URL>       Homepage URL
    --banner <PATH>        Banner image (.png | .jpg | .jpeg | .webp)
    --kind <KIND>          script | script-ui-panel | plugin | zxp | installer
                           (auto-detected from the files when omitted)
    --out <PATH>           Output .ppf path (default: ./<name>.ppf)
    -h, --help             Show this help

EXAMPLE:
    pluxel package --name MyEffect --version 1.0.0 --out ./MyEffect.ppf effect.jsx preset.ffx
";

/// Run the `package` subcommand. `args` excludes the program name and the
/// `package` token. Returns a process exit code.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    match run_inner(args) {
        Ok(out) => {
            println!("✔ wrote {}", out.display());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            2
        }
    }
}

fn run_inner(args: &[String]) -> Result<PathBuf, String> {
    let mut name: Option<String> = None;
    let mut version = String::new();
    let mut author = String::new();
    let mut description = String::new();
    let mut homepage = String::new();
    let mut kind_arg: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut banner: Option<PathBuf> = None;
    let mut inputs: Vec<PathBuf> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].clone();
        // Accept both `--flag` and `-flag`.
        let flag = match arg.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => rest.trim_start_matches('-').to_string(),
            _ => {
                inputs.push(PathBuf::from(&arg));
                i += 1;
                continue;
            }
        };
        macro_rules! value {
            () => {{
                i += 1;
                match args.get(i) {
                    Some(v) => v.clone(),
                    None => return Err(format!("missing value for -{flag}")),
                }
            }};
        }
        match flag.as_str() {
            "name" => name = Some(value!()),
            "version" => version = value!(),
            "author" => author = value!(),
            "description" => description = value!(),
            "homepage" => homepage = value!(),
            "banner" => {
                if banner.is_some() {
                    return Err("--banner may only be specified once".into());
                }
                banner = Some(PathBuf::from(value!()));
            }
            "kind" => kind_arg = Some(value!()),
            "out" | "o" => out = Some(PathBuf::from(value!())),
            other => return Err(format!("unknown option: -{other}")),
        }
        i += 1;
    }

    let name = name.ok_or("--name is required")?;
    if inputs.is_empty() {
        return Err("no input files given".into());
    }
    for input in &inputs {
        if !input.exists() {
            return Err(format!("no such file or folder: {}", input.display()));
        }
    }

    let kind = match kind_arg {
        Some(k) => parse_kind(&k)?,
        None => detect_kind(&inputs),
    };
    if kind == PackageKind::Unknown {
        return Err("could not determine package kind — pass --kind explicitly".into());
    }

    let manifest = Manifest {
        name: name.clone(),
        version,
        author,
        description,
        homepage,
        tags: Vec::new(),
        install_as: Some(kind),
    };

    let out = out.unwrap_or_else(|| PathBuf::from(format!("{name}.ppf")));
    match banner {
        Some(banner) => archive::write_ppf_with_banner(&out, &manifest, &inputs, &banner),
        None => archive::write_ppf_from_paths(&out, &manifest, &inputs),
    }
    .map_err(|e| e.to_string())?;
    Ok(out)
}

fn parse_kind(s: &str) -> Result<PackageKind, String> {
    match s.to_ascii_lowercase().as_str() {
        "script" => Ok(PackageKind::Script),
        "script-ui-panel" | "scriptui" | "script-ui" | "scriptuipanel" => {
            Ok(PackageKind::ScriptUiPanel)
        }
        "plugin" | "effect" => Ok(PackageKind::Plugin),
        "zxp" => Ok(PackageKind::Zxp),
        "installer" => Ok(PackageKind::Installer),
        other => Err(format!(
            "unknown kind '{other}' (expected script | script-ui-panel | plugin | zxp | installer)"
        )),
    }
}

/// Highest-priority kind among the inputs, matching the app's classification.
fn detect_kind(inputs: &[PathBuf]) -> PackageKind {
    let priority = |k: PackageKind| match k {
        PackageKind::Installer => 5,
        PackageKind::Zxp => 4,
        PackageKind::Plugin => 3,
        PackageKind::ScriptUiPanel => 2,
        PackageKind::Script => 1,
        PackageKind::Unknown => 0,
    };
    let mut best = PackageKind::Unknown;
    let mut consider = |p: &Path| {
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let k = PackageKind::from_extension(&ext);
        if priority(k) > priority(best) {
            best = k;
        }
    };
    for input in inputs {
        // A `.plugin`/`.aex` may itself be a bundle folder — check its own name.
        consider(input);
        if input.is_dir() {
            for entry in WalkDir::new(input).min_depth(1).into_iter().flatten() {
                if entry.file_type().is_file() || entry.file_type().is_dir() {
                    consider(entry.path());
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    #[test]
    fn explicit_banner_round_trip_accepts_arbitrary_names_and_spaces() {
        let root = std::env::temp_dir().join(format!("cli-banner-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("images")).unwrap();
        let script = root.join("Tool.jsx");
        let banner = root.join("images/My artwork.JPG");
        let out = root.join("Tool.ppf");
        fs::write(&script, b"// tool").unwrap();
        fs::write(&banner, b"banner bytes").unwrap();
        let args = vec![
            "--name".into(),
            "Tool".into(),
            "--banner".into(),
            banner.to_string_lossy().into(),
            "--out".into(),
            out.to_string_lossy().into(),
            script.to_string_lossy().into(),
            banner.to_string_lossy().into(),
        ];
        run_inner(&args).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
        assert_eq!(zip.len(), 3); // manifest, normalized banner, script; no duplicate input
        let mut bytes = Vec::new();
        zip.by_name("banner.jpg")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"banner bytes");
        drop(zip);
        let store = crate::library::LibraryStore::new(root.join("library")).unwrap();
        let imported = archive::import_archive(&store, &out).unwrap();
        assert_eq!(imported[0].banner.as_deref(), Some("banner.jpg"));
        assert_eq!(imported[0].kind, PackageKind::Script);
        assert!(imported[0].files.iter().all(|f| f.rel_path == "Tool.jsx"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_banner_and_conflicts_preserve_existing_output() {
        let root = std::env::temp_dir().join(format!("cli-banner-errors-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("Tool.jsx");
        let out = root.join("Tool.ppf");
        fs::write(&script, b"// tool").unwrap();
        fs::write(&out, b"existing output").unwrap();
        let invoke = |banner: &Path, extra: &[String]| {
            let mut args = vec![
                "--name".into(),
                "Tool".into(),
                "--out".into(),
                out.to_string_lossy().into(),
                "--banner".into(),
                banner.to_string_lossy().into(),
                script.to_string_lossy().into(),
            ];
            args.extend_from_slice(extra);
            let error = run_inner(&args).unwrap_err();
            assert_eq!(fs::read(&out).unwrap(), b"existing output");
            error
        };
        let text = root.join("not-image.txt");
        fs::write(&text, b"text").unwrap();
        assert!(invoke(&text, &[]).contains("--banner"));
        assert!(invoke(&root.join("missing.png"), &[]).contains("--banner"));
        let directory = root.join("directory.png");
        fs::create_dir(&directory).unwrap();
        assert!(invoke(&directory, &[]).contains("--banner"));
        let image = root.join("art.png");
        fs::write(&image, b"").unwrap();
        assert!(invoke(&image, &[]).contains("non-empty"));
        fs::File::create(&image)
            .unwrap()
            .set_len(32 * 1024 * 1024 + 1)
            .unwrap();
        assert!(invoke(&image, &[]).contains("32 MiB"));
        fs::write(&image, b"image").unwrap();
        let conflict = root.join("preview.jpg");
        fs::write(&conflict, b"other").unwrap();
        assert!(invoke(&image, &[conflict.to_string_lossy().into()]).contains("Conflicting banner"));
        assert!(
            invoke(&image, &["--banner".into(), image.to_string_lossy().into()])
                .contains("only be specified once")
        );
        let missing_value = vec![
            "--name".into(),
            "Tool".into(),
            script.to_string_lossy().into(),
            "--banner".into(),
        ];
        assert!(run_inner(&missing_value)
            .unwrap_err()
            .contains("missing value"));
        let manifest = Manifest::default();
        assert!(archive::write_ppf_with_banner(&image, &manifest, &[script], &image).is_err());
        assert_eq!(fs::read(&image).unwrap(), b"image");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parent_folder_plugin_autodetection_and_nested_tags_round_trip() {
        let root = std::env::temp_dir().join(format!("cli-plugin-{}", uuid::Uuid::new_v4()));
        let input = root.join("Release");
        let binary = input.join("Foo.plugin/Contents/MacOS/Foo");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, b"binary").unwrap();
        fs::write(
            input.join("manifest.json"),
            r#"{"name":"Foo","tags":["private-client"],"version":"1"}"#,
        )
        .unwrap();
        assert_eq!(detect_kind(&[input.clone()]), PackageKind::Plugin);
        let out = root.join("Foo.ppf");
        assert!(run_inner(&[
            "--name".into(),
            "Foo".into(),
            "--out".into(),
            out.to_string_lossy().into(),
            input.to_string_lossy().into()
        ])
        .is_ok());
        let mut zip = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
        for name in ["manifest.json", "Release/manifest.json"] {
            let mut text = String::new();
            zip.by_name(name)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(value.get("tags").is_none());
            if name == "manifest.json" {
                assert_eq!(value["install_as"], "plugin");
            } else {
                assert_eq!(value["version"], "1");
            }
        }
        let store = crate::library::LibraryStore::new(root.join("library")).unwrap();
        let packages = archive::import_archive(&store, &out).unwrap();
        assert_eq!(packages[0].kind, PackageKind::Plugin);
        assert!(packages[0].manifest.tags.is_empty());
        assert_eq!(fs::read(&binary).unwrap(), b"binary");
        drop(zip);
        fs::remove_dir_all(root).unwrap();
    }
}
