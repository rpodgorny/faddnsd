use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Determine the path to version.py relative to the Cargo.toml of faddns_rust.
    // This assumes faddns_rust is a subdirectory of the main git repo,
    // and faddns/version.py is in a sibling directory named 'faddns'.
    // Example structure:
    // your-git-repo/
    // ├── faddns/
    // │   └── version.py
    // ├── faddns_rust/  <-- Current crate
    // │   ├── Cargo.toml
    // │   └── build.rs
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let base_dir = Path::new(&crate_dir).parent().unwrap(); // Up one level from faddns_rust
    let version_py_path = base_dir.join("faddns").join("version.py");

    println!("cargo:rerun-if-changed={}", version_py_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let content = match fs::read_to_string(&version_py_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Failed to read version file at {}: {}. Using default version.",
                version_py_path.display(),
                e
            );
            println!("cargo:rustc-env=FADDNS_VERSION=0.0.0-unknown-fallback-rust");
            return;
        }
    };

    let mut version_str = "0.0.0-parse-error-fallback";
    for line in content.lines() {
        if line.trim_start().starts_with("__version__") {
            if let Some(val_part) = line.split('=').nth(1) {
                let trimmed_val = val_part.trim();
                if (trimmed_val.starts_with('\'') && trimmed_val.ends_with('\'')) ||
                   (trimmed_val.starts_with('"') && trimmed_val.ends_with('"')) {
                    version_str = &trimmed_val[1..trimmed_val.len()-1];
                    break;
                }
            }
        }
    }
    
    let final_version_str = format!("{}-rust", version_str);
    println!("cargo:rustc-env=FADDNS_VERSION={}", final_version_str);
}
