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
    archive::write_ppf_from_paths(&out, &manifest, &inputs).map_err(|e| e.to_string())?;
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
                if entry.file_type().is_file() {
                    consider(entry.path());
                }
            }
        }
    }
    best
}
