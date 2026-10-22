<img src="design/icon.png" width="128" height="128" alt="Evorar app icon" />

# evorar

**English | [简体中文](README.md)**

A quiet, fast, and beautiful archiver for Windows. **Zero ads, zero background processes, zero bundling — fully open source.**

![Tauri](https://img.shields.io/badge/Tauri-2.x-blue) ![Rust](https://img.shields.io/badge/Rust-stable-orange) ![License](https://img.shields.io/badge/License-MIT-green)

## Why

| Competitor | Problem |
|------------|---------|
| 360 Zip | Cloud antivirus, promotion-config fetching, background updater task every 3 hours, bundled downloads |
| WinRAR | Paid, trial nag dialog, dated UI |
| Bandizip Free | Desktop popup ads since 7.37 |

evorar's promise: **zero network requests, zero scheduled tasks, zero resident processes beyond explicit user actions. Closing the window exits the process.**

## Features

- **Formats**: create ZIP / 7Z; extract ZIP / 7Z / RAR (incl. RAR5) / TAR / GZ / BZ2 / XZ
- **Smart extract**: a single top-level folder extracts in place (no nesting); multiple entries go into a folder named after the archive
- **Mojibake fix**: auto-detects GBK/UTF-8 filenames inside ZIP archives (a long-standing pain for Chinese users)
- **Safety**: zip-slip path traversal blocked — illegal paths are rejected
- **Lightweight**: single portable EXE around 4 MB, ~35 MB RAM at startup (less than WinRAR / 360 Zip)
- Modern UI: Fluent style, light/dark theme follows the system, drag & drop, cancellable progress

> RAR is extract-only: the RAR compression format is RarLab's proprietary property; third-party tools may not create RAR archives.

## Download

Grab from [Releases](https://github.com/josebergo/evorar/releases):

- `evorar.exe` — portable single-file build, no installation required
- `evorar_x64-setup.exe` — NSIS installer (Start Menu / desktop shortcuts, uninstallable)

Requires Windows 10 (2004+) / Windows 11 (WebView2 runtime is built in).

## Build

```bash
pnpm install          # frontend deps
pnpm tauri dev        # dev mode with hot reload
pnpm tauri build      # release build
cd src-tauri && cargo test   # engine tests (incl. GBK mojibake e2e case)
```

Requires: Rust stable (MSVC), Node 20+, pnpm.

## Architecture

```
code/
├── src/                  # Frontend (WebView2 layer, zero-framework)
│   ├── main.ts           # state machine: empty / browse / compress
│   └── style.css         # Fluent style + light/dark themes
└── src-tauri/
    ├── src/lib.rs        # Tauri commands, task queue, progress events
    └── src/engine/       # archiving engines (fully decoupled from UI)
        ├── mod.rs        # unified trait + zip-slip protection
        ├── detect.rs     # magic-byte sniffing + GBK/UTF-8 filename fix
        ├── zipfmt.rs     # ZIP engine
        ├── sevenz.rs     # 7Z engine (LZMA2)
        ├── tarball.rs    # TAR/GZ/BZ2/XZ
        ├── rar.rs        # RAR extraction (unrar)
        └── smart.rs      # smart-extract strategy
```

## Roadmap

- [x] MVP: extract/compress, smart extract, GBK mojibake fix, cancellable progress
- [ ] P1: Explorer context-menu extension, file associations, in-archive preview, AES-256, multi-volume
- [ ] P2: self-extracting archives (SFX), recovery records, UI localization

## License

[MIT](LICENSE)

RAR extraction is powered by the [unrar](https://www.rarlab.com/rar_add.htm) sources under its license (free for decompression use); all other dependencies are open source.
