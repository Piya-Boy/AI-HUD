#Requires -Version 5.1
<#
.SYNOPSIS
    AI-HUD one-command installer for Windows.

.DESCRIPTION
    Downloads and installs the latest AI-HUD release from GitHub.
    Supports x64 and arm64. Registers autostart. No admin required.

.PARAMETER Portable
    Install into current directory only. No system modifications.

.PARAMETER Silent
    Suppress all interactive prompts.

.PARAMETER Uninstall
    Remove AI-HUD and all its startup entries.

.PARAMETER Version
    Pin a specific release tag (e.g. "v0.2.0"). Defaults to latest.

.EXAMPLE
    irm https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.ps1 | iex
    # or
    irm https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.ps1 | iex; Install-AiHud -Portable
#>
[CmdletBinding()]
param(
    [switch]$Portable,
    [switch]$Silent,
    [switch]$Uninstall,
    [string]$Version = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Constants ─────────────────────────────────────────────────────────────────
$REPO      = "Piya-Boy/AI-HUD"
$APP_NAME  = "AI-HUD"
$EXE_NAME  = "AI-HUD.exe"
$API_BASE  = "https://api.github.com/repos/$REPO"
$RELEASES  = "https://github.com/$REPO/releases"

# ── Color helpers ─────────────────────────────────────────────────────────────
function Write-Step  { param($msg) Write-Host "  $msg" -ForegroundColor Cyan }
function Write-Ok    { param($msg) Write-Host "  $msg" -ForegroundColor Green }
function Write-Warn  { param($msg) Write-Host "  $msg" -ForegroundColor Yellow }
function Write-Fail  { param($msg) Write-Host "  $msg" -ForegroundColor Red }
function Write-Dim   { param($msg) Write-Host "  $msg" -ForegroundColor DarkGray }

function Write-Banner {
    Write-Host ""
    Write-Host "  AI-HUD Installer" -ForegroundColor Magenta
    Write-Host "  ─────────────────────────────────" -ForegroundColor DarkGray
    Write-Host ""
}

# ── Architecture ──────────────────────────────────────────────────────────────
function Get-Arch {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -eq "ARM64") { return "arm64" }
    return "x64"
}

# ── Install path ──────────────────────────────────────────────────────────────
function Get-InstallDir {
    if ($Portable) { return (Get-Location).Path }
    return Join-Path $env:LOCALAPPDATA $APP_NAME
}

# ── Latest release from GitHub API ────────────────────────────────────────────
function Get-LatestVersion {
    if ($Version -ne "") { return $Version }
    try {
        $headers = @{ "User-Agent" = "AI-HUD-Installer/1.0"; "Accept" = "application/vnd.github+json" }
        $rel = Invoke-RestMethod -Uri "$API_BASE/releases/latest" -Headers $headers -TimeoutSec 15
        return $rel.tag_name
    } catch {
        throw "Could not fetch latest version from GitHub: $_"
    }
}

# ── Resolve download URL for our platform ─────────────────────────────────────
function Get-DownloadUrl {
    param([string]$tag)
    $arch = Get-Arch
    # tauri-action produces:  AI-HUD_0.1.0_x64-setup.exe  (strips "v" from tag)
    $ver  = $tag.TrimStart("v")
    $file = "AI-HUD_${ver}_${arch}-setup.exe"
    $url  = "$RELEASES/download/$tag/$file"
    return $url, $file
}

# ── Download with progress ─────────────────────────────────────────────────────
function Invoke-Download {
    param([string]$Url, [string]$Dest)
    Write-Step "Downloading from GitHub..."
    $wc = New-Object System.Net.WebClient
    $wc.Headers.Add("User-Agent", "AI-HUD-Installer/1.0")
    $wc.DownloadFile($Url, $Dest)
    Write-Ok "Downloaded $(Split-Path $Dest -Leaf) ($('{0:N1}' -f ((Get-Item $Dest).Length / 1MB)) MB)"
}

# ── Checksum verification ──────────────────────────────────────────────────────
function Test-Checksum {
    param([string]$File, [string]$SumUrl)
    try {
        $headers = @{ "User-Agent" = "AI-HUD-Installer/1.0" }
        $expected = (Invoke-RestMethod -Uri $SumUrl -Headers $headers -TimeoutSec 10).Trim().Split()[0]
        $actual   = (Get-FileHash -Path $File -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $expected.ToLower()) {
            throw "Checksum mismatch! Expected: $expected  Got: $actual"
        }
        Write-Ok "Checksum verified"
    } catch [System.Net.WebException] {
        Write-Warn "No checksum file found — skipping verification"
    }
}

# ── Run NSIS silent install ───────────────────────────────────────────────────
function Install-Nsis {
    param([string]$Installer, [string]$InstallDir)
    Write-Step "Running installer..."
    $args = @("/S", "/D=$InstallDir")
    $proc = Start-Process -FilePath $Installer -ArgumentList $args -Wait -PassThru
    if ($proc.ExitCode -ne 0) {
        throw "Installer exited with code $($proc.ExitCode)"
    }
    Write-Ok "Installed to $InstallDir"
}

# ── Portable extract (zip fallback) ──────────────────────────────────────────
function Install-Portable {
    param([string]$ZipFile, [string]$InstallDir)
    Write-Step "Extracting portable build..."
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Expand-Archive -Path $ZipFile -DestinationPath $InstallDir -Force
    Write-Ok "Extracted to $InstallDir"
}

# ── Startup registration (current-user Run key, no admin) ────────────────────
function Register-Startup {
    param([string]$ExePath)
    $key  = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
    Set-ItemProperty -Path $key -Name $APP_NAME -Value "`"$ExePath`"" -Type String
    Write-Ok "Startup registered (runs on login)"
}

function Remove-Startup {
    $key = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
    if (Get-ItemProperty -Path $key -Name $APP_NAME -ErrorAction SilentlyContinue) {
        Remove-ItemProperty -Path $key -Name $APP_NAME -Force
        Write-Ok "Startup entry removed"
    }
}

# ── Desktop shortcut ──────────────────────────────────────────────────────────
function New-Shortcut {
    param([string]$ExePath)
    $desktop = [Environment]::GetFolderPath("Desktop")
    $lnk     = Join-Path $desktop "$APP_NAME.lnk"
    $wsh     = New-Object -ComObject WScript.Shell
    $sc      = $wsh.CreateShortcut($lnk)
    $sc.TargetPath       = $ExePath
    $sc.WorkingDirectory = Split-Path $ExePath
    $sc.Description      = "AI-HUD Overlay"
    $sc.Save()
    Write-Ok "Desktop shortcut created"
}

# ── Version check (skip if same version already installed) ───────────────────
function Get-InstalledVersion {
    param([string]$InstallDir)
    $ver_file = Join-Path $InstallDir "version.txt"
    if (Test-Path $ver_file) { return (Get-Content $ver_file -Raw).Trim() }
    return $null
}

# ── Uninstall ─────────────────────────────────────────────────────────────────
function Uninstall-AiHud {
    Write-Banner
    Write-Step "Uninstalling $APP_NAME..."

    Remove-Startup

    $installDir = Get-InstallDir
    $uninstaller = Join-Path $installDir "Uninstall $APP_NAME.exe"
    if (Test-Path $uninstaller) {
        Write-Step "Running uninstaller..."
        Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait
    } elseif (Test-Path $installDir) {
        Write-Step "Removing install directory..."
        Remove-Item -Path $installDir -Recurse -Force
    } else {
        Write-Warn "$APP_NAME does not appear to be installed at $installDir"
    }

    # Remove desktop shortcut
    $lnk = Join-Path ([Environment]::GetFolderPath("Desktop")) "$APP_NAME.lnk"
    if (Test-Path $lnk) { Remove-Item $lnk -Force; Write-Ok "Shortcut removed" }

    Write-Host ""
    Write-Ok "$APP_NAME has been uninstalled."
    Write-Host ""
}

# ── Main install flow ─────────────────────────────────────────────────────────
function Install-AiHud {
    Write-Banner

    $tag     = Get-LatestVersion
    $ver     = $tag.TrimStart("v")
    $dir     = Get-InstallDir
    $tmpDir  = Join-Path $env:TEMP "ai-hud-install-$ver"
    $arch    = Get-Arch

    Write-Dim "Version   : $tag"
    Write-Dim "Platform  : Windows $arch"
    Write-Dim "Install   : $dir"
    Write-Host ""

    # ── Already installed? ────────────────────────────────────────────────────
    $installed = Get-InstalledVersion -InstallDir $dir
    if ($installed -eq $ver) {
        Write-Ok "$APP_NAME $ver is already up to date."
        Write-Host ""
        return
    }
    if ($installed) {
        Write-Step "Updating $APP_NAME $installed → $ver (settings preserved)"
    }

    # ── Stop running instance ─────────────────────────────────────────────────
    Get-Process -Name "AI-HUD" -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 500

    # ── Prepare temp dir ──────────────────────────────────────────────────────
    New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

    try {
        if ($Portable) {
            # Download zip for portable mode
            $verNodot = $ver.Replace(".", "-")
            $zipName  = "AI-HUD_${ver}_windows_${arch}.zip"
            $zipUrl   = "$RELEASES/download/$tag/$zipName"
            $zipPath  = Join-Path $tmpDir $zipName

            Invoke-Download -Url $zipUrl -Dest $zipPath
            Test-Checksum -File $zipPath -SumUrl "$zipUrl.sha256"
            Install-Portable -ZipFile $zipPath -InstallDir $dir
        } else {
            # Download NSIS installer
            $url, $file = Get-DownloadUrl -tag $tag
            $exePath    = Join-Path $tmpDir $file

            Invoke-Download -Url $url -Dest $exePath
            Test-Checksum -File $exePath -SumUrl "$url.sha256"
            Install-Nsis -Installer $exePath -InstallDir $dir
        }

        # ── Write version stamp ───────────────────────────────────────────────
        $ver | Set-Content -Path (Join-Path $dir "version.txt") -Encoding UTF8

        # ── Register startup ──────────────────────────────────────────────────
        $exe = Join-Path $dir $EXE_NAME
        if (-not $Portable -and (Test-Path $exe)) {
            Register-Startup -ExePath $exe
            if (-not $Silent) { New-Shortcut -ExePath $exe }
        }

        # ── Launch app ────────────────────────────────────────────────────────
        Write-Step "Launching $APP_NAME..."
        if (Test-Path $exe) { Start-Process -FilePath $exe }

        Write-Host ""
        Write-Ok "$APP_NAME $ver installed successfully!"
        Write-Host ""
        Write-Dim "  Uninstall : irm https://raw.githubusercontent.com/$REPO/main/scripts/install.ps1 | iex; Uninstall-AiHud"
        Write-Host ""

    } finally {
        Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Entry point ───────────────────────────────────────────────────────────────
if ($Uninstall) {
    Uninstall-AiHud
} else {
    Install-AiHud
}
