$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Work = Join-Path $Root "src-tauri\target\freerdp-sidecar-windows"
$Downloads = Join-Path $Work "downloads"
$Sources = Join-Path $Work "sources"
$Output = Join-Path $Root "src-tauri\resources\freerdp"
$Architecture = if ($env:CNSHELL_WINDOWS_ARCH) { $env:CNSHELL_WINDOWS_ARCH } else { "x64" }
if ($Architecture -notin @("x64", "arm64")) {
  throw "CNSHELL_WINDOWS_ARCH must be x64 or arm64"
}

$FreeRdpVersion = "3.28.0"
$FreeRdpSha256 = "2d6e37cd726163c37c2070a9aa38a4624feb6b2d414f4d9dbecd60600e971142"
$VcpkgCommit = "908da3a305a0a8028d9602ab241b433652b3df69"
$Triplet = if ($Architecture -eq "arm64") { "arm64-windows-static" } else { "x64-windows-static" }
$Archive = Join-Path $Downloads "freerdp-$FreeRdpVersion.tar.gz"
$Source = Join-Path $Sources "freerdp"
$Build = Join-Path $Work "build-$Architecture"
$Vcpkg = Join-Path $Work "vcpkg"

New-Item -ItemType Directory -Force $Downloads, $Sources, (Join-Path $Output "licenses") | Out-Null

function Get-CheckedFile([string]$Url, [string]$Path, [string]$Sha256) {
  $valid = Test-Path -LiteralPath $Path
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

Get-CheckedFile `
  "https://github.com/FreeRDP/FreeRDP/archive/refs/tags/$FreeRdpVersion.tar.gz" `
  $Archive `
  $FreeRdpSha256

if (-not (Test-Path (Join-Path $Source "CMakeLists.txt"))) {
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Source
  New-Item -ItemType Directory -Force $Source | Out-Null
  & tar.exe -xzf $Archive --strip-components=1 -C $Source
  if ($LASTEXITCODE -ne 0) { throw "Unable to extract FreeRDP source" }
}

foreach ($Patch in @("freerdp-sdl-user-close.patch", "freerdp-sdl-state-marker.patch")) {
  $PatchPath = Join-Path $Root "scripts\patches\$Patch"
  & git.exe -C $Source apply --check $PatchPath 2>$null
  if ($LASTEXITCODE -eq 0) {
    & git.exe -C $Source apply $PatchPath
    if ($LASTEXITCODE -ne 0) { throw "Unable to apply $Patch" }
  }
}

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
& $VcpkgExe install "openssl:$Triplet" "sdl3:$Triplet" "sdl3-ttf:$Triplet" --clean-after-build
if ($LASTEXITCODE -ne 0) { throw "Unable to build pinned FreeRDP dependencies" }

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Build
$Toolchain = Join-Path $Vcpkg "scripts\buildsystems\vcpkg.cmake"
$Generator = Get-VisualStudioGenerator $Architecture
Write-Host "Configuring FreeRDP with $Generator for $Architecture"
& cmake.exe -S $Source -B $Build `
  -G $Generator `
  -A $Architecture `
  "-DCMAKE_TOOLCHAIN_FILE=$Toolchain" `
  "-DVCPKG_TARGET_TRIPLET=$Triplet" `
  -DBUILD_SHARED_LIBS=OFF `
  -DBUILD_TESTING=OFF `
  -DBUILD_TESTING_INTERNAL=OFF `
  -DWITH_SERVER=OFF `
  -DWITH_SAMPLE=OFF `
  -DWITH_MANPAGES=OFF `
  -DWITH_X11=OFF `
  -DWITH_FFMPEG=OFF `
  -DWITH_SWSCALE=OFF `
  -DWITH_JSON_DISABLED=ON `
  -DWITH_INTERNAL_MD4=ON `
  -DWITH_INTERNAL_MD5=ON `
  -DWITH_INTERNAL_RC4=ON `
  -DWITH_SMARTCARD_EMULATE=OFF `
  -DWITH_SMARTCARD_PCSC=OFF `
  -DWITH_PCSC=OFF `
  -DWITH_AAD=OFF `
  -DCHANNEL_URBDRC=OFF `
  -DCHANNEL_SMARTCARD=OFF `
  -DCHANNEL_PRINTER=OFF `
  -DCHANNEL_SERIAL=OFF `
  -DCHANNEL_PARALLEL=OFF `
  -DWITH_CLIENT_SDL=ON `
  -DWITH_CLIENT_SDL2=OFF `
  -DWITH_CLIENT_SDL3=ON `
  -DWITH_SDL_LINK_SHARED=OFF `
  -DWITH_SDL_IMAGE_DIALOGS=OFF `
  -DWITH_WEBVIEW=OFF `
  -DWITH_CCACHE=OFF `
  -DWITH_CLANG_FORMAT=OFF `
  -DWITHOUT_FREERDP_3x_DEPRECATED=ON
if ($LASTEXITCODE -ne 0) { throw "Unable to configure FreeRDP" }

& cmake.exe --build $Build --config Release --target sdl3-freerdp --parallel
if ($LASTEXITCODE -ne 0) { throw "Unable to build sdl-freerdp" }

$Helper = Get-ChildItem -Path $Build -Recurse -Filter "sdl-freerdp.exe" |
  Where-Object { $_.FullName -match "Release" } |
  Select-Object -First 1
if (-not $Helper) { throw "sdl-freerdp.exe was not generated" }
Copy-Item -Force $Helper.FullName (Join-Path $Output "sdl-freerdp.exe")
Copy-Item -Force (Join-Path $Source "LICENSE") (Join-Path $Output "licenses\FreeRDP-Apache-2.0.txt")
Copy-Item -Force (Join-Path $Root "docs\THIRD_PARTY_NOTICES.md") (Join-Path $Output "licenses\THIRD_PARTY_NOTICES.md")

$Built = Join-Path $Output "sdl-freerdp.exe"
if (-not (Test-Path $Built) -or (Get-Item $Built).Length -eq 0) {
  throw "Windows FreeRDP helper is missing or empty"
}
Write-Host "FreeRDP Windows helper generated: $Built ($Architecture, $Triplet)"
