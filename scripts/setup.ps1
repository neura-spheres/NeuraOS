# =============================================================================
#   NeuraOS - Setup Script  (Windows / PowerShell)
#   Checks that your machine has everything it needs to build NeuraOS.
#   If something's missing, it walks you through what to do step by step.
#
#   Run this from the repo root:
#       powershell -ExecutionPolicy Bypass -File scripts\setup.ps1
# =============================================================================

#Requires -Version 5.1

$e        = [char]27
$R        = "$e[0m"
$BOLD     = "$e[1m"
$ACCENT   = "$e[38;2;122;162;247m"   # #7aa2f7  blue
$FG       = "$e[38;2;192;202;245m"   # #c0caf5
$MUTED    = "$e[38;2;130;140;170m"   # #828caa
$OK_C     = "$e[38;2;158;206;106m"   # #9ece6a  green
$WARN_C   = "$e[38;2;224;175;104m"   # #e0af68  orange
$ERR_C    = "$e[38;2;247;118;142m"   # #f7768e  red
$CYAN     = "$e[38;2;125;207;255m"   # #7dcfff
$PURPLE   = "$e[38;2;187;154;247m"   # #bb9af7

$RUST_MIN   = [Version]"1.85.0"
$RUSTUP_URL = "https://rustup.rs"
$REPO_URL   = "https://github.com/neura-spheres/NeuraOS"

$MissingDeps = 0
$RustOk      = $false

function Write-Banner {
    Write-Host ""
    Write-Host "$ACCENT$BOLD  NeuraOS Setup Script (Windows / PowerShell)$R"
    Write-Host ""
}

function Write-Section([string]$Title) {
    $pad = "-" * [Math]::Max(0, 48 - $Title.Length)
    Write-Host ""
    Write-Host "$MUTED$BOLD  --- $ACCENT$Title$MUTED $pad$R"
    Write-Host ""
}

function Write-CheckOk([string]$msg)   { Write-Host "  $OK_C$BOLD  [OK]   $R  $FG$msg$R" }
function Write-CheckWarn([string]$msg) { Write-Host "  $WARN_C$BOLD  [WARN] $R  $WARN_C$msg$R" }
function Write-CheckErr([string]$msg)  { Write-Host "  $ERR_C$BOLD  [ERR]  $R  $ERR_C$msg$R" }
function Write-Info([string]$msg)      { Write-Host "       $MUTED$msg$R" }
function Write-Link([string]$url)      { Write-Host "       $CYAN->  $url$R" }
function Write-CmdHint([string]$cmd)   { Write-Host "       $PURPLE>  $FG$cmd$R" }
function Write-StepNum([string]$n, [string]$msg) { Write-Host "  $PURPLE$BOLD  [$n]$R  $FG$msg$R" }
function Write-Divider { Write-Host "  $MUTED$('-' * 52)$R" }

function Get-CommandPath([string]$Name) {
    try { (Get-Command $Name -ErrorAction Stop).Source } catch { $null }
}

function Compare-Versions([string]$ver, [Version]$min) {
    try {
        $v = [Version]($ver -replace '-.*$', '')  # strip pre-release suffix
        return ($v -ge $min)
    } catch { return $false }
}

# Imports environment variables from a vcvars*.bat file into the current
# PowerShell process so that cl.exe, link.exe, lib.exe, etc. are all on PATH.
# Uses a temp .bat shim to avoid any quoting edge-cases with Invoke-Expression.
function Import-VcVarsEnv([string]$vcvars) {
    $tmpBat = [IO.Path]::ChangeExtension([IO.Path]::GetTempFileName(), '.bat')
    $tmpOut = [IO.Path]::GetTempFileName()
    try {
        # Write a small .bat that sources vcvars then dumps the environment
        "@echo off`r`ncall `"$vcvars`" >nul 2>&1`r`nset" | Set-Content -Encoding ASCII $tmpBat
        cmd /c $tmpBat 2>$null | Set-Content -Encoding UTF8 $tmpOut
        $lines = Get-Content $tmpOut
        foreach ($line in $lines) {
            # values can contain '=', so only split on the FIRST '='
            $idx = $line.IndexOf('=')
            if ($idx -gt 0) {
                $key = $line.Substring(0, $idx)
                $val = $line.Substring($idx + 1)
                # Use the .NET API so special chars in names are handled safely
                [System.Environment]::SetEnvironmentVariable($key, $val, 'Process')
            }
        }
    } finally {
        Remove-Item $tmpBat, $tmpOut -ErrorAction SilentlyContinue
    }
}

Write-Banner
Write-Section "Checking Requirements"

# ---------------------------------------------------------------------------
# Check: git
# ---------------------------------------------------------------------------
$gitPath = Get-CommandPath "git"
if ($gitPath) {
    $gitVer = (git --version 2>&1) -replace "git version ", ""
    Write-CheckOk "git $gitVer"
} else {
    Write-CheckErr "git is not installed"
    Write-Info "You need git to clone the repo. Install it from:"
    Write-Link "https://git-scm.com/download/win"
    Write-Info "Or with winget:"
    Write-CmdHint "winget install --id Git.Git -e"
    $MissingDeps++
}

# ---------------------------------------------------------------------------
# Check: rustc + cargo
# ---------------------------------------------------------------------------
$rustcPath = Get-CommandPath "rustc"
$cargoPath = Get-CommandPath "cargo"

if ($rustcPath -and $cargoPath) {
    $rustVerRaw  = (rustc --version 2>&1) -replace "rustc ", "" -replace " \(.*\)", ""
    $cargoVerRaw = (cargo --version 2>&1) -replace "cargo ", "" -replace " \(.*\)", ""

    if (Compare-Versions $rustVerRaw $RUST_MIN) {
        Write-CheckOk "rustc $rustVerRaw   (min required: $RUST_MIN - you're good)"
        Write-CheckOk "cargo $cargoVerRaw"
        $RustOk = $true
    } else {
        Write-CheckWarn "rustc $rustVerRaw  <- too old, need >= $RUST_MIN"
        Write-Info "Update Rust with:"
        Write-CmdHint "rustup update stable"
        $MissingDeps++
    }
} else {
    Write-CheckErr "Rust is not installed"
    $RustOk = $false
    $MissingDeps++
}

# ---------------------------------------------------------------------------
# Check: Visual Studio C++ build tools (MSVC linker + compiler)
# NeuraOS depends on 'rusqlite' (bundled), which compiles C source at build
# time, so we need *both* link.exe (linker) and cl.exe (C compiler).
# ---------------------------------------------------------------------------
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vsFound = $false
$vsInstPath = $null

if (Test-Path $vswhere) {
    $vsInstalls = & $vswhere -products * -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64 2>&1
    $vsFound    = ($vsInstalls -match "installationPath")
    if ($vsFound) {
        $vsInstPath = (& $vswhere -latest -products * `
            -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64 `
            -property installationPath 2>$null | Select-Object -First 1)
    }
}

# Fallback: cl.exe already on PATH (e.g. Developer PowerShell, GNU cl shim)
if (-not $vsFound) { $vsFound = $null -ne (Get-CommandPath "cl") }

if ($vsFound) {
    Write-CheckOk "MSVC C++ build tools found"

    # Make sure the full compiler+linker environment is active in this shell.
    # We need cl.exe (C compiler) AND link.exe (linker) — rusqlite's bundled
    # build script requires cl.exe to compile SQLite from source.
    $clOk   = $null -ne (Get-CommandPath "cl")
    $linkOk = $null -ne (Get-CommandPath "link")

    if (-not $clOk -or -not $linkOk) {
        $missing = @()
        if (-not $clOk)   { $missing += "cl.exe (C compiler)" }
        if (-not $linkOk) { $missing += "link.exe (linker)" }
        Write-Info "Missing from PATH: $($missing -join ', ')"
        Write-Info "Configuring MSVC environment automatically..."

        $vcvars = $null
        if ($vsInstPath) {
            $vcvars = Join-Path $vsInstPath 'VC\Auxiliary\Build\vcvars64.bat'
            if (-not (Test-Path $vcvars)) { $vcvars = $null }
        }

        # If the primary install path didn't have it, try vswhere again broadly
        if (-not $vcvars -and (Test-Path $vswhere)) {
            $anyPath = & $vswhere -latest -property installationPath 2>$null | Select-Object -First 1
            if ($anyPath) {
                $candidate = Join-Path $anyPath 'VC\Auxiliary\Build\vcvars64.bat'
                if (Test-Path $candidate) { $vcvars = $candidate }
            }
        }

        if ($vcvars) {
            Import-VcVarsEnv $vcvars
            $clOk   = $null -ne (Get-CommandPath "cl")
            $linkOk = $null -ne (Get-CommandPath "link")

            if ($clOk -and $linkOk) {
                Write-CheckOk "cl.exe + link.exe added to PATH via vcvars64.bat"
            } elseif ($linkOk) {
                Write-CheckWarn "link.exe found but cl.exe still missing after sourcing vcvars64"
                Write-Info "The C compiler (cl.exe) is required for building native dependencies."
                Write-Info "Make sure 'Desktop development with C++' is installed in Visual Studio."
                $MissingDeps++
            } else {
                Write-CheckWarn "Failed to add MSVC tools to PATH from vcvars64.bat"
                Write-Info "Try opening a Developer PowerShell or running:"
                Write-CmdHint "& `"$vcvars`""
                $MissingDeps++
            }
        } else {
            Write-CheckWarn "Could not locate vcvars64.bat in your Visual Studio install"
            Write-Info "Open a 'Developer PowerShell for VS' from the Start menu and re-run this script."
            $MissingDeps++
        }
    } else {
        Write-CheckOk "cl.exe + link.exe already on PATH"
    }
} else {
    # Check for GNU toolchain as fallback
    $gccFound = $null -ne (Get-CommandPath "gcc")
    if ($gccFound) {
        Write-CheckOk "gcc found (GNU toolchain)"
        Write-Info "Note: you may also need to set the Rust target to x86_64-pc-windows-gnu:"
        Write-CmdHint "rustup target add x86_64-pc-windows-gnu"
    } else {
        Write-CheckErr "No C/C++ build tools found (MSVC or GNU)"
        Write-Info "Rust on Windows and NeuraOS's native dependencies need a C linker."
        Write-Info "Easiest option - install Visual Studio Build Tools (free):"
        Write-Link "https://visualstudio.microsoft.com/visual-cpp-build-tools/"
        Write-Info "During setup, select: 'Desktop development with C++'"
        Write-Host ""
        Write-Info "Or with winget:"
        Write-CmdHint "winget install Microsoft.VisualStudio.2022.BuildTools --override `"--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive`""
        Write-Host ""
        Write-Info "Alternative: GNU/MinGW toolchain:"
        Write-Link "https://www.rust-lang.org/tools/install#windows"
        $MissingDeps++
    }
}

Write-Host ""

# ---------------------------------------------------------------------------
# Section 2: Rust install guide (only if Rust is missing)
# ---------------------------------------------------------------------------
if (-not $RustOk) {
    Write-Section "How to Install Rust on Windows"

    Write-Host "  $FG Rust uses a tool called $ACCENT${BOLD}rustup$R$FG to manage everything - it's basically$R"
    Write-Host "  $FG the official installer. It handles Rust, Cargo, and future updates.$R"
    Write-Host "  $FG The whole process takes about 5 minutes.$R"
    Write-Host ""

    Write-Divider
    Write-Host ""

    Write-StepNum "1" "Download and run the rustup installer"
    Write-Host ""
    Write-Info "Open this link in your browser and download 'RUSTUP-INIT.EXE':"
    Write-Link "https://win.rustup.rs/x86_64"
    Write-Host ""
    Write-Info "Or with winget:"
    Write-CmdHint "winget install Rustlang.Rustup"
    Write-Host ""
    Write-Info "Run the installer and press Enter to accept defaults."
    Write-Info "It installs rustc, cargo, and sets up your PATH automatically."
    Write-Host ""

    Write-Divider
    Write-Host ""

    Write-StepNum "2" 'Install MSVC C++ build tools (if not already installed)'
    Write-Host ""
    Write-Info "Rust on Windows needs the C++ linker from Visual Studio."
    Write-Info "You don't need the full VS IDE - just the Build Tools:"
    Write-Link "https://visualstudio.microsoft.com/visual-cpp-build-tools/"
    Write-Host ""
    Write-Info "During the installer, select: 'Desktop development with C++'"
    Write-Host ""

    Write-Divider
    Write-Host ""

    Write-StepNum "3" "Close and reopen your terminal, then verify"
    Write-Host ""
    Write-CmdHint "rustc --version"
    Write-CmdHint "cargo --version"
    Write-Host ""

    Write-Divider
    Write-Host ""

    Write-StepNum "4" "Re-run this script"
    Write-Host ""
    Write-CmdHint "powershell -ExecutionPolicy Bypass -File scripts\setup.ps1"
    Write-Host ""

    Write-Host "  $MUTED--  More info  ----------------------------------------------------------$R"
    Write-Host ""
    Write-Info "Official Rust install guide for Windows:"
    Write-Link "https://doc.rust-lang.org/book/ch01-01-installation.html#installing-rustup-on-windows"
    Write-Host ""
    Write-Info "If you hit issues, the Rust Discord is super helpful:"
    Write-Link "https://discord.gg/rust-lang"
    Write-Host ""

    Write-Host "  $WARN_C$BOLD  ->  Re-run this script after installing Rust.$R"
    Write-Host ""
    exit 1
}

# ---------------------------------------------------------------------------
# Section 3: Other missing deps
# ---------------------------------------------------------------------------
if ($MissingDeps -gt 0) {
    Write-Section "Almost There"
    Write-Host "  $WARN_C Fix the issues above, then re-run:$R"
    Write-Host ""
    Write-CmdHint "powershell -ExecutionPolicy Bypass -File scripts\setup.ps1"
    Write-Host ""
    exit 1
}

# ---------------------------------------------------------------------------
# Section 4: All good - verification build
# ---------------------------------------------------------------------------
Write-Section "Everything Looks Good"

Write-Host "  $OK_C$BOLD All requirements satisfied.$R"
Write-Host ""

# Show the active Rust toolchain/target so users can report it if they hit issues
$rustupShow = rustup show active-toolchain 2>&1
Write-Info "Active toolchain: $rustupShow"
Write-Host ""

Write-Host "  $FG Verifying the build environment with$R $PURPLE cargo check$R$MUTED ...$R"
Write-Host "  $MUTED (This downloads and checks all dependencies. First run can take a few minutes.)$R"
Write-Host ""

# Capture both stdout and stderr so we can show them on failure.
# We intentionally do NOT use --quiet or 2>$null here so errors are visible.
$cargoOut = & cargo check 2>&1
$cargoExit = $LASTEXITCODE

if ($cargoExit -eq 0) {
    Write-CheckOk "Verification build succeeded - toolchain is working correctly"
} else {
    Write-CheckErr "Verification build failed (cargo check returned exit code $cargoExit)"
    Write-Host ""
    Write-Host "  $ERR_C---- cargo output --------------------------------------------------------$R"
    Write-Host ""
    foreach ($line in $cargoOut) {
        Write-Host "    $MUTED$line$R"
    }
    Write-Host ""
    Write-Host "  $ERR_C-------------------------------------------------------------------------$R"
    Write-Host ""
    Write-Info "Diagnostic info:"
    Write-CmdHint "rustup show"
    Write-CmdHint "cargo --version"
    Write-CmdHint "rustc --version --verbose"
    Write-Host ""
    Write-Info "Common fixes:"
    Write-Info "  1. Open a 'Developer PowerShell for VS XXXX' from the Start menu and re-run."
    Write-Info "  2. Make sure 'Desktop development with C++' is installed in Visual Studio."
    Write-Info "  3. Run: rustup update stable"
    Write-Info "  4. Run: rustup component add rust-src"
    Write-Host ""
    Write-Info "If the error mentions a missing crate or feature, it may be a Rust edition"
    Write-Info "issue. Make sure you have Rust >= $RUST_MIN (run 'rustup update stable')."
    Write-Host ""
    exit 1
}

Write-Host ""
Write-Divider
Write-Host ""
Write-Host "  $FG Building in $ACCENT${BOLD}release mode$R$FG - fully optimised, may take a few minutes the$R"
Write-Host "  $FG first time (Rust compiles all dependencies). Subsequent builds are fast.$R"
Write-Host ""

$reply = Read-Host "  $ACCENT${BOLD}Build now? [Y/n]$R"
if ([string]::IsNullOrWhiteSpace($reply)) { $reply = "Y" }

if ($reply -match "^[Yy]") {
    Write-Section "Building NeuraOS"
    Write-Host "  $MUTED Running: $PURPLE cargo build --release$R"
    Write-Host ""

    cargo build --release
    $buildExit = $LASTEXITCODE

    Write-Host ""

    if ($buildExit -eq 0) {
        Write-Section "Done"
        Write-CheckOk "Build complete!  Run NeuraOS with:"
        Write-Host ""
        Write-CmdHint ".\target\release\neuraos.exe"
        Write-Host ""
        Write-Info "Or with cargo:"
        Write-CmdHint "cargo run --release"
        Write-Host ""
        Write-Host "  $FG Once it boots:$R"
        Write-Host "  $MUTED  *  Type $FG help$MUTED in the shell to see what's available$R"
        Write-Host "  $MUTED  *  Press $FG Ctrl+P$MUTED to open the app launcher$R"
        Write-Host "  $MUTED  *  Open Settings to add your AI API key$R"
        Write-Host ""
        Write-Host "  $ACCENT$BOLD Enjoy NeuraOS. Drop a star on GitHub if you like it!$R"
        Write-Link $REPO_URL
        Write-Host ""
    } else {
        Write-Section "Build Failed"
        Write-CheckErr "cargo build --release failed (exit code $buildExit)"
        Write-Host ""
        Write-Info "Check the output above for the specific error."
        Write-Info "If you need help, open an issue at:"
        Write-Link "$REPO_URL/issues"
        Write-Host ""
        exit 1
    }
} else {
    Write-Section "Ready to Build Whenever You Are"
    Write-Host "  $FG Run when ready:$R"
    Write-Host ""
    Write-CmdHint "cargo build --release"
    Write-Host ""
    Write-CmdHint ".\target\release\neuraos.exe"
    Write-Host ""
}
