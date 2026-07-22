use sha2::{Digest, Sha256};

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let executable = if target_os == "windows" {
        "cnshell-mcp.exe"
    } else {
        "cnshell-mcp"
    };
    let path = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join("mcp")
        .join(executable);
    println!("cargo:rerun-if-changed={}", path.display());
    let digest = std::fs::read(&path)
        .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
        .unwrap_or_else(|_| "unavailable".into());
    println!("cargo:rustc-env=CNSHELL_MCP_SIDECAR_SHA256={digest}");
    tauri_build::build()
}
