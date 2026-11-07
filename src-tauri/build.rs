fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_macos_helper();
    }
    tauri_build::build()
}

fn run(command: &mut std::process::Command) {
    assert!(
        command
            .status()
            .expect("Cannot run native helper compiler")
            .success(),
        "Native helper compilation failed"
    );
}

fn build_macos_helper() {
    use std::{env, fs, path::PathBuf, process::Command};
    let native = PathBuf::from("native-helper");
    println!("cargo:rerun-if-changed=native-helper");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let helper_dir = PathBuf::from("target/native-helper");
    fs::create_dir_all(&helper_dir).unwrap();
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("Unsupported helper architecture"),
    };
    let flags = [
        "-arch",
        arch,
        "-fobjc-arc",
        "-fblocks",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Wno-deprecated-declarations",
    ];
    run(Command::new("xcrun")
        .args(["clang"])
        .args(flags)
        .args(["-mmacosx-version-min=10.15", "-c"])
        .arg(native.join("Bridge.m"))
        .arg("-o")
        .arg(out.join("Bridge.o")));
    run(Command::new("xcrun")
        .args(["ar", "rcs"])
        .arg(out.join("libpluxel_helper_bridge.a"))
        .arg(out.join("Bridge.o")));
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=pluxel_helper_bridge");
    // Clang lowers @available checks to its availability runtime when the
    // release app retains the macOS 10.15 deployment target.
    let resource = Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
        .expect("Cannot locate Clang runtime");
    assert!(resource.status.success());
    let runtime =
        PathBuf::from(String::from_utf8(resource.stdout).unwrap().trim()).join("lib/darwin");
    println!("cargo:rustc-link-search=native={}", runtime.display());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
    for framework in [
        "Foundation",
        "LocalAuthentication",
        "ServiceManagement",
        "Security",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    run(Command::new("xcrun")
        .args(["clang"])
        .args(flags)
        .arg("-mmacosx-version-min=13.0")
        .arg(native.join("Helper.m"))
        .arg(native.join("Filesystem.m"))
        .args(["-framework", "Foundation", "-framework", "Security"])
        .args([
            "-Wl,-sectcreate,__TEXT,__info_plist,native-helper/Helper-Info.plist",
            "-o",
        ])
        .arg(helper_dir.join("pluxel-helper")));
}
