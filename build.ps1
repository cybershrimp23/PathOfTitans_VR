# PoT VR Launcher build script
# Stages:
#   1. UEVR backend (PoT_UEVR)  -> UEVRBackend.dll
#   2. Launcher (cargo)         -> pot-vr-launcher.exe
# Usage:
#   .\build.ps1                # build everything
#   .\build.ps1 -SkipUevr      # launcher only
#   .\build.ps1 -SkipLauncher  # UEVR only
#   .\build.ps1 -Clean         # wipe UEVR build dir first

param(
    [switch]$SkipUevr,
    [switch]$SkipLauncher,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

$RepoRoot   = $PSScriptRoot
$UevrSrc    = if (Test-Path -LiteralPath "$env:USERPROFILE\PoT_UEVR") { "$env:USERPROFILE\PoT_UEVR" } else { throw "PoT_UEVR repo not found at $env:USERPROFILE\PoT_UEVR" }
$UevrBuild  = Join-Path $UevrSrc "build"
$BackendDll = Join-Path $UevrBuild "bin\uevr\UEVRBackend.dll"

function Find-CMake {
    $cmd = Get-Command cmake -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $fallback = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
    if (Test-Path -LiteralPath $fallback) { return $fallback }
    throw "cmake.exe not found (checked PATH and VS 2022 BuildTools)"
}

function Step($msg) { Write-Host "`n==== $msg ====" -ForegroundColor Cyan }

# ---------------------------------------------------------------- UEVR backend
if (-not $SkipUevr) {
    Step "Building UEVR backend (PoT_UEVR)"

    if (Get-Process -Name "PathOfTitans-Win64-Shipping" -ErrorAction SilentlyContinue) {
        Write-Warning "Path of Titans is running - close it before building (UEVRBackend.dll may be locked). Continuing to the launcher stage."
    }

    if ($Clean -and (Test-Path -LiteralPath $UevrBuild)) {
        Write-Host "Cleaning $UevrBuild"
        Remove-Item -LiteralPath $UevrBuild -Recurse -Force
    }

    $cmake = Find-CMake

    Step "Initializing submodules (UESDK, imgui, spdlog, glm)"
    & git -C $UevrSrc submodule update --init --recursive
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "git submodule update failed (exit $LASTEXITCODE). The build environment may be incomplete (e.g. the private UESDK submodule). Continuing to the launcher stage."
    }

    Write-Host "Configuring ($cmake)..."
    & $cmake -S $UevrSrc -B $UevrBuild -G "Visual Studio 17 2022" -A x64
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "Configure failed - wiping build dir and retrying once"
        if (Test-Path -LiteralPath $UevrBuild) { Remove-Item -LiteralPath $UevrBuild -Recurse -Force }
        & $cmake -S $UevrSrc -B $UevrBuild -G "Visual Studio 17 2022" -A x64
        if ($LASTEXITCODE -ne 0) { Write-Warning "CMake configure failed on retry (exit $LASTEXITCODE). Continuing to the launcher stage." }
    }

    Write-Host "Building target 'uevr' (Release)..."
    & $cmake --build $UevrBuild --config Release --target uevr
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "UEVR build failed (exit $LASTEXITCODE). Continuing to the launcher stage."
    }

    if (-not (Test-Path -LiteralPath $BackendDll)) {
        Write-Warning "UEVR build did not produce $BackendDll. The build environment may be incomplete (e.g. the private UESDK submodule). Fixing that is outside the launcher's scope - continuing to the launcher stage."
    } else {
        $info = Get-Item -LiteralPath $BackendDll
        Write-Host ("OK: {0}  ({1:N0} bytes, {2})" -f $info.FullName, $info.Length, $info.LastWriteTime) -ForegroundColor Green
    }
}

# --------------------------------------------------------------------- Launcher
if (-not $SkipLauncher) {
    Step "Building launcher (cargo)"

    if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot "Cargo.toml"))) {
        Write-Warning "Cargo.toml not found in $RepoRoot - skipping launcher stage (crate not created yet)"
    } else {
        Push-Location $RepoRoot
        try {
            & cargo build --release
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
        } finally {
            Pop-Location
        }
        $exe = Join-Path $RepoRoot "target\release\pot-vr-launcher.exe"
        if (Test-Path -LiteralPath $exe) {
            Write-Host "OK: $exe" -ForegroundColor Green
        }
    }
}

Step "Done"
