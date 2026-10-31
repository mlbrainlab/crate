# Builds the Windows installer: release binaries, then Inno Setup
# (packaging/windows/crate.iss). Output in target\dist\:
# Crate-<version>-x64-setup.exe and SHA256SUMS.
#
#   pwsh scripts/package-windows.ps1
#
# Needs Inno Setup 6 (iscc.exe on PATH, or in its default folder):
# winget install JRSoftware.InnoSetup, or choco install innosetup.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
Write-Host "Crate $version"

cargo build --release --locked -p crate-app -p crate-cli
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# The binaries must run on a fresh Windows: no Visual C++ runtime DLLs
# (.cargo/config.toml links the C runtime in).
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$dumpbin = & $vswhere -latest -products * -find "VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe" | Select-Object -First 1
if (-not $dumpbin) { throw "dumpbin.exe not found (Visual Studio's C++ tools)" }
foreach ($bin in "crate-app.exe", "crate.exe") {
    $deps = & $dumpbin /nologo /dependents "target\release\$bin" | Where-Object { $_ -match '^\s+\S+\.dll$' } | ForEach-Object { $_.Trim() }
    $runtime = $deps | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt-)' }
    if ($runtime) { throw "$bin needs DLLs a fresh Windows doesn't have: $($runtime -join ', ')" }
    Write-Host "$bin needs: $($deps -join ', ')"
}

$iscc = Get-Command iscc -ErrorAction SilentlyContinue
$iscc = if ($iscc) { $iscc.Source } else {
    @("$env:ProgramFiles (x86)\Inno Setup 6\ISCC.exe",
      "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
      "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw "Inno Setup 6 not found (iscc.exe)" }

New-Item -ItemType Directory -Force target\dist | Out-Null
& $iscc /Qp "/DVersion=$version" "/DBin=..\..\target\release" packaging\windows\crate.iss
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Same format as sha256sum, so `sha256sum -c` checks it anywhere.
Push-Location target\dist
Get-ChildItem *.exe | ForEach-Object {
    "$((Get-FileHash $_ -Algorithm SHA256).Hash.ToLower())  $($_.Name)"
} | Set-Content -Encoding ascii SHA256SUMS
Get-Content SHA256SUMS
Pop-Location
