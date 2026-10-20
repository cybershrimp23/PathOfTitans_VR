# PoT VR Launcher build script
# Stages:
#   1. UEVR backend (PoT_UEVR)  -> UEVRBackend.dll
#   2. Launcher (cargo)         -> pot-vr-launcher.exe
#   3. Installer (NSIS)         -> dist\PoTVRLauncher-Setup-<ver>.exe
# Usage:
#   .\build.ps1                 # build everything
#   .\build.ps1 -SkipUevr       # launcher + installer only
#   .\build.ps1 -SkipLauncher   # UEVR only
#   .\build.ps1 -SkipInstaller  # skip the NSIS package
#   .\build.ps1 -Clean          # wipe UEVR build dir first

param(
    [switch]$SkipUevr,
    [switch]$SkipLauncher,
    [switch]$SkipInstaller,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

$RepoRoot   = $PSScriptRoot
# Build the UEVR backend from THIS repo's submodule (not a sibling checkout).
$UevrSrc    = Join-Path $RepoRoot "PoT_UEVR"
$UevrBuild  = Join-Path $UevrSrc "build"
$BackendDll = Join-Path $UevrBuild "bin\uevr\UEVRBackend.dll"

function Find-CMake {
    $cmd = Get-Command cmake -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $fallback = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
    if (Test-Path -LiteralPath $fallback) { return $fallback }
    throw "cmake.exe not found (checked PATH and VS 2022 BuildTools)"
}

function Find-Makensis {
    $cmd = Get-Command makensis -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($p in @(
        "C:\Program Files (x86)\NSIS\makensis.exe",
        "C:\Program Files\NSIS\makensis.exe")) {
        if (Test-Path -LiteralPath $p) { return $p }
    }
    return $null
}

function Cargo-Version {
    $toml = Join-Path $RepoRoot "Cargo.toml"
    $m = Select-String -Path $toml -Pattern '^\s*version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if ($m) { return $m.Matches[0].Groups[1].Value }
    return "0.0.0"
}

function Step($msg) { Write-Host "`n==== $msg ====" -ForegroundColor Cyan }

# ---------------------------------------------------------------- UEVR backend
if (-not $SkipUevr) {
    Step "Building UEVR backend (in-repo PoT_UEVR submodule)"

    if (-not (Test-Path -LiteralPath (Join-Path $UevrSrc "CMakeLists.txt"))) {
        Write-Warning "PoT_UEVR submodule missing/uninitialized at $UevrSrc. Run: git submodule update --init --recursive. Skipping UEVR stage."
    } else {

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

# ------------------------------------------------------------------- Installer
if (-not $SkipInstaller) {
    Step "Packaging installer (NSIS)"

    $makensis = Find-Makensis
    if (-not $makensis) {
        Write-Warning "makensis.exe not found. Install NSIS (winget install NSIS.NSIS) and re-run, or use -SkipInstaller."
    } else {
        $exe = Join-Path $RepoRoot "target\release\pot-vr-launcher.exe"
        if (-not (Test-Path -LiteralPath $exe)) {
            Write-Warning "Launcher exe missing (build without -SkipLauncher first). Skipping installer."
        } else {
            $stage    = Join-Path $RepoRoot "installer\stage"
            $dist     = Join-Path $RepoRoot "dist"
            $uevrBin  = Join-Path $UevrBuild "bin\uevr"
            New-Item -ItemType Directory -Force -Path $stage | Out-Null
            New-Item -ItemType Directory -Force -Path $dist  | Out-Null
            Get-ChildItem -LiteralPath $stage -File -ErrorAction SilentlyContinue | Remove-Item -Force

            Copy-Item -LiteralPath $exe -Destination $stage -Force
            foreach ($dll in @("UEVRBackend.dll","openvr_api.dll","UEVRPluginNullifier.dll","openxr_loader.dll")) {
                $src = Join-Path $uevrBin $dll
                if (Test-Path -LiteralPath $src) { Copy-Item -LiteralPath $src -Destination (Join-Path $stage $dll) -Force }
            }
            $capsule = Join-Path $RepoRoot "assets\vr_image.png"
            if (Test-Path -LiteralPath $capsule) { Copy-Item -LiteralPath $capsule -Destination $stage -Force }

            if (-not (Test-Path -LiteralPath (Join-Path $stage "UEVRBackend.dll"))) {
                Write-Warning "UEVRBackend.dll not staged (UEVR build missing). Skipping installer."
            } else {
                $ver   = Cargo-Version
                $nsi   = Join-Path $RepoRoot "installer\PoTVRLauncher.nsi"
                $setup = Join-Path $dist "PoTVRLauncher-Setup-$ver.exe"
                & $makensis "/DVERSION=$ver" "/DSTAGE_DIR=$stage" "/DOUT_FILE=$setup" $nsi
                if ($LASTEXITCODE -ne 0) {
                    Write-Warning "makensis failed (exit $LASTEXITCODE)."
                } elseif (Test-Path -LiteralPath $setup) {
                    $i = Get-Item -LiteralPath $setup
                    Write-Host ("OK installer: {0}  ({1:N0} bytes)" -f $i.FullName, $i.Length) -ForegroundColor Green
                } else {
                    Write-Warning "makensis succeeded but $setup was not produced."
                }
            }
        }
    }
}

Step "Done"
