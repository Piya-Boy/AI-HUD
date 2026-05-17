# Changelog

All notable changes to AI-HUD are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

## [0.1.0] - 2026-05-18

### Added
- Initial release
- Per-terminal overlay: one transparent HUD per terminal HWND
- Windows Terminal tab detection via IUIAutomation (correct tab matching)
- Overlay z-order management: non-topmost, owned by terminal window
- Hard-hide via Win32 ShowWindow when terminal loses focus
- Occlusion detection: WindowFromPoint 4-point sampling
- 30 Hz anchor loop for real-time overlay repositioning
- Claude Code session tracking with 500 ms stability filter
- Session/daily/weekly token + cost display
- Auto-updater: background check every 4 h, emits update toast
- One-command installer for Windows, macOS, Linux
- GitHub Actions release pipeline (Windows/macOS x64+arm64/Linux)
