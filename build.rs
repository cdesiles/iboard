//! Embeds `macos/Info.plist` in the binaries so macOS names the app "iboard"
//! in the menu bar even when it runs outside an `.app` bundle, and compiles
//! the Swift code: the shim that names new issues with Apple's on-device model
//! (`macos/titles.swift`), and the two WebKit helpers run as subprocesses, the
//! preview renderer (`macos/render.swift`) and the live catalog window
//! (`macos/catalog.swift`). The binaries embed the helpers, see
//! `src/design/helpers.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The WebKit helpers: each runs as its own process, so WebKit never loads
/// into the app's. Binary, source, cfg, the variable naming its path, and what
/// a build without it loses.
const HELPERS: [(&str, &str, &str, &str, &str); 2] = [
    (
        "iboard-render",
        "macos/render.swift",
        "apple_render",
        "IBOARD_RENDER_HELPER",
        "the component renderer; component previews are unavailable",
    ),
    (
        "iboard-catalog",
        "macos/catalog.swift",
        "apple_catalog",
        "IBOARD_CATALOG_HELPER",
        "the live catalog window; the design catalog opens in the browser instead",
    ),
];

fn main() -> Result<(), std::env::VarError> {
    println!("cargo:rerun-if-changed=macos/Info.plist");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    for cfg in ["apple_titles", "apple_render", "apple_catalog"] {
        println!("cargo::rustc-check-cfg=cfg({cfg})");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return Ok(());
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    let plist = manifest.join("macos/Info.plist");
    println!(
        "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );

    let swift = match Swift::find() {
        Ok(swift) => swift,
        Err(why) => {
            println!("cargo:warning=building without Swift ({why}): no on-device titles");
            for (_, _, _, _, lost) in HELPERS {
                println!("cargo:warning=building without {lost}");
            }
            return Ok(());
        }
    };

    match swift.titles_shim(&manifest, &out) {
        Ok(()) => println!("cargo:rustc-cfg=apple_titles"),
        Err(why) => println!(
            "cargo:warning=building without on-device titles ({why}); \
             the composer falls back to the review agent or the first line"
        ),
    }
    for (binary, source, cfg, variable, lost) in HELPERS {
        let path = out.join(binary);
        match swift.compile(&manifest.join(source), &[], &path) {
            Ok(()) => {
                println!("cargo:rustc-cfg={cfg}");
                println!("cargo:rustc-env={variable}={}", path.display());
            }
            Err(why) => println!("cargo:warning=building without {lost} ({why})"),
        }
    }
    Ok(())
}

/// The Swift compiler and macOS SDK, for the architecture being built.
struct Swift {
    swiftc: String,
    sdk: String,
    target: String,
}

impl Swift {
    fn find() -> Result<Self, String> {
        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64",
            Ok("x86_64") => "x86_64",
            other => return Err(format!("unsupported architecture {other:?}")),
        };
        let swiftc = xcrun(&["--find", "swiftc"]).ok_or("swiftc not found")?;
        let sdk = xcrun(&["--sdk", "macosx", "--show-sdk-path"]).ok_or("macOS SDK not found")?;
        Ok(Self {
            swiftc,
            sdk,
            target: format!("{arch}-apple-macos12.0"),
        })
    }

    /// Compiles `source` to `output`, an executable unless `args` say
    /// otherwise.
    fn compile(&self, source: &Path, args: &[&str], output: &Path) -> Result<(), String> {
        println!("cargo:rerun-if-changed={}", source.display());
        let status = Command::new(&self.swiftc)
            // Cargo's environment (SDKROOT, deployment target) is meant for rustc.
            .env_remove("SDKROOT")
            .env_remove("MACOSX_DEPLOYMENT_TARGET")
            .args(["-sdk", &self.sdk, "-O", "-target", &self.target])
            .args(args)
            .arg("-o")
            .arg(output)
            .arg(source)
            .status()
            .map_err(|err| format!("couldn't run swiftc: {err}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("swiftc exited with {status}"))
        }
    }

    /// Compiles `macos/titles.swift` into a static library linked with the
    /// Swift runtime. FoundationModels is weak-linked, so the binary still
    /// starts on a Mac without it; the shim checks availability at runtime.
    fn titles_shim(&self, manifest: &Path, out: &Path) -> Result<(), String> {
        self.compile(
            &manifest.join("macos/titles.swift"),
            &[
                "-emit-library",
                "-static",
                "-parse-as-library",
                "-module-name",
                "IboardTitles",
            ],
            &out.join("libiboard_titles.a"),
        )?;
        // The Swift runtime: the toolchain's copy to link against, the
        // system's copy (macOS 12+) at run time.
        let toolchain = Path::new(&self.swiftc)
            .parent()
            .and_then(Path::parent)
            .ok_or("unexpected swiftc location")?
            .join("lib/swift/macosx");
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-search=native={}", toolchain.display());
        println!("cargo:rustc-link-search=native={}/usr/lib/swift", self.sdk);
        println!("cargo:rustc-link-lib=static=iboard_titles");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
        Ok(())
    }
}

fn xcrun(args: &[&str]) -> Option<String> {
    let output = Command::new("xcrun").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|path| !path.is_empty())
}
