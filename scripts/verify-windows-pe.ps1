param(
  [Parameter(Mandatory = $true)]
  [string]$Path,

  [Parameter(Mandatory = $true)]
  [ValidateSet("x64", "arm64")]
  [string]$Architecture,

  [switch]$RequireWindowsGui
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Resolved = (Resolve-Path -LiteralPath $Path).Path
$Bytes = [System.IO.File]::ReadAllBytes($Resolved)
if ($Bytes.Length -lt 64 -or $Bytes[0] -ne 0x4D -or $Bytes[1] -ne 0x5A) {
  throw "Not a valid DOS/PE executable: $Resolved"
}
$PeOffset = [BitConverter]::ToInt32($Bytes, 0x3C)
if ($PeOffset -lt 0 -or $PeOffset + 6 -gt $Bytes.Length) {
  throw "PE header offset is outside the file: $Resolved"
}
if (
  $Bytes[$PeOffset] -ne 0x50 -or
  $Bytes[$PeOffset + 1] -ne 0x45 -or
  $Bytes[$PeOffset + 2] -ne 0 -or
  $Bytes[$PeOffset + 3] -ne 0
) {
  throw "PE signature is invalid: $Resolved"
}

$Machine = [BitConverter]::ToUInt16($Bytes, $PeOffset + 4)
$Expected = if ($Architecture -eq "x64") { 0x8664 } else { 0xAA64 }
if ($Machine -ne $Expected) {
  throw ("Unexpected PE machine 0x{0:X4} for {1}; expected {2}" -f $Machine, $Resolved, $Architecture)
}

if ($RequireWindowsGui) {
  $OptionalHeader = $PeOffset + 24
  if ($OptionalHeader + 70 -gt $Bytes.Length) {
    throw "PE optional header is too small to contain the subsystem field: $Resolved"
  }
  $Subsystem = [BitConverter]::ToUInt16($Bytes, $OptionalHeader + 68)
  if ($Subsystem -ne 2) {
    throw ("Expected a Windows GUI subsystem (2), found {0} in {1}" -f $Subsystem, $Resolved)
  }
}

Write-Host ("Verified {0} PE architecture: {1}" -f $Architecture, $Resolved)
