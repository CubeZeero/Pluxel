//! Core-logic tests that exercise the import → export → import → install flow
//! without the Tauri runtime (operating directly on a `LibraryStore`).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::archive;
use crate::importer;
use crate::installer;
use crate::library::LibraryStore;
use crate::models::{AeInstallation, Bundle, PackageKind};

fn tmp(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("pluxel-test-{tag}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p).unwrap();
    p
}

fn fake_ae(root: &std::path::Path) -> AeInstallation {
    AeInstallation {
        name: "AE Test 2024".into(),
        version: "2024".into(),
        root: root.to_string_lossy().into(),
        scripts_dir: root.join("Scripts").to_string_lossy().into(),
        scriptui_dir: root.join("Scripts/ScriptUI Panels").to_string_lossy().into(),
        plugins_dir: root.join("Plug-ins").to_string_lossy().into(),
        cep_extensions_dir: root.join("CEP").to_string_lossy().into(),
    }
}

#[test]
fn script_import_export_import_install_roundtrip() {
    let work = tmp("work");
    let jsx = work.join("Hello.jsx");
    fs::write(&jsx, "alert('hi');").unwrap();

    // Import into a fresh store.
    let store = LibraryStore::new(tmp("store")).unwrap();
    let pkg = importer::import_path(&store, &jsx).unwrap();
    assert_eq!(pkg.kind, PackageKind::Script);
    assert_eq!(pkg.files.len(), 1);
    assert_eq!(pkg.manifest.name, "Hello");
    assert_eq!(store.load().unwrap().packages.len(), 1);

    // Export all → zip.
    let zip = work.join("export.zip");
    assert_eq!(archive::export_library(&store, &[], &zip).unwrap(), 1);
    assert!(zip.exists());

    // Import the zip into a second store; expect a fresh id but same metadata.
    let store2 = LibraryStore::new(tmp("store2")).unwrap();
    let imported = archive::import_archive(&store2, &zip).unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].manifest.name, "Hello");
    assert_ne!(imported[0].id, pkg.id);
    assert!(store2.files_dir(&imported[0].id).join("Hello.jsx").exists());

    // Install into a fake AE → lands in Scripts/.
    let ae_root = tmp("ae");
    let ae = fake_ae(&ae_root);
    let target = std::path::PathBuf::from(&ae.scripts_dir);
    let written = installer::install_to(&store, &pkg, &target).unwrap();
    assert_eq!(written.len(), 1);
    assert!(ae_root.join("Scripts/Hello.jsx").exists());

    // Uninstall by removing the written paths.
    for p in &written {
        let _ = fs::remove_file(p);
    }
    assert!(!ae_root.join("Scripts/Hello.jsx").exists());
}

#[test]
fn scriptui_override_targets_panels_folder() {
    let work = tmp("work-ui");
    let jsx = work.join("Panel.jsx");
    fs::write(&jsx, "// panel").unwrap();

    let store = LibraryStore::new(tmp("store-ui")).unwrap();
    let pkg = importer::import_path(&store, &jsx).unwrap();

    let ae_root = tmp("ae-ui");
    let ae = fake_ae(&ae_root);
    let target = std::path::PathBuf::from(&ae.scriptui_dir);
    installer::install_to(&store, &pkg, &target).unwrap();
    assert!(ae_root.join("Scripts/ScriptUI Panels/Panel.jsx").exists());
}

#[test]
fn unsupported_file_is_rejected() {
    let work = tmp("work-bad");
    let img = work.join("photo.png");
    fs::write(&img, b"\x89PNG").unwrap();
    let store = LibraryStore::new(tmp("store-bad")).unwrap();
    assert!(importer::import_path(&store, &img).is_err());
    assert_eq!(store.load().unwrap().packages.len(), 0); // nothing left behind
}

#[test]
fn unsupported_zip_is_rejected() {
    let work = tmp("work-badzip");
    let zip_path = work.join("photos.zip");
    {
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("a.png", opts).unwrap();
        zw.write_all(b"\x89PNG").unwrap();
        zw.start_file("b.jpg", opts).unwrap();
        zw.write_all(b"jpg").unwrap();
        zw.finish().unwrap();
    }
    let store = LibraryStore::new(tmp("store-badzip")).unwrap();
    assert!(archive::import_archive(&store, &zip_path).is_err());
    assert_eq!(store.load().unwrap().packages.len(), 0);
}

#[test]
fn bundle_export_import_roundtrip() {
    let work = tmp("work-bundle");
    let a = work.join("NF_Glow.jsx");
    fs::write(&a, "// glow").unwrap();
    let b = work.join("NF_Blur.jsx");
    fs::write(&b, "// blur").unwrap();

    let store = LibraryStore::new(tmp("store-bundle")).unwrap();
    let pa = importer::import_path(&store, &a).unwrap();
    let pb = importer::import_path(&store, &b).unwrap();

    let bundle = Bundle {
        id: "bid".into(),
        name: "NF Series".into(),
        description: "effects".into(),
        author: "NF".into(),
        homepage: String::new(),
        banner: None,
        package_ids: vec![pa.id.clone(), pb.id.clone()],
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    store.upsert_bundle(bundle).unwrap();

    let zip = work.join("NF.zip");
    archive::export_bundle(&store, "bid", &zip).unwrap();
    assert!(zip.exists());

    // Import the bundle zip into a fresh store.
    let store2 = LibraryStore::new(tmp("store-bundle2")).unwrap();
    let imported = archive::import_archive(&store2, &zip).unwrap();
    assert_eq!(imported.len(), 2);

    let lib2 = store2.load().unwrap();
    assert_eq!(lib2.bundles.len(), 1);
    assert_eq!(lib2.bundles[0].name, "NF Series");
    assert_eq!(lib2.bundles[0].package_ids.len(), 2);
    for id in &lib2.bundles[0].package_ids {
        assert!(lib2.packages.iter().any(|p| &p.id == id));
    }
}

#[test]
fn installer_files_are_classified_as_installer() {
    let work = tmp("work-inst");
    let pkg_file = work.join("Setup.pkg");
    fs::write(&pkg_file, b"pkg").unwrap();
    let exe_file = work.join("Setup.exe");
    fs::write(&exe_file, b"exe").unwrap();

    let store = LibraryStore::new(tmp("store-inst")).unwrap();
    assert_eq!(
        importer::import_path(&store, &pkg_file).unwrap().kind,
        PackageKind::Installer
    );
    assert_eq!(
        importer::import_path(&store, &exe_file).unwrap().kind,
        PackageKind::Installer
    );
}

#[test]
fn zxp_installs_by_extracting_into_cep() {
    // Build a minimal .zxp (a zip) with one file inside.
    let work = tmp("work-zxp");
    let zxp = work.join("MyExt.zxp");
    {
        let f = fs::File::create(&zxp).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("CSXS/manifest.xml", opts).unwrap();
        zw.write_all(b"<ExtensionManifest/>").unwrap();
        zw.finish().unwrap();
    }

    let store = LibraryStore::new(tmp("store-zxp")).unwrap();
    let pkg = importer::import_path(&store, &zxp).unwrap();
    assert_eq!(pkg.kind, PackageKind::Zxp);

    let ae_root = tmp("ae-zxp");
    let ae = fake_ae(&ae_root);
    let target = std::path::PathBuf::from(&ae.cep_extensions_dir);
    installer::install_to(&store, &pkg, &target).unwrap();
    // Extracted into <cep>/MyExt/CSXS/manifest.xml
    assert!(ae_root.join("CEP/MyExt/CSXS/manifest.xml").exists());
}

#[test]
fn folder_with_manifest_and_banner_is_detected() {
    let work = tmp("work-folder");
    let bundle = work.join("CoolScript");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("CoolScript.jsx"), "// cool").unwrap();
    fs::write(
        bundle.join("manifest.json"),
        r#"{"name":"Cool Script","version":"1.2","author":"Me","tags":["fx","util"]}"#,
    )
    .unwrap();
    // 1x1 PNG-ish placeholder is fine; extension drives detection.
    fs::write(bundle.join("banner.png"), b"\x89PNG\r\n").unwrap();

    let store = LibraryStore::new(tmp("store-folder")).unwrap();
    let pkg = importer::import_path(&store, &bundle).unwrap();

    assert_eq!(pkg.manifest.name, "Cool Script");
    assert_eq!(pkg.manifest.version, "1.2");
    assert_eq!(pkg.manifest.tags, vec!["fx", "util"]);
    assert_eq!(pkg.banner.as_deref(), Some("banner.png"));
    assert_eq!(pkg.kind, PackageKind::Script);
}

#[test]
fn companion_files_added_then_installed_together() {
    let work = tmp("work-comp");
    let jsx = work.join("Main.jsx");
    fs::write(&jsx, "// main").unwrap();
    // A companion .ffx preset and an images folder.
    let ffx = work.join("Preset.ffx");
    fs::write(&ffx, b"ffx").unwrap();
    let images = work.join("images");
    fs::create_dir_all(&images).unwrap();
    fs::write(images.join("icon.png"), b"\x89PNG").unwrap();

    let store = LibraryStore::new(tmp("store-comp")).unwrap();
    let mut pkg = importer::import_path(&store, &jsx).unwrap();
    pkg = crate::importer::add_source(&store, &pkg.id, &ffx)
        .and_then(|_| crate::importer::add_source(&store, &pkg.id, &images))
        .and_then(|_| {
            crate::importer::rescan(&store, &mut pkg)?;
            Ok(())
        })
        .map(|_| pkg)
        .unwrap();
    assert!(pkg.files.iter().any(|f| f.rel_path == "Preset.ffx"));
    assert!(pkg.files.iter().any(|f| f.rel_path == "images/icon.png"));

    // Install → all companion files land in Scripts/ preserving structure.
    let ae_root = tmp("ae-comp");
    let target = ae_root.join("Scripts");
    installer::install_to(&store, &pkg, &target).unwrap();
    assert!(ae_root.join("Scripts/Main.jsx").exists());
    assert!(ae_root.join("Scripts/Preset.ffx").exists());
    assert!(ae_root.join("Scripts/images/icon.png").exists());
}

#[test]
fn removing_a_companion_file_updates_the_list() {
    let work = tmp("work-rm");
    let jsx = work.join("Main.jsx");
    fs::write(&jsx, "// main").unwrap();
    let ffx = work.join("Preset.ffx");
    fs::write(&ffx, b"ffx").unwrap();

    let store = LibraryStore::new(tmp("store-rm")).unwrap();
    let mut pkg = importer::import_path(&store, &jsx).unwrap();
    importer::add_source(&store, &pkg.id, &ffx).unwrap();
    importer::rescan(&store, &mut pkg).unwrap();
    assert!(pkg.files.iter().any(|f| f.rel_path == "Preset.ffx"));

    // Simulate remove_file: delete the companion, then rescan.
    fs::remove_file(store.files_dir(&pkg.id).join("Preset.ffx")).unwrap();
    importer::rescan(&store, &mut pkg).unwrap();
    assert!(!pkg.files.iter().any(|f| f.rel_path == "Preset.ffx"));
    assert!(pkg.files.iter().any(|f| f.rel_path == "Main.jsx")); // body remains
}

#[test]
fn replace_files_bumps_version_keeping_id() {
    let work = tmp("work-upd");
    let v1 = work.join("Tool.jsx");
    fs::write(&v1, "// v1").unwrap();

    let store = LibraryStore::new(tmp("store-upd")).unwrap();
    let mut pkg = importer::import_path(&store, &v1).unwrap();
    let id = pkg.id.clone();

    let v2 = work.join("Tool.jsx");
    fs::write(&v2, "// v2 with more code").unwrap();
    importer::replace_files(&store, &mut pkg, &[v2]).unwrap();

    assert_eq!(pkg.id, id); // same package id
    let stored = fs::read_to_string(store.files_dir(&id).join("Tool.jsx")).unwrap();
    assert_eq!(stored, "// v2 with more code");
}

#[test]
fn update_button_reinstalls_current_files() {
    // The detail panel's "更新" (update) button just re-installs to the same
    // target. This verifies that a re-install overwrites the destination with
    // the library's CURRENT files — i.e. an updated package really updates.
    let work = tmp("work-upd2");
    let jsx = work.join("Tool.jsx");
    fs::write(&jsx, "// v1").unwrap();
    let store = LibraryStore::new(tmp("store-upd2")).unwrap();
    let mut pkg = importer::import_path(&store, &jsx).unwrap();

    let target = tmp("upd-target");
    installer::install_to(&store, &pkg, &target).unwrap();
    assert_eq!(fs::read_to_string(target.join("Tool.jsx")).unwrap(), "// v1");

    // The library copy is updated (e.g. user re-imported / replaced files)…
    let v2 = work.join("Tool.jsx");
    fs::write(&v2, "// v2 UPDATED").unwrap();
    importer::replace_files(&store, &mut pkg, &[v2]).unwrap();

    // …and pressing "更新" (re-install to the same target) reflects the change.
    installer::install_to(&store, &pkg, &target).unwrap();
    assert_eq!(
        fs::read_to_string(target.join("Tool.jsx")).unwrap(),
        "// v2 UPDATED",
        "re-install (update) must overwrite the destination with the new content"
    );
}

#[test]
fn non_elevated_install_needs_no_uac_for_writable_target() {
    // A plain (non-elevated) install just copies files. It succeeds without any
    // elevation when the target folder is user-writable, and only fails (→ the
    // app then offers elevation) when the folder is not writable.
    let work = tmp("work-noelev");
    let jsx = work.join("Tool.jsx");
    fs::write(&jsx, "// tool").unwrap();
    let store = LibraryStore::new(tmp("store-noelev")).unwrap();
    let pkg = importer::import_path(&store, &jsx).unwrap();

    // 1) Writable target → installs with NO elevation.
    let writable = tmp("writable-target");
    let written = installer::install_to(&store, &pkg, &writable).unwrap();
    assert!(writable.join("Tool.jsx").exists(), "file installed without elevation");
    assert_eq!(written.len(), 1);

    // 2) Read-only target → permission error, which is what triggers the
    //    elevation prompt in the app (Unix only; simulate via chmod 0o555).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = tmp("locked-target");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let res = installer::install_to(&store, &pkg, &locked);
        assert!(res.is_err(), "writing to a read-only folder should fail (→ elevate)");
        // restore perms so the temp dir can be cleaned
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn is_safe_segment_rejects_traversal() {
    use crate::library::is_safe_segment;
    // Safe: plain filenames / ids.
    assert!(is_safe_segment("banner.png"));
    assert!(is_safe_segment("a1b2c3-uuid"));
    // Unsafe: separators, parent refs, absolute/drive, empty.
    assert!(!is_safe_segment(""));
    assert!(!is_safe_segment(".."));
    assert!(!is_safe_segment("../evil"));
    assert!(!is_safe_segment("a/b"));
    assert!(!is_safe_segment("a\\b"));
    assert!(!is_safe_segment("/etc/passwd"));
    assert!(!is_safe_segment("../../../../Users/victim/.ssh/id_rsa"));
}

#[test]
fn resolve_install_params_validates_dirs_and_subdir() {
    use crate::commands::{resolve_install_params, InstallParams};
    let base = |custom: &str, elevated: bool, sub: Option<&str>| InstallParams {
        installation: None,
        kind: None,
        custom_dir: Some(custom.to_string()),
        label: None,
        elevated: Some(elevated),
        effect_subdir: sub.map(|s| s.to_string()),
    };
    // A traversal-y effect subfolder is rejected.
    assert!(resolve_install_params(base("/Users/me/Effects", false, Some("../evil"))).is_err());
    // A plain subfolder + normal folder is accepted.
    assert!(resolve_install_params(base("/Users/me/Effects", true, Some("Effects"))).is_ok());
    #[cfg(target_os = "macos")]
    {
        // Elevated install into a privileged location is rejected.
        assert!(resolve_install_params(base("/Library/LaunchDaemons", true, None)).is_err());
    }
}

#[test]
fn reject_privileged_dir_blocks_system_locations() {
    use crate::commands::reject_privileged_dir;
    // A normal user folder is allowed.
    assert!(reject_privileged_dir("/Users/me/Effects").is_ok());
    #[cfg(target_os = "macos")]
    {
        assert!(reject_privileged_dir("/Library/LaunchDaemons").is_err());
        assert!(reject_privileged_dir("/Library/LaunchDaemons/evil").is_err());
        assert!(reject_privileged_dir("/System/Library").is_err());
        assert!(reject_privileged_dir("/usr/local/bin").is_err());
    }
}

#[test]
fn replace_keeps_metadata_for_plain_file_but_adopts_package() {
    use crate::commands::merged_replace_manifest;
    use crate::models::Manifest;

    let existing = Manifest {
        name: "Glow".into(),
        version: "2.1".into(),
        author: "NF".into(),
        description: "nice glow".into(),
        homepage: "https://nf.example".into(),
        tags: vec!["fav".into()],
        install_as: None,
    };

    // Replaced by a plain file (no manifest metadata) → keep existing info.
    let bare = Manifest { name: "Glow".into(), ..Manifest::default() };
    let m = merged_replace_manifest(existing.clone(), bare);
    assert_eq!(m.version, "2.1");
    assert_eq!(m.author, "NF");
    assert_eq!(m.description, "nice glow");
    assert_eq!(m.homepage, "https://nf.example");
    assert_eq!(m.tags, vec!["fav".to_string()]);

    // Replaced by a package that carries its own metadata → adopt it (keep tags).
    let pkg = Manifest {
        name: "Glow".into(),
        version: "3.0".into(),
        author: "NF Studio".into(),
        description: "v3".into(),
        homepage: String::new(),
        tags: vec![],
        install_as: None,
    };
    let m = merged_replace_manifest(existing.clone(), pkg);
    assert_eq!(m.version, "3.0");
    assert_eq!(m.author, "NF Studio");
    assert_eq!(m.description, "v3");
    assert_eq!(m.tags, vec!["fav".to_string()]); // user tags preserved
}

#[test]
fn install_to_ae_routes_by_extension() {
    // A package bundling a script, a plugin and a companion image.
    let work = tmp("work-route");
    let bundle = work.join("MixedTool");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("Tool.jsx"), "// script").unwrap();
    fs::write(bundle.join("Effect.aex"), b"plugin").unwrap();
    fs::write(bundle.join("preview.png"), b"\x89PNG").unwrap();

    let store = LibraryStore::new(tmp("store-route")).unwrap();
    // Import each file individually so files/ has three top-level entries.
    let pkg = importer::import_path(&store, &bundle.join("Tool.jsx")).unwrap();
    importer::add_source(&store, &pkg.id, &bundle.join("Effect.aex")).unwrap();
    importer::add_source(&store, &pkg.id, &bundle.join("preview.png")).unwrap();
    let mut pkg = store.get(&pkg.id).unwrap();
    importer::rescan(&store, &mut pkg).unwrap();

    let ae_root = tmp("ae-route");
    let ae = fake_ae(&ae_root);
    installer::install_to_ae(&store, &pkg, &ae, PackageKind::Script, None).unwrap();

    // .jsx → Scripts, .aex → Plug-ins, companion image follows primary (Scripts).
    assert!(ae_root.join("Scripts/Tool.jsx").exists());
    assert!(ae_root.join("Plug-ins/Effect.aex").exists());
    assert!(ae_root.join("Scripts/preview.png").exists());
    assert!(!ae_root.join("Plug-ins/preview.png").exists());
}

#[test]
fn effect_subdir_nests_plugins() {
    let work = tmp("work-eff");
    let plugin = work.join("NF_Glow.aex");
    fs::write(&plugin, b"plugin").unwrap();

    let store = LibraryStore::new(tmp("store-eff")).unwrap();
    let pkg = importer::import_path(&store, &plugin).unwrap();
    assert_eq!(pkg.kind, PackageKind::Plugin);

    let ae_root = tmp("ae-eff");
    let ae = fake_ae(&ae_root);
    installer::install_to_ae(&store, &pkg, &ae, PackageKind::Plugin, Some("Effects")).unwrap();

    // Plugin nested under Plug-ins/Effects/.
    assert!(ae_root.join("Plug-ins/Effects/NF_Glow.aex").exists());
    assert!(!ae_root.join("Plug-ins/NF_Glow.aex").exists());
}

#[test]
fn distribution_export_import_roundtrip() {
    let work = tmp("work-dist");
    let bundle = work.join("NiceScript");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("NiceScript.jsx"), "// nice").unwrap();
    fs::write(
        bundle.join("manifest.json"),
        r#"{"name":"Nice Script","version":"2.0","author":"Author","tags":["local","fav"]}"#,
    )
    .unwrap();
    fs::write(bundle.join("banner.png"), b"\x89PNG\r\n").unwrap();

    let store = LibraryStore::new(tmp("store-dist")).unwrap();
    let pkg = importer::import_path(&store, &bundle).unwrap();
    assert_eq!(pkg.manifest.tags, vec!["local", "fav"]); // kept locally

    // Export as a distribution zip.
    let zip = work.join("NiceScript.zip");
    archive::export_package(&store, &pkg.id, &zip).unwrap();
    assert!(zip.exists());

    // Import it into a fresh store; manifest + banner + files survive.
    let store2 = LibraryStore::new(tmp("store-dist2")).unwrap();
    let imported = archive::import_archive(&store2, &zip).unwrap();
    assert_eq!(imported.len(), 1);
    let p = &imported[0];
    assert_eq!(p.manifest.name, "Nice Script");
    assert_eq!(p.manifest.version, "2.0");
    assert!(p.manifest.tags.is_empty()); // tags excluded from distribution
    assert_eq!(p.banner.as_deref(), Some("banner.png"));
    assert_eq!(p.kind, PackageKind::Script);
    assert!(store2.files_dir(&p.id).join("NiceScript/NiceScript.jsx").exists());
}

#[test]
fn library_bundle_backup_keeps_tags() {
    // The whole-library backup (export_library) must PRESERVE tags, unlike the
    // per-package distribution export.
    let work = tmp("work-bkp");
    let bundle = work.join("Tagged");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("Tagged.jsx"), "// t").unwrap();
    fs::write(
        bundle.join("manifest.json"),
        r#"{"name":"Tagged","tags":["a","b"]}"#,
    )
    .unwrap();

    let store = LibraryStore::new(tmp("store-bkp")).unwrap();
    let pkg = importer::import_path(&store, &bundle).unwrap();
    assert_eq!(pkg.manifest.tags, vec!["a", "b"]);

    let zip = work.join("backup.zip");
    archive::export_library(&store, &[], &zip).unwrap();

    let store2 = LibraryStore::new(tmp("store-bkp2")).unwrap();
    let imported = archive::import_archive(&store2, &zip).unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].manifest.tags, vec!["a", "b"]); // backup keeps tags
}

#[test]
fn library_backup_preserves_bundle_with_banner() {
    // The whole-library backup must carry bundles AND their banner files, so a
    // series survives a backup → restore round-trip intact.
    let work = tmp("work-bbkp");
    let a = work.join("Alpha.jsx");
    fs::write(&a, "// a").unwrap();

    let store = LibraryStore::new(tmp("store-bbkp")).unwrap();
    let pa = importer::import_path(&store, &a).unwrap();

    // Create a bundle with a banner file on disk.
    let bdir = store.bundle_dir("bid");
    fs::create_dir_all(&bdir).unwrap();
    fs::write(bdir.join("banner.png"), b"\x89PNG-fake").unwrap();
    let bundle = Bundle {
        id: "bid".into(),
        name: "Alpha Series".into(),
        description: String::new(),
        author: "Me".into(),
        homepage: String::new(),
        banner: Some("banner.png".into()),
        package_ids: vec![pa.id.clone()],
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    store.upsert_bundle(bundle).unwrap();

    let zip = work.join("backup.zip");
    archive::export_library(&store, &[], &zip).unwrap();

    let store2 = LibraryStore::new(tmp("store-bbkp2")).unwrap();
    archive::import_archive(&store2, &zip).unwrap();

    let lib2 = store2.load().unwrap();
    assert_eq!(lib2.bundles.len(), 1, "bundle restored");
    let b2 = &lib2.bundles[0];
    assert_eq!(b2.name, "Alpha Series");
    assert_eq!(b2.package_ids.len(), 1);
    assert_eq!(b2.banner.as_deref(), Some("banner.png"), "banner reference kept");
    let banner_path = store2.bundle_dir(&b2.id).join("banner.png");
    assert!(banner_path.exists(), "banner file restored on disk");
    assert_eq!(fs::read(&banner_path).unwrap(), b"\x89PNG-fake");
}

#[test]
fn distribution_export_omits_tags_in_every_manifest() {
    use std::io::Read;

    // A folder that ships its OWN manifest.json containing tags — the tricky
    // case where a nested manifest could leak tags into the distribution.
    let work = tmp("work-tags");
    let bundle = work.join("TagTool");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("TagTool.jsx"), "// x").unwrap();
    fs::write(
        bundle.join("manifest.json"),
        r#"{"name":"Tag Tool","version":"1.0","tags":["secret","local"]}"#,
    )
    .unwrap();

    let store = LibraryStore::new(tmp("store-tags")).unwrap();
    let pkg = importer::import_path(&store, &bundle).unwrap();
    assert_eq!(pkg.manifest.tags, vec!["secret", "local"]); // kept in the library

    let zip_path = work.join("TagTool.zip");
    archive::export_package(&store, &pkg.id, &zip_path).unwrap();

    // Open the produced zip and inspect EVERY manifest.json entry directly.
    let f = fs::File::open(&zip_path).unwrap();
    let mut zip = zip::ZipArchive::new(f).unwrap();
    let mut manifests_seen = 0;
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).unwrap();
        let name = e.name().to_string();
        if name.ends_with("manifest.json") {
            let mut s = String::new();
            e.read_to_string(&mut s).unwrap();
            assert!(!s.contains("\"tags\""), "tags field leaked in {name}: {s}");
            assert!(!s.contains("secret"), "tag value leaked in {name}: {s}");
            manifests_seen += 1;
        }
    }
    // Both the Pluxel root manifest and the bundled nested one were checked.
    assert!(manifests_seen >= 2, "expected >=2 manifest.json entries, saw {manifests_seen}");
}

#[test]
fn cli_ppf_roundtrips_through_import() {
    let work = tmp("work-cli");
    let jsx = work.join("MyEffect.jsx");
    fs::write(&jsx, "// effect").unwrap();
    let ffx = work.join("Preset.ffx");
    fs::write(&ffx, b"ffx data").unwrap();

    let manifest = crate::models::Manifest {
        name: "My Effect".into(),
        version: "1.0.0".into(),
        author: "cubezeero".into(),
        description: String::new(),
        homepage: String::new(),
        tags: Vec::new(),
        install_as: Some(PackageKind::Script),
    };
    let ppf = work.join("MyEffect.ppf");
    archive::write_ppf_from_paths(&ppf, &manifest, &[jsx.clone(), ffx.clone()]).unwrap();
    assert!(ppf.exists());

    // The generated .ppf imports back like any distribution package.
    let store = LibraryStore::new(tmp("store-cli")).unwrap();
    let imported = archive::import_archive(&store, &ppf).unwrap();
    assert_eq!(imported.len(), 1);
    let p = &imported[0];
    assert_eq!(p.manifest.name, "My Effect");
    assert_eq!(p.manifest.version, "1.0.0");
    assert_eq!(p.kind, PackageKind::Script);
    assert!(p.manifest.tags.is_empty()); // no tags in distribution
    assert!(store.files_dir(&p.id).join("MyEffect.jsx").exists());
    assert!(store.files_dir(&p.id).join("Preset.ffx").exists());
}

#[test]
fn cli_run_parses_args_and_detects_kind() {
    let work = tmp("work-clirun");
    let jsx = work.join("Foo.jsx");
    fs::write(&jsx, "// foo").unwrap();
    let out = work.join("Foo.ppf");
    let code = crate::cli::run(&[
        "--name".into(),
        "Foo".into(),
        "--version".into(),
        "1.0.0".into(),
        "--out".into(),
        out.to_string_lossy().into_owned(),
        jsx.to_string_lossy().into_owned(),
    ]);
    assert_eq!(code, 0);
    assert!(out.exists());

    // Missing --name is an error (non-zero exit).
    assert_ne!(crate::cli::run(&[jsx.to_string_lossy().into_owned()]), 0);
}

#[test]
fn install_to_all_targets_every_ae() {
    let work = tmp("work-all");
    let jsx = work.join("Tool.jsx");
    fs::write(&jsx, "// tool").unwrap();

    let store = LibraryStore::new(tmp("store-all")).unwrap();
    let pkg = importer::import_path(&store, &jsx).unwrap();

    let ae1 = tmp("ae-all-1");
    let ae2 = tmp("ae-all-2");
    let aes = vec![fake_ae(&ae1), fake_ae(&ae2)];
    let params = crate::commands::InstallParams {
        installation: None,
        kind: None,
        custom_dir: None,
        label: None,
        elevated: Some(false),
        effect_subdir: None,
    };
    let out = crate::commands::install_to_all(&store, &pkg.id, &aes, &params).unwrap();

    // The script landed in both AEs' Scripts folders, with one record per AE.
    assert!(ae1.join("Scripts/Tool.jsx").exists());
    assert!(ae2.join("Scripts/Tool.jsx").exists());
    assert_eq!(out.installs.len(), 2);
}
