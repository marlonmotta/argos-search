<div align="center">

# 👁️ Argos Search

**Fast local file search for Windows & macOS**

[![CI](https://github.com/marlonmotta/argos-search/actions/workflows/ci.yml/badge.svg)](https://github.com/marlonmotta/argos-search/actions/workflows/ci.yml)
[![Release](https://github.com/marlonmotta/argos-search/actions/workflows/release.yml/badge.svg)](https://github.com/marlonmotta/argos-search/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-100%25-orange.svg)](https://www.rust-lang.org/)

*Sub-25ms search across your entire filesystem — powered by Tantivy, built with Tauri v2.*

</div>

---

## ✨ Features

| Feature | Description |
|---------|-------------|
| ⚡ **Blazing Fast** | Sub-25ms search powered by [Tantivy](https://github.com/quickwit-oss/tantivy) full-text engine |
| 📁 **Incremental Indexing** | Only re-indexes changed files (mtime + size + xxh3 hash) |
| 🖥️ **Native Desktop App** | Built with [Tauri v2](https://v2.tauri.app/) — lightweight ~16 MB |
| 💻 **CLI with JSON Output** | Integrate with any tool or pipeline |
| 🔍 **Accent-Insensitive** | Search "mae" to find "mãe", "cafe" to find "café" |
| 🏠 **4 Search Scopes** | Personal → Extended → Full → System |
| ⌨️ **Global Shortcut** | `Ctrl+Space` from anywhere — customizable |
| 🗄️ **System Tray** | Close-to-tray, minimize-to-tray, quick access |
| 📄 **40+ File Types** | Text, code, config, scripts, web files |
| 🧵 **Parallel Processing** | Uses all CPU cores via [Rayon](https://github.com/rayon-rs/rayon) |
| ⚙️ **Configurable** | `config.toml` for extensions, excludes, limits |

## 🏗️ Architecture

```
argos-search/
├── crates/
│   ├── core/          ← Search engine (Tantivy + SQLite, zero GUI deps)
│   │   ├── config.rs      Scope presets, excludes, auto-detect
│   │   ├── engine.rs      Tantivy indexing + custom ASCII folding tokenizer
│   │   ├── extractors.rs  File content extraction + hashing
│   │   └── metadata.rs    SQLite metadata store (WAL mode)
│   ├── cli/           ← Command-line interface (clap, --json)
│   ├── tauri-app/     ← Desktop backend (tray, shortcut, IPC)
│   └── frontend/      ← Web UI (vanilla JS + premium dark theme)
├── config.toml        ← User configuration
├── Cargo.toml         ← Workspace
└── .github/           ← CI + Release automation
```

## 🚀 Quick Start

### From Releases (Recommended)

Download the latest installer from [Releases](https://github.com/marlonmotta/argos-search/releases).

### From Source

```powershell
# Prerequisites: Rust 1.70+, Node.js (for Tauri CLI)
cargo install tauri-cli --version "^2"

# Clone and build
git clone https://github.com/marlonmotta/argos-search.git
cd argos-search

# Run in development mode
cargo tauri dev -c crates/tauri-app/tauri.conf.json

# Or build release binary
cargo tauri build -c crates/tauri-app/tauri.conf.json
```

### CLI Usage

```powershell
# Build CLI
cargo build --release -p argos-cli

# Index a folder
cargo run -p argos-cli -- index build --root "C:\Users\You\Documents"

# Search via CLI
cargo run -p argos-cli -- search "my query" --root "C:\Users\You\Documents" --json
```

## 🔎 Search Scopes

| Scope | Icon | Coverage |
|-------|------|----------|
| **Personal** | 🏠 | Projects, Desktop, Downloads |
| **Extended** | 💿 | + Documents, other drives/SSDs |
| **Full** | 📦 | + Program Files, installed software |
| **System** | 🖥️ | Everything including Windows/macOS system |

Each scope automatically excludes 30+ junk directories (AppData, node_modules, .git, caches, etc.).

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Space` | Toggle app visibility (global, customizable) |
| `Ctrl+F` | Focus search box |
| `Escape` | Clear search / close settings |
| `Double-click` result | Open file in default app |
| `Click` result | Copy full path to clipboard |
| `Right-click` result | Context menu (Open / Open folder / Copy) |

The global shortcut can be customized in Settings (⚙️) → Quick Launch Shortcut → Record.

## ⚙️ Configuration

Edit `config.toml` to customize:

```toml
# Folders to skip during indexing
excludes = ["node_modules", ".git", "target", "AppData"]

# File extensions to index with full content extraction
include_extensions = ["txt", "md", "rs", "py", "js", "ts", "json", "toml"]

# Max file size for text extraction (default: 2 MB)
max_file_size_bytes = 2_097_152

# CPU cores to use (0 = all available)
threads = 0
```

## 🔧 Tech Stack

| Component | Technology |
|-----------|-----------|
| Search Engine | [Tantivy](https://github.com/quickwit-oss/tantivy) 0.22 |
| Metadata Store | [SQLite](https://www.sqlite.org/) via rusqlite (bundled, WAL) |
| Desktop Framework | [Tauri](https://v2.tauri.app/) v2 |
| Parallelism | [Rayon](https://github.com/rayon-rs/rayon) |
| File Hashing | [xxHash](https://github.com/Cyan4973/xxHash) (xxh3_64) |
| Accent Folding | Unicode NFD normalization |
| CLI | [clap](https://github.com/clap-rs/clap) v4 |

## 📋 Roadmap

### ✅ v0.4 — Current
- Full-text search engine with accent-insensitive tokenizer
- Tauri v2 desktop app with premium dark theme
- System tray integration (close-to-tray, minimize-to-tray)
- Global shortcut (`Ctrl+Space`, customizable)
- 4 search scope presets with smart excludes
- Context menu, file actions, settings panel
- Search history persistence

### ⏳ v0.5 — Launcher Mode
- Minimal floating search bar (Spotlight/Alfred style)
- Transparent window, no borders, center screen
- 5-10 results with infinite scroll

### 🔮 v1.0 — Production Ready
- Signed `.exe` installer + `.dmg`
- Auto-start with system
- Content search with snippets
- Advanced filters (extension, size, date)
- Filesystem watcher for incremental updates

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes and commit: `git commit -m "feat: add my feature"`
4. Push to your fork: `git push origin feature/my-feature`
5. Open a Pull Request against `develop`

### Commit Convention

Use [Conventional Commits](https://www.conventionalcommits.org/):
- `feat:` — New features
- `fix:` — Bug fixes
- `docs:` — Documentation
- `ci:` — CI/CD changes
- `refactor:` — Code refactoring
- `test:` — Tests

## 📜 License

[MIT](LICENSE) — free for everyone.

---

<div align="center">

Built with 🦀 by **[Lord Hell](https://github.com/marlonmotta)**

</div>
