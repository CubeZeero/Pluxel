use crate::models::{Package, PackageKind};

/// Logical primary item. A .plugin is protected as one opaque directory,
/// including when it is nested inside a generic companion folder.
pub fn primary(pkg: &Package) -> Option<String> {
    let kind = if pkg.kind == PackageKind::ScriptUiPanel {
        PackageKind::Script
    } else {
        pkg.kind
    };
    if kind == PackageKind::Plugin {
        for file in &pkg.files {
            let mut parts = Vec::new();
            for part in file.rel_path.split('/') {
                parts.push(part);
                if part.to_ascii_lowercase().ends_with(".plugin") {
                    return Some(parts.join("/"));
                }
            }
        }
    }
    pkg.files
        .iter()
        .find(|f| f.kind == kind)
        .or_else(|| pkg.files.first())
        .map(|f| f.rel_path.clone())
}

pub fn protects(pkg: &Package, removed: &str) -> bool {
    primary(pkg).is_some_and(|main| {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let (main, removed) = (main.to_lowercase(), removed.to_lowercase());
        main == removed
            || main.starts_with(&format!("{removed}/"))
            || removed.starts_with(&format!("{main}/"))
    })
}
