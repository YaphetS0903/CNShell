param(
  [Parameter(Mandatory = $true)]
  [string]$ClientPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Client = (Resolve-Path -LiteralPath $ClientPath).Path
$Stdout = [System.IO.Path]::GetTempFileName()
$Stderr = [System.IO.Path]::GetTempFileName()

try {
  $process = Start-Process `
    -FilePath $Client `
    -ArgumentList @("--self-test") `
    -NoNewWindow `
    -Wait `
    -PassThru `
    -RedirectStandardOutput $Stdout `
    -RedirectStandardError $Stderr
  $stdoutText = [System.IO.File]::ReadAllText($Stdout)
  $stderrText = [System.IO.File]::ReadAllText($Stderr)
  $output = "$stdoutText`n$stderrText".Trim()
  if ($process.ExitCode -ne 0) {
    throw "Mosh encrypted UDP loopback exited with status $($process.ExitCode): $output"
  }
  if ($output -notmatch "encrypted UDP loopback passed") {
    throw "Mosh encrypted UDP loopback did not report success: $output"
  }
  Write-Host $output
} finally {
  Remove-Item -Force -ErrorAction SilentlyContinue $Stdout, $Stderr
}
