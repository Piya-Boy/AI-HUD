#!/usr/bin/env bash
# AI-HUD one-command installer for Linux and macOS
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.sh | bash -s -- --portable
#   curl -fsSL https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.sh | bash -s -- --uninstall

set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────────
REPO="Piya-Boy/AI-HUD"
APP_NAME="AI-HUD"
API_BASE="https://api.github.com/repos/${REPO}"
RELEASES="https://github.com/${REPO}/releases"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/main"

# ── Flags ─────────────────────────────────────────────────────────────────────
PORTABLE=0
SILENT=0
UNINSTALL=0
PINNED_VERSION=""

for arg in "$@"; do
    case "$arg" in
        --portable)  PORTABLE=1 ;;
        --silent)    SILENT=1   ;;
        --uninstall) UNINSTALL=1 ;;
        --version=*) PINNED_VERSION="${arg#--version=}" ;;
    esac
done

# ── Colors ─────────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    CYAN='\033[0;36m' GREEN='\033[0;32m' YELLOW='\033[1;33m'
    RED='\033[0;31m'  DIM='\033[2m'      MAGENTA='\033[0;35m' NC='\033[0m'
else
    CYAN='' GREEN='' YELLOW='' RED='' DIM='' MAGENTA='' NC=''
fi

step()  { echo -e "  ${CYAN}$*${NC}"; }
ok()    { echo -e "  ${GREEN}$*${NC}"; }
warn()  { echo -e "  ${YELLOW}$*${NC}"; }
fail()  { echo -e "  ${RED}$*${NC}" >&2; exit 1; }
dim()   { echo -e "  ${DIM}$*${NC}"; }

banner() {
    echo ""
    echo -e "  ${MAGENTA}AI-HUD Installer${NC}"
    echo -e "  ${DIM}─────────────────────────────────${NC}"
    echo ""
}

# ── Platform detection ─────────────────────────────────────────────────────────
detect_os() {
    case "$(uname -s)" in
        Darwin*) echo "macos" ;;
        Linux*)  echo "linux" ;;
        *)       fail "Unsupported OS: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x64"   ;;
        aarch64|arm64) echo "arm64" ;;
        *) fail "Unsupported architecture: $(uname -m)" ;;
    esac
}

# ── Install directory ──────────────────────────────────────────────────────────
get_install_dir() {
    local os="$1"
    if [ "$PORTABLE" -eq 1 ]; then
        echo "$(pwd)/ai-hud"
        return
    fi
    case "$os" in
        macos) echo "$HOME/Applications" ;;
        linux) echo "$HOME/.local/share/ai-hud" ;;
    esac
}

# ── Bin directory (for symlink / PATH) ────────────────────────────────────────
get_bin_dir() {
    if [ "$PORTABLE" -eq 1 ]; then echo "$(pwd)/ai-hud/bin"; return; fi
    if [ -d "$HOME/.local/bin" ]; then echo "$HOME/.local/bin"; return; fi
    echo "$HOME/.local/bin"
}

# ── Downloader (curl preferred, wget fallback) ────────────────────────────────
download() {
    local url="$1" dest="$2"
    if command -v curl &>/dev/null; then
        curl -fsSL --retry 3 --retry-delay 2 -o "$dest" "$url"
    elif command -v wget &>/dev/null; then
        wget -q --tries=3 --waitretry=2 -O "$dest" "$url"
    else
        fail "Neither curl nor wget found. Install one and retry."
    fi
}

download_stdout() {
    local url="$1"
    if command -v curl &>/dev/null; then
        curl -fsSL --retry 3 "$url"
    else
        wget -q -O- "$url"
    fi
}

# ── Latest version from GitHub API ────────────────────────────────────────────
get_latest_version() {
    if [ -n "$PINNED_VERSION" ]; then echo "$PINNED_VERSION"; return; fi
    download_stdout "$API_BASE/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"\(.*\)".*/\1/'
}

# ── Resolve artifact filename ──────────────────────────────────────────────────
get_artifact() {
    local os="$1" arch="$2" ver="$3"
    # tauri-action naming:
    #   macOS:  AI-HUD_0.1.0_aarch64.dmg  or  AI-HUD_0.1.0_x64.dmg
    #   Linux:  AI-HUD_0.1.0_amd64.AppImage  or  AI-HUD_0.1.0_arm64.AppImage
    local tauri_arch
    case "$arch" in
        x64)   tauri_arch="amd64" ;;
        arm64) tauri_arch="aarch64" ;;
    esac
    case "$os" in
        macos) echo "AI-HUD_${ver}_${tauri_arch}.dmg" ;;
        linux) echo "AI-HUD_${ver}_${tauri_arch}.AppImage" ;;
    esac
}

# ── Checksum verification ──────────────────────────────────────────────────────
verify_checksum() {
    local file="$1" url="$2"
    local sum_url="${url}.sha256"
    local expected
    expected=$(download_stdout "$sum_url" 2>/dev/null | awk '{print $1}') || true
    if [ -z "$expected" ]; then
        warn "No checksum file found — skipping verification"
        return
    fi
    if command -v sha256sum &>/dev/null; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum &>/dev/null; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    else
        warn "No sha256 tool found — skipping verification"
        return
    fi
    if [ "$actual" != "$expected" ]; then
        fail "Checksum mismatch!\n  Expected: $expected\n  Got:      $actual"
    fi
    ok "Checksum verified"
}

# ── macOS DMG install ──────────────────────────────────────────────────────────
install_dmg() {
    local dmg="$1" install_dir="$2"
    step "Mounting disk image..."
    local mount_point
    mount_point=$(hdiutil attach "$dmg" -nobrowse -quiet | awk 'END{print $NF}')
    mkdir -p "$install_dir"
    local app_src
    app_src=$(find "$mount_point" -maxdepth 1 -name "*.app" | head -1)
    if [ -z "$app_src" ]; then fail "No .app bundle found in DMG"; fi
    local app_name
    app_name=$(basename "$app_src")
    rm -rf "$install_dir/$app_name"
    cp -R "$app_src" "$install_dir/"
    hdiutil detach "$mount_point" -quiet
    ok "Installed ${app_name} to ${install_dir}"
    echo "$install_dir/$app_name"
}

# ── Linux AppImage install ─────────────────────────────────────────────────────
install_appimage() {
    local appimage="$1" install_dir="$2"
    mkdir -p "$install_dir"
    local dest="$install_dir/AI-HUD.AppImage"
    cp "$appimage" "$dest"
    chmod +x "$dest"
    ok "Installed AppImage to $dest"
    echo "$dest"
}

# ── PATH setup ────────────────────────────────────────────────────────────────
ensure_path() {
    local bin_dir="$1"
    mkdir -p "$bin_dir"
    local shell_rc=""
    if [ -f "$HOME/.zshrc" ]; then shell_rc="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then shell_rc="$HOME/.bashrc"
    elif [ -f "$HOME/.bash_profile" ]; then shell_rc="$HOME/.bash_profile"; fi

    if [[ ":$PATH:" != *":$bin_dir:"* ]]; then
        if [ -n "$shell_rc" ]; then
            echo "" >> "$shell_rc"
            echo "# AI-HUD" >> "$shell_rc"
            echo "export PATH=\"\$PATH:$bin_dir\"" >> "$shell_rc"
            dim "Added $bin_dir to PATH in $shell_rc (restart terminal to take effect)"
        fi
    fi
}

# ── macOS Login Item (launchd plist) ──────────────────────────────────────────
register_startup_macos() {
    local app_path="$1"
    local plist="$HOME/Library/LaunchAgents/com.dev.ai-hud.plist"
    local exec_path="$app_path/Contents/MacOS/AI-HUD"
    cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>             <string>com.dev.ai-hud</string>
    <key>ProgramArguments</key>  <array><string>$exec_path</string></array>
    <key>RunAtLoad</key>         <true/>
    <key>KeepAlive</key>         <false/>
    <key>StandardErrorPath</key> <string>/tmp/ai-hud.err</string>
    <key>StandardOutPath</key>   <string>/tmp/ai-hud.out</string>
</dict>
</plist>
PLIST
    launchctl load "$plist" 2>/dev/null || true
    ok "Login item registered (launchd)"
}

remove_startup_macos() {
    local plist="$HOME/Library/LaunchAgents/com.dev.ai-hud.plist"
    if [ -f "$plist" ]; then
        launchctl unload "$plist" 2>/dev/null || true
        rm -f "$plist"
        ok "Login item removed"
    fi
}

# ── Linux autostart (XDG desktop entry) ───────────────────────────────────────
register_startup_linux() {
    local appimage="$1"
    local autostart_dir="$HOME/.config/autostart"
    mkdir -p "$autostart_dir"
    cat > "$autostart_dir/ai-hud.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=AI-HUD
Exec=$appimage
Icon=ai-hud
Comment=AI usage overlay
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
DESKTOP
    ok "Autostart registered (~/.config/autostart/ai-hud.desktop)"

    # Also install .desktop into applications menu
    local apps_dir="$HOME/.local/share/applications"
    mkdir -p "$apps_dir"
    cp "$autostart_dir/ai-hud.desktop" "$apps_dir/ai-hud.desktop"
    command -v update-desktop-database &>/dev/null \
        && update-desktop-database "$apps_dir" 2>/dev/null || true
}

remove_startup_linux() {
    rm -f "$HOME/.config/autostart/ai-hud.desktop"
    rm -f "$HOME/.local/share/applications/ai-hud.desktop"
    ok "Autostart entries removed"
}

# ── Installed version stamp ────────────────────────────────────────────────────
get_installed_version() {
    local stamp="$1/version.txt"
    [ -f "$stamp" ] && cat "$stamp" || echo ""
}

write_version_stamp() {
    local dir="$1" ver="$2"
    echo "$ver" > "$dir/version.txt"
}

# ── Uninstall ─────────────────────────────────────────────────────────────────
do_uninstall() {
    local os
    os=$(detect_os)
    banner
    step "Uninstalling $APP_NAME..."

    local install_dir
    install_dir=$(get_install_dir "$os")

    case "$os" in
        macos)
            remove_startup_macos
            rm -rf "$install_dir/AI-HUD.app"
            ;;
        linux)
            remove_startup_linux
            rm -rf "$install_dir"
            rm -f "$(get_bin_dir)/ai-hud"
            ;;
    esac

    echo ""
    ok "$APP_NAME has been uninstalled."
    echo ""
}

# ── Main install flow ──────────────────────────────────────────────────────────
do_install() {
    local os arch tag ver install_dir artifact_name artifact_url tmp_dir

    os=$(detect_os)
    arch=$(detect_arch)
    banner

    step "Fetching latest version..."
    tag=$(get_latest_version)
    [ -z "$tag" ] && fail "Could not determine latest version from GitHub"
    ver="${tag#v}"

    install_dir=$(get_install_dir "$os")
    artifact_name=$(get_artifact "$os" "$arch" "$ver")
    artifact_url="${RELEASES}/download/${tag}/${artifact_name}"

    dim "Version   : $tag"
    dim "Platform  : $os / $arch"
    dim "Artifact  : $artifact_name"
    dim "Install   : $install_dir"
    echo ""

    # ── Already installed? ──────────────────────────────────────────────────
    local installed_ver
    installed_ver=$(get_installed_version "$install_dir")
    if [ "$installed_ver" = "$ver" ]; then
        ok "$APP_NAME $ver is already up to date."
        echo ""
        return
    fi
    [ -n "$installed_ver" ] && step "Updating $APP_NAME $installed_ver → $ver (settings preserved)"

    # ── Kill running instance ───────────────────────────────────────────────
    pkill -x "AI-HUD" 2>/dev/null || true
    sleep 0.3

    # ── Download ────────────────────────────────────────────────────────────
    tmp_dir=$(mktemp -d)
    trap "rm -rf '$tmp_dir'" EXIT

    local tmp_file="$tmp_dir/$artifact_name"
    step "Downloading $artifact_name..."
    download "$artifact_url" "$tmp_file"
    local size
    size=$(du -sh "$tmp_file" | cut -f1)
    ok "Downloaded $artifact_name ($size)"

    verify_checksum "$tmp_file" "$artifact_url"

    # ── Install ─────────────────────────────────────────────────────────────
    local installed_path=""
    case "$os" in
        macos)
            installed_path=$(install_dmg "$tmp_file" "$install_dir")
            if [ "$PORTABLE" -eq 0 ]; then
                register_startup_macos "$installed_path"
                write_version_stamp "$HOME/.local/share/ai-hud" "$ver"
            fi
            ;;
        linux)
            installed_path=$(install_appimage "$tmp_file" "$install_dir")
            # Symlink into bin
            local bin_dir
            bin_dir=$(get_bin_dir)
            ensure_path "$bin_dir"
            ln -sf "$installed_path" "$bin_dir/ai-hud"
            write_version_stamp "$install_dir" "$ver"
            if [ "$PORTABLE" -eq 0 ]; then
                register_startup_linux "$installed_path"
            fi
            ;;
    esac

    # ── Launch ──────────────────────────────────────────────────────────────
    step "Launching $APP_NAME..."
    case "$os" in
        macos)
            open "$installed_path" 2>/dev/null || true
            ;;
        linux)
            nohup "$installed_path" >/dev/null 2>&1 &
            ;;
    esac

    echo ""
    ok "$APP_NAME $ver installed successfully!"
    echo ""
    dim "  Uninstall:"
    dim "  curl -fsSL ${RAW_BASE}/scripts/install.sh | bash -s -- --uninstall"
    echo ""
}

# ── Entry ─────────────────────────────────────────────────────────────────────
if [ "$UNINSTALL" -eq 1 ]; then
    do_uninstall
else
    do_install
fi
