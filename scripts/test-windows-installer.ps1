param(
  [Parameter(Mandatory = $true)]
  [string]$InstallerPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($env:CI -ne "true") {
  throw "This installer lifecycle test is restricted to an ephemeral CI account"
}

$Installer = (Resolve-Path -LiteralPath $InstallerPath).Path
$InstallDirectory = Join-Path $env:LOCALAPPDATA "CNshell"
$DataDirectory = Join-Path $env:APPDATA "com.cnshell.desktop"
$Sentinel = Join-Path $DataDirectory "windows-installer-preserve.test"
$DesktopShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "CNshell.lnk"
$StartMenuShortcut = Join-Path ([Environment]::GetFolderPath("StartMenu")) "Programs\CNshell\CNshell.lnk"

function Invoke-CheckedProcess([string]$Path, [string[]]$Arguments) {
  $process = Start-Process -FilePath $Path -ArgumentList $Arguments -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "$Path exited with status $($process.ExitCode)"
  }
}

function Install-CNshell {
  Invoke-CheckedProcess $Installer @("/S")
  if (-not (Test-Path -LiteralPath $InstallDirectory -PathType Container)) {
    throw "CNshell install directory was not created: $InstallDirectory"
  }
  if (-not (Test-Path -LiteralPath $StartMenuShortcut -PathType Leaf)) {
    throw "CNshell start menu shortcut was not created: $StartMenuShortcut"
  }
  if (Test-Path -LiteralPath $DesktopShortcut) {
    throw "CNshell created a desktop shortcut without an explicit user choice"
  }
}

function Get-CNshellExecutable {
  $candidate = Get-ChildItem -LiteralPath $InstallDirectory -Filter "*.exe" -File |
    Where-Object { $_.Name -ne "uninstall.exe" -and $_.BaseName -ieq "cnshell" } |
    Select-Object -First 1
  if (-not $candidate) {
    throw "Installed CNshell executable was not found in $InstallDirectory"
  }
  return $candidate.FullName
}

function Assert-CNshellStarts {
  $Executable = Get-CNshellExecutable
  $preflight = & $Executable --rdp-preflight
  if ($LASTEXITCODE -ne 0 -or ($preflight -join "`n") -notmatch '"available"\s*:') {
    throw "Installed CNshell executable failed its command-line startup preflight"
  }
  $process = Start-Process -FilePath $Executable -PassThru
  try {
    Start-Sleep -Seconds 5
    if ($process.HasExited) {
      throw "Installed CNshell UI exited during the startup probe with status $($process.ExitCode)"
    }
  } finally {
    if (-not $process.HasExited) {
      Stop-Process -Id $process.Id -Force
      $process.WaitForExit()
    }
  }
}

function Uninstall-CNshell {
  $uninstaller = Join-Path $InstallDirectory "uninstall.exe"
  if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
    throw "CNshell uninstaller was not created: $uninstaller"
  }
  Invoke-CheckedProcess $uninstaller @("/S")
  if (Test-Path -LiteralPath (Join-Path $InstallDirectory "CNshell.exe")) {
    throw "CNshell executable remains after uninstall"
  }
}

if (Test-Path -LiteralPath (Join-Path $InstallDirectory "uninstall.exe")) {
  Uninstall-CNshell
}

Install-CNshell
Assert-CNshellStarts
New-Item -ItemType Directory -Force $DataDirectory | Out-Null
[System.IO.File]::WriteAllText($Sentinel, "preserve across upgrade and uninstall")

Install-CNshell
if (-not (Test-Path -LiteralPath $Sentinel -PathType Leaf)) {
  throw "CNshell in-place upgrade removed user data"
}
Assert-CNshellStarts

Uninstall-CNshell
if (-not (Test-Path -LiteralPath $Sentinel -PathType Leaf)) {
  throw "CNshell uninstall removed user data without explicit consent"
}

Install-CNshell
if (-not (Test-Path -LiteralPath $Sentinel -PathType Leaf)) {
  throw "CNshell reinstall could not see preserved user data"
}
Assert-CNshellStarts
Uninstall-CNshell

Remove-Item -Force -LiteralPath $Sentinel
Write-Host "CNshell NSIS install, upgrade, uninstall, data preservation, and reinstall gates passed."
