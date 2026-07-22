$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Manifest = Join-Path $Root "src-tauri\Cargo.toml"
$Output = Join-Path $Root "src-tauri\resources\mcp"
$TargetTriple = if ($env:TAURI_ENV_TARGET_TRIPLE) {
  $env:TAURI_ENV_TARGET_TRIPLE
} elseif ($env:CARGO_BUILD_TARGET) {
  $env:CARGO_BUILD_TARGET
} elseif ($env:CNSHELL_WINDOWS_ARCH -eq "arm64") {
  "aarch64-pc-windows-msvc"
} elseif (-not $env:CNSHELL_WINDOWS_ARCH -or $env:CNSHELL_WINDOWS_ARCH -eq "x64") {
  "x86_64-pc-windows-msvc"
} else {
  throw "Unsupported CNSHELL_WINDOWS_ARCH: $env:CNSHELL_WINDOWS_ARCH"
}

New-Item -ItemType Directory -Force -Path $Output | Out-Null
# The MCP sidecar is launched directly by external MCP hosts.  Keep it
# self-contained so a clean Windows installation does not require the VC++
# redistributable merely to initialize the client credential.
if ([string]::IsNullOrWhiteSpace($env:RUSTFLAGS)) {
  $env:RUSTFLAGS = "-C target-feature=+crt-static"
} else {
  $env:RUSTFLAGS = "$($env:RUSTFLAGS) -C target-feature=+crt-static"
}
& cargo.exe build --manifest-path $Manifest --release --bin cnshell-mcp --target $TargetTriple
if ($LASTEXITCODE -ne 0) { throw "Unable to build cnshell-mcp for $TargetTriple" }
$Built = Join-Path $Root "src-tauri\target\$TargetTriple\release\cnshell-mcp.exe"
if (-not (Test-Path $Built -PathType Leaf)) { throw "cnshell-mcp.exe was not generated" }
Copy-Item -Force $Built (Join-Path $Output "cnshell-mcp.exe")
$License = Join-Path $Root "src-tauri\resources\licenses\rmcp-Apache-2.0.txt"
if (-not (Test-Path $License -PathType Leaf) -or (Get-Item $License).Length -lt 10000) {
  throw "The complete rmcp Apache-2.0 license is missing"
}
Write-Host "CNshell MCP sidecar generated for $TargetTriple"
