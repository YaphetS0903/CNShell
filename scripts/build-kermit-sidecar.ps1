$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Output = Join-Path $Root "src-tauri\resources\kermit"
New-Item -ItemType Directory -Force (Join-Path $Output "licenses") | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Output "gkermit.exe")

if ($env:CNSHELL_REQUIRE_ADVANCED_SIDECARS -eq "1") {
  throw "Windows native G-Kermit sidecar has not completed the pinned MSYS2 build gate"
}
Write-Warning "Windows Kermit is disabled for the core Beta until the native x64/ARM64 helper passes its build and runtime gates."
