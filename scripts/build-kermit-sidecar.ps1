$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Work = Join-Path $Root "src-tauri\target\kermit-sidecar-windows"
$Source = Join-Path $Work "source"
$Build = Join-Path $Work "build"
$Output = Join-Path $Root "src-tauri\resources\kermit"
$PortSource = Join-Path $Root "scripts\kermit-windows"
$Archive = Join-Path $Output "source\gku201.tar.gz"
$ArchiveSha256 = "19f9ac00d7b230d0a841928a25676269363c2925afc23e62704cde516fc1abbd"
$Architecture = if ($env:CNSHELL_WINDOWS_ARCH) { $env:CNSHELL_WINDOWS_ARCH } else { "x64" }
if ($Architecture -notin @("x64", "arm64")) {
  throw "CNSHELL_WINDOWS_ARCH must be x64 or arm64"
}
$CMakeArchitecture = if ($Architecture -eq "arm64") { "ARM64" } else { "x64" }

function Get-VisualStudioGenerator([string]$TargetArchitecture) {
  $candidates = @()
  $onPath = Get-Command vswhere.exe -ErrorAction SilentlyContinue
  if ($onPath) { $candidates += $onPath.Source }
  if (${env:ProgramFiles(x86)}) {
    $candidates += Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  }
  $vswhere = $candidates |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Select-Object -First 1
  if (-not $vswhere) {
    throw "vswhere.exe is required to locate the installed MSVC toolchain"
  }

  $component = if ($TargetArchitecture -eq "arm64") {
    "Microsoft.VisualStudio.Component.VC.Tools.ARM64"
  } else {
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64"
  }
  $installationVersion = @(& $vswhere `
    -latest `
    -products * `
    -requires $component `
    -property installationVersion) |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
    Select-Object -First 1
  if ($LASTEXITCODE -ne 0 -or -not $installationVersion) {
    throw "No Visual Studio installation contains the required $component toolchain"
  }

  $major = [int]($installationVersion.Split('.')[0])
  $cmakeHelp = & cmake.exe --help 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect CMake generators" }
  $match = [regex]::Match($cmakeHelp, "Visual Studio $major \d{4}")
  if (-not $match.Success) {
    throw "CMake does not support the installed Visual Studio $major toolchain"
  }
  return $match.Value
}

if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) {
  throw "Pinned G-Kermit source archive is missing: $Archive"
}
$actualHash = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $ArchiveSha256) {
  throw "G-Kermit source checksum mismatch ($actualHash)"
}
foreach ($path in @(
  "CMakeLists.txt",
  "gkermit-windows-compat.h",
  "gkermit-windows-main.c",
  "gkermit-windows-io.c"
)) {
  if (-not (Test-Path -LiteralPath (Join-Path $PortSource $path) -PathType Leaf)) {
    throw "G-Kermit Windows port source is missing: $path"
  }
}

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Source, $Build
foreach ($directory in @(
  $Source,
  (Join-Path $Output "licenses"),
  (Join-Path $Output "source\windows-port")
)) {
  New-Item -ItemType Directory -Force $directory | Out-Null
}
& tar.exe -xzf $Archive -C $Source
if ($LASTEXITCODE -ne 0) { throw "Unable to extract G-Kermit source" }

$Generator = Get-VisualStudioGenerator $Architecture
Write-Host "Configuring G-Kermit with $Generator for $Architecture"
& cmake.exe `
  -S $PortSource `
  -B $Build `
  -G $Generator `
  -A $CMakeArchitecture `
  "-DGK_SOURCE_DIR=$Source"
if ($LASTEXITCODE -ne 0) { throw "Unable to configure G-Kermit" }
& cmake.exe --build $Build --config Release --target gkermit --parallel
if ($LASTEXITCODE -ne 0) { throw "Unable to build G-Kermit" }

$Helper = Get-ChildItem -Path $Build -Recurse -Filter "gkermit.exe" |
  Where-Object { $_.FullName -match "Release" } |
  Select-Object -First 1
if (-not $Helper -or $Helper.Length -eq 0) { throw "gkermit.exe was not generated" }
Remove-Item -Force -ErrorAction SilentlyContinue `
  (Join-Path $Output "gkermit"), `
  (Join-Path $Output "gkermit.exe")
Copy-Item -Force $Helper.FullName (Join-Path $Output "gkermit.exe")
Copy-Item -Force (Join-Path $Source "COPYING") (Join-Path $Output "licenses\G-Kermit-GPL-2.0.txt")
Copy-Item -Force (Join-Path $PortSource "*") (Join-Path $Output "source\windows-port")
Copy-Item -Force $PSCommandPath (Join-Path $Output "source\windows-port\build-kermit-sidecar.ps1")

$notice = @"
# G-Kermit third-party notice

- G-Kermit 2.01: GPL-2.0-or-later. Project: https://www.kermitproject.org/gkermit.html
- Original corresponding source: source/gku201.tar.gz (SHA-256: $ArchiveSha256).
- CNshell Windows external-protocol adapter source: source/windows-port/.
- The Windows binary supports only CNshell's external pipe mode and statically links the MSVC runtime; it does not require MSYS2.
"@
[System.IO.File]::WriteAllText((Join-Path $Output "THIRD_PARTY_NOTICES.md"), $notice)

$Built = Join-Path $Output "gkermit.exe"
& (Join-Path $Root "scripts\verify-windows-pe.ps1") $Built $Architecture
if ($Architecture -eq "x64") {
  $stdout = Join-Path $Work "gkermit-help.stdout"
  $stderr = Join-Path $Work "gkermit-help.stderr"
  Remove-Item -Force -ErrorAction SilentlyContinue $stdout, $stderr
  $process = Start-Process `
    -FilePath $Built `
    -ArgumentList "-h" `
    -NoNewWindow `
    -Wait `
    -PassThru `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr
  $help = "$(Get-Content -Raw -ErrorAction SilentlyContinue $stdout)`n$(Get-Content -Raw -ErrorAction SilentlyContinue $stderr)"
  Remove-Item -Force -ErrorAction SilentlyContinue $stdout, $stderr
  if ($process.ExitCode -ne 0 -or $help -notmatch "G-Kermit 2\.01") {
    throw "G-Kermit x64 runtime smoke failed"
  }
}

Write-Host "G-Kermit Windows helper generated: $Built ($Architecture)"
