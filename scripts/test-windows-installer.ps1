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
$Database = Join-Path $DataDirectory "cnshell.sqlite"
$Sentinel = Join-Path $DataDirectory "windows-installer-preserve.test"
$CredentialTarget = "com.cnshell.desktop/installer-preserve-$env:GITHUB_RUN_ID-$PID"
$CredentialCreated = $false
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

function Test-WebViewDescendant([int]$ParentProcessId) {
  $processes = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name)
  $pending = [System.Collections.Generic.Queue[uint32]]::new()
  $visited = [System.Collections.Generic.HashSet[uint32]]::new()
  $pending.Enqueue([uint32]$ParentProcessId)
  [void]$visited.Add([uint32]$ParentProcessId)
  while ($pending.Count -gt 0) {
    $parent = $pending.Dequeue()
    foreach ($candidate in $processes | Where-Object { $_.ParentProcessId -eq $parent }) {
      if ($candidate.Name -ieq "msedgewebview2.exe") {
        return $true
      }
      if ($visited.Add([uint32]$candidate.ProcessId)) {
        $pending.Enqueue([uint32]$candidate.ProcessId)
      }
    }
  }
  return $false
}

function Assert-UserData {
  if (-not (Test-Path -LiteralPath $Sentinel -PathType Leaf)) {
    throw "CNshell user-data sentinel is missing: $Sentinel"
  }
  $databaseFile = Get-Item -LiteralPath $Database -ErrorAction SilentlyContinue
  if (-not $databaseFile -or $databaseFile.Length -eq 0) {
    throw "CNshell SQLite database is missing or empty: $Database"
  }
}

function Set-TestCredential {
  & cmdkey.exe "/generic:$CredentialTarget" "/user:CNshellInstallerTest" "/pass:CNSHELL_INSTALLER_PRESERVE_TEST" | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to create the Windows Credential Manager preservation fixture"
  }
  $script:CredentialCreated = $true
}

function Assert-TestCredential {
  $output = & cmdkey.exe "/list:$CredentialTarget" 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0 -or -not $output.Contains($CredentialTarget)) {
    throw "CNshell namespaced credential did not survive the installer lifecycle"
  }
}

function Remove-TestCredential {
  if (-not $script:CredentialCreated) {
    return
  }
  & cmdkey.exe "/delete:$CredentialTarget" 2>&1 | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to remove the Windows Credential Manager preservation fixture"
  }
  $script:CredentialCreated = $false
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
    $databaseFile = Get-Item -LiteralPath $Database -ErrorAction SilentlyContinue
    if (-not $databaseFile -or $databaseFile.Length -eq 0) {
      throw "Installed CNshell did not initialize its SQLite database"
    }
    if (-not (Test-WebViewDescendant $process.Id)) {
      throw "Installed CNshell did not start a WebView2 renderer"
    }
    if (-not $process.CloseMainWindow()) {
      throw "Installed CNshell did not accept a native window close request"
    }
    if (-not $process.WaitForExit(10000)) {
      throw "Installed CNshell did not exit after its native window close request"
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

try {
  Install-CNshell
  Assert-CNshellStarts
  New-Item -ItemType Directory -Force $DataDirectory | Out-Null
  [System.IO.File]::WriteAllText($Sentinel, "preserve across upgrade and uninstall")
  Set-TestCredential
  Assert-UserData
  Assert-TestCredential

  Install-CNshell
  Assert-UserData
  Assert-TestCredential
  Assert-CNshellStarts

  Uninstall-CNshell
  Assert-UserData
  Assert-TestCredential

  Install-CNshell
  Assert-UserData
  Assert-TestCredential
  Assert-CNshellStarts
  Uninstall-CNshell
  Assert-UserData
  Assert-TestCredential

  Write-Host "CNshell NSIS install, frontend startup, native close, upgrade, uninstall, SQLite, credential, and reinstall gates passed."
} finally {
  Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $Sentinel
  Remove-TestCredential
}
