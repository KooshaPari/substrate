# scripts/install.ps1 — install `substrate` CLI binary (Windows / PowerShell)
# Wraps: cargo build --release -p driver-cli
# Default: $env:LOCALAPPDATA\Programs\substrate (override with $Env:INSTALL_DIR)
[CmdletBinding()]
param(
  [string]$InstallDir = "$env:LOCALAPPDATA\Programs\substrate",
  [string]$RepoRoot = (Get-Location).Path,
  [int]$Jobs = $env:NUMBER_OF_PROCESSORS
)

$ErrorActionPreference = "Stop"
$BinaryName = "substrate.exe"
Set-Location -Path $RepoRoot

Write-Host "==> building release binary (jobs=$Jobs)"
cargo build --release -p driver-cli --jobs $Jobs
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$BinPath = Join-Path -Path "target\release" -ChildPath $BinaryName
if (-not (Test-Path $BinPath)) { throw "build did not produce $BinPath" }

if (-not (Test-Path $InstallDir)) {
  New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$Size = (Get-Item $BinPath).Length
Write-Host "==> built $BinPath ($([math]::Round($Size / 1MB, 2)) MB)"

$Dest = Join-Path -Path $InstallDir -ChildPath $BinaryName
Copy-Item -Path $BinPath -Destination $Dest -Force

Write-Host "==> verifying install"
& $Dest --version

# ensure PATH contains install dir (user-scope, no admin)
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
  [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
  Write-Host "==> added $InstallDir to user PATH (restart shell to take effect)"
}

Write-Host ""
Write-Host "substrate installed to $Dest"