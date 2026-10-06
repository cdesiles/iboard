//! Embeds `macos/Info.plist` in the binary so macOS names the app "iboard"
//! in the menu bar even when it runs outside an `.app` bundle, and compiles
//! the Swift shim that names new issues with Apple's on-device model
//! (`macos/titles.swift`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=macos/Info.plist");
    println!("cargo::rustc-check-cfg=cfg(apple_titles)");
    println!("cargo::rustc-check-cfg=cfg(apple_render)");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let plist = manifest.join("macos/Info.plist");
    println!(
        "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );

    match build_titles_shim(&manifest) {
        Ok(()) => println!("cargo:rustc-cfg=apple_titles"),
        Err(why) => println!(
            "cargo:warning=building without on-device titles ({why}); \
             the composer falls back to the review agent or the first line"
        ),
    }

    match build_render_helper(&manifest) {
        Ok(path) => {
            println!("cargo:rustc-cfg=apple_render");
            println!("cargo:rustc-env=IBOARD_RENDER_HELPER={}", path.display());
        }
        Err(why) => println!(
            "cargo:warning=building without the component renderer ({why}); \
             design previews stay unavailable and iboard opens the HTML instead"
        ),
    }
}

/// Compiles `macos/render.swift` into a small executable the app runs as a
/// short-lived helper to snapshot an HTML preview to a PNG. Keeping WebKit out
/// of the app's own process is deliberate: startup stays instant and a render
/// cannot take the board down.
fn build_render_helper(manifest: &Path) -> Result<PathBuf, String> {
    let source = manifest.join("macos/render.swift");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let binary = out.join("iboard-render");
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => return Err(format!("unsupported architecture {other:?}")),
    };

    let swiftc = xcrun(&["--find", "swiftc"]).ok_or("swiftc not found")?;
    let sdk = xcrun(&["--sdk", "macosx", "--show-sdk-path"]).ok_or("macOS SDK not found")?;
    let status = Command::new(&swiftc)
        .env_remove("SDKROOT")
        .env_remove("MACOSX_DEPLOYMENT_TARGET")
        .args(["-sdk", &sdk])
        .args(["-O"])
        .args(["-target", &format!("{arch}-apple-macos12.0")])
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .status()
        .map_err(|err| format!("couldn't run swiftc: {err}"))?;
    if !status.success() {
        return Err(format!("swiftc exited with {status}"));
    }
    Ok(binary)
}

/// Compiles `macos/titles.swift` into a static library and links it with the
/// Swift runtime. FoundationModels is weak-linked, so the binary still starts
/// on a Mac without it; the shim checks availability at runtime.
fn build_titles_shim(manifest: &Path) -> Result<(), String> {
    let source = manifest.join("macos/titles.swift");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => return Err(format!("unsupported architecture {other:?}")),
    };

    let swiftc = xcrun(&["--find", "swiftc"]).ok_or("swiftc not found")?;
    let sdk = xcrun(&["--sdk", "macosx", "--show-sdk-path"]).ok_or("macOS SDK not found")?;
    let status = Command::new(&swiftc)
        // Cargo's environment (SDKROOT, deployment target) is meant for rustc.
        .env_remove("SDKROOT")
        .env_remove("MACOSX_DEPLOYMENT_TARGET")
        .args(["-sdk", &sdk])
        .args(["-emit-library", "-static", "-O", "-parse-as-library"])
        .args(["-module-name", "IboardTitles"])
        .args(["-target", &format!("{arch}-apple-macos12.0")])
        .arg("-o")
        .arg(out.join("libiboard_titles.a"))
        .arg(&source)
        .status()
        .map_err(|err| format!("couldn't run swiftc: {err}"))?;
    if !status.success() {
        return Err(format!("swiftc exited with {status}"));
    }

    // The Swift runtime: the toolchain's copy to link against, the system's
    // copy (macOS 12+) at run time.
    let toolchain = Path::new(&swiftc)
        .parent()
        .and_then(Path::parent)
        .ok_or("unexpected swiftc location")?
        .join("lib/swift/macosx");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-search=native={}", toolchain.display());
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    println!("cargo:rustc-link-lib=static=iboard_titles");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
    Ok(())
}

fn xcrun(args: &[&str]) -> Option<String> {
    let output = Command::new("xcrun").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|path| !path.is_empty())
}
