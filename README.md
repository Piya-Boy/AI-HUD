# AI-HUD

Transparent overlay that shows Claude Code token usage and cost on top of your terminal — automatically, per-tab, in real time.

---

## Install

### Windows

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.ps1 | iex"
```

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.sh | bash
```

One command → downloads latest release → installs → registers startup → launches.

---

## Installer options

| Flag | Effect |
|------|--------|
| `--portable` | Install into current folder only, no system changes |
| `--silent` | Skip interactive prompts |
| `--uninstall` | Remove the app and all startup entries |
| `--version=vX.Y.Z` | Pin a specific release (Unix) |

**Windows examples**
```powershell
# Portable
irm https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.ps1 | iex; Install-AiHud -Portable

# Uninstall
irm https://raw.githubusercontent.com/Piya-Boy/AI-HUD/main/scripts/install.ps1 | iex; Uninstall-AiHud
```

**Unix examples**
```bash
# Portable
curl -fsSL .../install.sh | bash -s -- --portable

# Uninstall
curl -fsSL .../install.sh | bash -s -- --uninstall

# Pin version
curl -fsSL .../install.sh | bash -s -- --version=v0.1.0
```

---

## What it installs

| Platform | Location | Startup |
|----------|----------|---------|
| Windows | `%LOCALAPPDATA%\AI-HUD` | Registry `HKCU\…\Run` |
| macOS | `~/Applications` | `~/Library/LaunchAgents` plist |
| Linux | `~/.local/share/ai-hud` | `~/.config/autostart` desktop entry |

Settings (`settings.json`) are preserved across updates.

---

## Updating

Re-run the install command. The installer detects the current version and skips if already up to date. The app also checks for updates in the background every 4 hours and shows a compact toast in the overlay when a new release is available.

---

## Building from source

```bash
git clone https://github.com/Piya-Boy/AI-HUD.git
cd AI-HUD
npm install
npm run tauri dev
```

Requires: Node 22, Rust stable, platform WebKit libraries (Linux only — see CI workflow for apt packages).

---

## Releasing

```bash
git tag v0.2.0
git push origin v0.2.0
```

GitHub Actions builds Windows / macOS (x64 + arm64) / Linux, generates SHA-256 checksums, and publishes the release automatically.

Required repository secrets:

| Secret | Purpose |
|--------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | Updater signature key (generate with `npx tauri signer generate`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the above |
| `APPLE_*` (optional) | macOS notarization |

---

## License

MIT
