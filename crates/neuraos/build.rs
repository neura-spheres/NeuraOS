// NeuraOS build script
// Runs before compilation. Verifies the Rust version and prints a friendly
// message to the console so contributors know what's happening.

fn main() {
    // Emit rerun condition — only re-run if build.rs itself changes
    println!("cargo:rerun-if-changed=build.rs");

    // ── Rust version check ────────────────────────────────────────────────────
    // The workspace Cargo.toml already enforces `rust-version = "1.85"` so
    // Cargo will reject older toolchains before we even get here. This block
    // just emits a human-friendly warning as extra context when something seems off.
    let rustc_ver = rustc_version();
    if let Some((major, minor)) = rustc_ver {
        if major < 1 || (major == 1 && minor < 85) {
            // This path is almost unreachable (Cargo already blocks it),
            // but left here as a last-resort guard.
            println!(
                "cargo:warning=\n\
                cargo:warning=╔══════════════════════════════════════════════════════════╗\n\
                cargo:warning=║   NeuraOS · Rust Version Check                           ║\n\
                cargo:warning=╠══════════════════════════════════════════════════════════╣\n\
                cargo:warning=║                                                          ║\n\
                cargo:warning=║   Your Rust version ({major}.{minor}.x) is too old.           ║\n\
                cargo:warning=║   NeuraOS requires Rust >= 1.85 (edition 2024).          ║\n\
                cargo:warning=║                                                          ║\n\
                cargo:warning=║   Update with:                                           ║\n\
                cargo:warning=║     rustup update stable                                 ║\n\
                cargo:warning=║                                                          ║\n\
                cargo:warning=║   Or run the setup script for guided help:               ║\n\
                cargo:warning=║     bash scripts/setup.sh          # Linux / macOS       ║\n\
                cargo:warning=║     .\\scripts\\setup.ps1             # Windows            ║\n\
                cargo:warning=║                                                          ║\n\
                cargo:warning=╚══════════════════════════════════════════════════════════╝\n\
                cargo:warning=",
                major = major,
                minor = minor,
            );
        } else {
            // All good — print a subtle build banner so contributors see
            // that the build script ran and things look healthy.
            println!(
                "cargo:warning= NeuraOS · rustc {major}.{minor}.x detected — minimum met (1.85)",
                major = major,
                minor = minor,
            );
        }
    }
}

/// Parses the running rustc version from the CARGO_CFG_* environment variables
/// that Cargo exposes to build scripts.
fn rustc_version() -> Option<(u32, u32)> {
    // Cargo sets CARGO_CFG_TARGET_OS etc., but not the rustc version directly.
    // We can read it from the version string via `rustc --version` at build time,
    // but that requires spawning a process. Instead we rely on the build-metadata
    // env var that rustc itself emits.
    //
    // CARGO_PKG_RUST_VERSION is the *minimum* required, not the actual version.
    // For the actual version we check the compile-time cfg values set by rustc.
    //
    // The cleanest approach without extra crates: use the RUSTC env var that
    // Cargo always sets, pointing to the rustc binary being used.
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());

    let output = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()?;

    let version_str = String::from_utf8(output.stdout).ok()?;
    // Expected format: "rustc 1.85.0 (4d91de4e4 2025-02-17)"
    let ver_part = version_str.trim().split_whitespace().nth(1)?;
    let mut parts = ver_part.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;

    Some((major, minor))
}
