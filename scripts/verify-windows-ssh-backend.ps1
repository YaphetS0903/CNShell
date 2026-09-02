param(
  [Parameter(Mandatory = $false)]
  [ValidateSet("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc")]
  [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Read-CargoFeatureTree {
  param([Parameter(Mandatory = $true)][string]$Package)

  # Windows PowerShell 5.1 wraps native stderr as ErrorRecord objects. Cargo writes normal
  # progress there, so temporarily avoid turning that informational stream into an exception.
  $previousPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $output = (& cargo tree `
      --manifest-path src-tauri/Cargo.toml `
      --target $Target `
      --edges features `
      --invert $Package 2>&1 | Out-String)
    $exitCode = $LASTEXITCODE
  }
  finally {
    $ErrorActionPreference = $previousPreference
  }
  if ($exitCode -ne 0) {
    throw "Unable to resolve Cargo features for $Package on $Target`n$output"
  }
  return $output
}

$ssh2Tree = Read-CargoFeatureTree "ssh2"
$libssh2Tree = Read-CargoFeatureTree "libssh2-sys"

foreach ($required in @(
  'ssh2 feature "openssl-on-win32"',
  'ssh2 feature "vendored-openssl"'
)) {
  if (-not $ssh2Tree.Contains($required)) {
    throw "Windows SSH backend is missing required feature: $required`n$ssh2Tree"
  }
}

foreach ($required in @(
  'libssh2-sys feature "openssl-on-win32"',
  'libssh2-sys feature "vendored-openssl"'
)) {
  if (-not $libssh2Tree.Contains($required)) {
    throw "Windows libssh2 backend is missing required feature: $required`n$libssh2Tree"
  }
}

Write-Host "Verified vendored OpenSSL libssh2 backend for $Target"
