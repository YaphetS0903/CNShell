$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Work = Join-Path $Root "src-tauri\target\mosh-sidecar-windows"
$Downloads = Join-Path $Work "downloads"
$Sources = Join-Path $Work "sources"
$Output = Join-Path $Root "src-tauri\resources\mosh"
$PortSource = Join-Path $Root "scripts\mosh-windows"
$Architecture = if ($env:CNSHELL_WINDOWS_ARCH) { $env:CNSHELL_WINDOWS_ARCH } else { "x64" }
if ($Architecture -notin @("x64", "arm64")) {
  throw "CNSHELL_WINDOWS_ARCH must be x64 or arm64"
}

$MoshVersion = "1.4.0"
$MoshSha256 = "872e4b134e5df29c8933dff12350785054d2fd2839b5ae6b5587b14db1465ddd"
$ProtobufVersion = "21.12"
$ProtobufSha256 = "2c6a36c7b5a55accae063667ef3c55f2642e67476d96d355ff0acb13dbb47f09"
$VcpkgCommit = "908da3a305a0a8028d9602ab241b433652b3df69"
$Triplet = if ($Architecture -eq "arm64") { "arm64-windows-static" } else { "x64-windows-static" }
$CMakeArchitecture = if ($Architecture -eq "arm64") { "ARM64" } else { "x64" }
$MoshArchive = Join-Path $Downloads "mosh-$MoshVersion.tar.gz"
$ProtobufArchive = Join-Path $Downloads "protobuf-all-$ProtobufVersion.tar.gz"
$MoshSource = Join-Path $Sources "mosh"
$ProtobufSource = Join-Path $Sources "protobuf"
$ProtobufTarget = Join-Path $Work "protobuf-$Architecture"
$ProtobufHost = if ($Architecture -eq "arm64") {
  Join-Path $Work "protobuf-host-x64"
} else {
  $ProtobufTarget
}
$Generated = Join-Path $Work "generated-protobuf"
$Build = Join-Path $Work "build-$Architecture"
$Vcpkg = Join-Path $Root "src-tauri\target\freerdp-sidecar-windows\vcpkg"

New-Item -ItemType Directory -Force `
  $Downloads, `
  $Sources, `
  (Join-Path $Output "licenses"), `
  (Join-Path $Output "source") | Out-Null

function Get-CheckedFile([string]$Url, [string]$Path, [string]$Sha256) {
  $valid = Test-Path -LiteralPath $Path -PathType Leaf
  if ($valid) {
    $valid = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -eq $Sha256
  }
  if (-not $valid) {
    Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $Path
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Path
  }
  $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $Sha256) { throw "Checksum mismatch for $Path ($actual)" }
}

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
  if (-not $vswhere) { throw "vswhere.exe is required to locate the installed MSVC toolchain" }
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
  if (-not $match.Success) { throw "CMake does not support the installed Visual Studio $major toolchain" }
  return $match.Value
}

function Expand-CheckedArchive([string]$Archive, [string]$Destination, [string]$Marker) {
  if (Test-Path -LiteralPath (Join-Path $Destination $Marker) -PathType Leaf) { return }
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Destination
  New-Item -ItemType Directory -Force $Destination | Out-Null
  & tar.exe -xzf $Archive --strip-components=1 -C $Destination
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $Destination $Marker) -PathType Leaf)) {
    throw "Unable to extract $Archive"
  }
}

function Build-Protobuf([string]$TargetArchitecture, [string]$Prefix, [bool]$BuildProtoc) {
  $library = Join-Path $Prefix "lib\libprotobuf.lib"
  $protoc = Join-Path $Prefix "bin\protoc.exe"
  if ((Test-Path -LiteralPath $library -PathType Leaf) -and
      ((-not $BuildProtoc) -or (Test-Path -LiteralPath $protoc -PathType Leaf))) {
    return
  }
  $generator = Get-VisualStudioGenerator $TargetArchitecture
  $cmakeArchitecture = if ($TargetArchitecture -eq "arm64") { "ARM64" } else { "x64" }
  $protobufBuild = Join-Path $Work "protobuf-build-$TargetArchitecture"
  if ($BuildProtoc -and $TargetArchitecture -eq "x64" -and $Prefix -ne $ProtobufTarget) {
    $protobufBuild = Join-Path $Work "protobuf-build-host-x64"
  }
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $protobufBuild, $Prefix
  $protocOption = if ($BuildProtoc) { "ON" } else { "OFF" }
  & cmake.exe `
    -S (Join-Path $ProtobufSource "cmake") `
    -B $protobufBuild `
    -G $generator `
    -A $cmakeArchitecture `
    "-DCMAKE_INSTALL_PREFIX=$Prefix" `
    -Dprotobuf_BUILD_TESTS=OFF `
    -Dprotobuf_BUILD_SHARED_LIBS=OFF `
    -Dprotobuf_WITH_ZLIB=OFF `
    -Dprotobuf_MSVC_STATIC_RUNTIME=ON `
    "-Dprotobuf_BUILD_PROTOC_BINARIES=$protocOption"
  if ($LASTEXITCODE -ne 0) { throw "Unable to configure Protobuf $TargetArchitecture" }
  & cmake.exe --build $protobufBuild --config Release --target install --parallel
  if ($LASTEXITCODE -ne 0) { throw "Unable to build Protobuf $TargetArchitecture" }
}

Get-CheckedFile `
  "https://github.com/mobile-shell/mosh/releases/download/mosh-$MoshVersion/mosh-$MoshVersion.tar.gz" `
  $MoshArchive `
  $MoshSha256
Get-CheckedFile `
  "https://github.com/protocolbuffers/protobuf/releases/download/v$ProtobufVersion/protobuf-all-$ProtobufVersion.tar.gz" `
  $ProtobufArchive `
  $ProtobufSha256
Expand-CheckedArchive $MoshArchive $MoshSource "src\network\network.cc"
Expand-CheckedArchive $ProtobufArchive $ProtobufSource "cmake\CMakeLists.txt"

foreach ($path in @(
  "CMakeLists.txt",
  "mosh-client-windows.cc",
  "mosh-windows-compat.cc",
  "mosh-windows-compat.h",
  "network-windows.cc",
  "crypto-windows.cc",
  "locale-utils-windows.cc",
  "terminaldisplayinit-windows.cc",
  "include\config.h",
  "include\select.h",
  "include\mosh-windows-prefix.h"
)) {
  if (-not (Test-Path -LiteralPath (Join-Path $PortSource $path) -PathType Leaf)) {
    throw "Mosh Windows port source is missing: $path"
  }
}

Build-Protobuf $Architecture $ProtobufTarget ($Architecture -eq "x64")
if ($Architecture -eq "arm64") {
  Build-Protobuf "x64" $ProtobufHost $true
}
$Protoc = Join-Path $ProtobufHost "bin\protoc.exe"
if (-not (Test-Path -LiteralPath $Protoc -PathType Leaf)) { throw "Pinned protoc.exe is missing" }
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Generated
New-Item -ItemType Directory -Force $Generated | Out-Null
$ProtoSource = Join-Path $MoshSource "src\protobufs"
& $Protoc `
  "--proto_path=$ProtoSource" `
  "--cpp_out=$Generated" `
  (Join-Path $ProtoSource "hostinput.proto") `
  (Join-Path $ProtoSource "transportinstruction.proto") `
  (Join-Path $ProtoSource "userinput.proto")
if ($LASTEXITCODE -ne 0) { throw "Unable to generate pinned Mosh protobuf sources" }

if (-not (Test-Path (Join-Path $Vcpkg ".git"))) {
  & git.exe clone --filter=blob:none https://github.com/microsoft/vcpkg.git $Vcpkg
  if ($LASTEXITCODE -ne 0) { throw "Unable to clone vcpkg" }
}
& git.exe -C $Vcpkg fetch --depth 1 origin $VcpkgCommit
if ($LASTEXITCODE -ne 0) { throw "Unable to fetch pinned vcpkg commit" }
& git.exe -C $Vcpkg checkout --detach $VcpkgCommit
if ($LASTEXITCODE -ne 0) { throw "Unable to checkout pinned vcpkg commit" }
& (Join-Path $Vcpkg "bootstrap-vcpkg.bat") -disableMetrics
if ($LASTEXITCODE -ne 0) { throw "Unable to bootstrap vcpkg" }
$VcpkgExe = Join-Path $Vcpkg "vcpkg.exe"
& $VcpkgExe install "openssl:$Triplet" --clean-after-build
if ($LASTEXITCODE -ne 0) { throw "Unable to build pinned Windows OpenSSL" }

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Build
$Generator = Get-VisualStudioGenerator $Architecture
$Toolchain = Join-Path $Vcpkg "scripts\buildsystems\vcpkg.cmake"
$ProtobufCMake = Join-Path $ProtobufTarget "lib\cmake\protobuf"
Write-Host "Configuring native Mosh $MoshVersion with $Generator for $Architecture"
& cmake.exe `
  -S $PortSource `
  -B $Build `
  -G $Generator `
  -A $CMakeArchitecture `
  "-DCMAKE_TOOLCHAIN_FILE=$Toolchain" `
  "-DVCPKG_TARGET_TRIPLET=$Triplet" `
  "-DCMAKE_PREFIX_PATH=$ProtobufTarget" `
  "-DProtobuf_DIR=$ProtobufCMake" `
  "-DMOSH_SOURCE_DIR=$MoshSource" `
  "-DMOSH_PROTO_DIR=$Generated"
if ($LASTEXITCODE -ne 0) { throw "Unable to configure native Windows Mosh" }
& cmake.exe --build $Build --config Release --target mosh-client --parallel
if ($LASTEXITCODE -ne 0) { throw "Unable to build native Windows Mosh" }

$Helper = Get-ChildItem -Path $Build -Recurse -Filter "mosh-client.exe" |
  Where-Object { $_.FullName -match "Release" } |
  Select-Object -First 1
if (-not $Helper) { throw "mosh-client.exe was not generated" }
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Output "mosh-client.exe")
Copy-Item -Force $Helper.FullName (Join-Path $Output "mosh-client.exe")
Copy-Item -Force (Join-Path $MoshSource "COPYING") (Join-Path $Output "licenses\Mosh-GPL-3.0-or-later.txt")
Copy-Item -Force (Join-Path $ProtobufSource "LICENSE") (Join-Path $Output "licenses\Protobuf-BSD-3-Clause.txt")
Copy-Item -Force (Join-Path $Vcpkg "installed\$Triplet\share\openssl\copyright") (Join-Path $Output "licenses\OpenSSL-Apache-2.0.txt")
Copy-Item -Force $MoshArchive (Join-Path $Output "source\mosh-$MoshVersion.tar.gz")
Copy-Item -Force $ProtobufArchive (Join-Path $Output "source\protobuf-all-$ProtobufVersion.tar.gz")
$WindowsPortDestination = Join-Path $Output "source\windows-port"
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $WindowsPortDestination
New-Item -ItemType Directory -Force $WindowsPortDestination | Out-Null
Copy-Item -Recurse -Force (Join-Path $PortSource "*") $WindowsPortDestination
Copy-Item -Force $PSCommandPath (Join-Path $WindowsPortDestination "build-mosh-sidecar.ps1")

$Notice = @"
# Mosh third-party notices

- Mosh ${MoshVersion}: GPL-3.0-or-later. Corresponding source: source/mosh-$MoshVersion.tar.gz (SHA-256: $MoshSha256).
- Protocol Buffers ${ProtobufVersion}: BSD-3-Clause. Source: source/protobuf-all-$ProtobufVersion.tar.gz (SHA-256: $ProtobufSha256).
- OpenSSL is statically built by pinned vcpkg commit $VcpkgCommit; its Apache-2.0 license is included.
- CNshell's native WinSock/ConPTY adapter source is in source/windows-port/.
- The helper does not require WSL, MSYS2, Homebrew, or an external Mosh client installation.
"@
[System.IO.File]::WriteAllText((Join-Path $Output "licenses\THIRD_PARTY_NOTICES.md"), $Notice)

$Built = Join-Path $Output "mosh-client.exe"
& (Join-Path $Root "scripts\verify-windows-pe.ps1") $Built $Architecture
if ($Architecture -eq "x64") {
  $colors = & $Built -c 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0 -or $colors.Trim() -ne "256") { throw "Mosh color capability smoke failed" }
  $version = & $Built --version 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0 -or $version -notmatch "mosh 1\.4\.0") { throw "Mosh version smoke failed" }
  $selfTest = & $Built --self-test 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0 -or $selfTest -notmatch "encrypted UDP loopback passed") {
    throw "Mosh encrypted UDP loopback failed: $selfTest"
  }
}

Write-Host "Native Windows Mosh helper generated: $Built ($Architecture, $Triplet)"
