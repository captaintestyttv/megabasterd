# MegaBasterd (Rust/Tauri Rewrite)

A high-performance rewrite of [MegaBasterd](https://github.com/tonikelope/megabasterd) in Rust + Tauri, targeting faster downloads, lower memory usage, and no JVM dependency. Downloads only — no uploads or streaming.

## Features

- Parallel chunk downloading with AES-CTR decryption
- Smart proxy rotation (SOCKS5 + HTTP)
- Resume interrupted downloads
- Clipboard link detection
- MEGA account support (v1 and v2 auth, including 2FA)
- MegaCrypter link support
- SQLite-backed persistence (settings, downloads, accounts)
- Svelte 5 frontend with real-time progress

---

## Prerequisites

All platforms require:

- **Rust** (stable, 1.77+) — install via [rustup](https://rustup.rs)
- **Node.js** (18+) — for the frontend build
- **Tauri CLI v2** — installed via Cargo (see below)

Install the Tauri CLI once after installing Rust:

```sh
cargo install tauri-cli --version "^2"
```

---

## Platform-Specific Setup

### Linux

Install system dependencies (required by Tauri for WebView and bundling):

**Debian / Ubuntu:**
```sh
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libxdo-dev \
  libglib2.0-dev
```

**Fedora / RHEL:**
```sh
sudo dnf install -y \
  webkit2gtk4.1-devel \
  gtk3-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  openssl-devel \
  libxdo-devel \
  gcc \
  curl \
  wget \
  file
```

**Arch Linux:**
```sh
sudo pacman -S --needed \
  webkit2gtk-4.1 \
  gtk3 \
  libappindicator-gtk3 \
  librsvg \
  openssl \
  xdotool \
  base-devel \
  curl \
  wget
```

> **Clipboard support** (`arboard`) requires either X11 (`xclip` or `xdotool`) or Wayland (`wl-clipboard`). Install whichever matches your display server:
> ```sh
> # X11
> sudo apt install xclip   # Debian/Ubuntu
> # Wayland
> sudo apt install wl-clipboard
> ```

---

### macOS

Install Xcode Command Line Tools (if not already present):

```sh
xcode-select --install
```

No additional system libraries are required. macOS ships with WebKit, which Tauri uses directly.

> Apple Silicon (M1/M2/M3) is fully supported. The Rust toolchain installed via `rustup` defaults to the native `aarch64-apple-darwin` target.

---

### Windows

Install the following:

1. **Microsoft C++ Build Tools** — download from [visualstudio.microsoft.com](https://visualstudio.microsoft.com/visual-cpp-build-tools/). During setup select the **"Desktop development with C++"** workload.

2. **WebView2 Runtime** — pre-installed on Windows 10 (1803+) and Windows 11. If missing, download from [Microsoft's WebView2 page](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

> All commands below work in **PowerShell** or **Command Prompt**. If using Git Bash, the commands are identical.

---

## Running in Development Mode

Development mode starts a hot-reloading Vite dev server for the frontend and compiles the Rust backend in debug mode.

```sh
# 1. Clone and enter the repository
git clone https://github.com/captaintestyttv/megabasterd.git
cd megabasterd/megabasterd-rs

# 2. Install frontend dependencies
cd crates/megabasterd-app/frontend
npm install
cd ../../..

# 3. Start the app
cargo tauri dev
```

The app window will open automatically. The frontend hot-reloads on changes to `.svelte`/`.ts` files. Rust changes trigger a recompile and restart.

---

## Building a Release Binary

This produces an optimized native binary and installer for the current platform.

```sh
cd megabasterd/megabasterd-rs

# Install frontend dependencies (if not already done)
cd crates/megabasterd-app/frontend && npm install && cd ../../..

# Build
cargo tauri build
```

Output locations after a successful build:

| Platform | Artifact | Location |
|----------|----------|----------|
| Linux | `.deb` package | `target/release/bundle/deb/` |
| Linux | `.rpm` package | `target/release/bundle/rpm/` |
| Linux | AppImage | `target/release/bundle/appimage/` |
| macOS | `.dmg` disk image | `target/release/bundle/dmg/` |
| macOS | `.app` bundle | `target/release/bundle/macos/` |
| Windows | `.msi` installer | `target/release/bundle/msi/` |
| Windows | `.exe` NSIS installer | `target/release/bundle/nsis/` |

Install the package for your platform or run the binary directly from `target/release/megabasterd`.

---

## Running Tests

Unit tests cover crypto operations, chunk math, database CRUD, link parsing, and proxy logic:

```sh
cd megabasterd/megabasterd-rs
cargo test --package megabasterd-core
```

Expected output: **36 tests, 0 failures**.

---

## Project Structure

```
megabasterd-rs/
  Cargo.toml                        # Workspace root
  crates/
    megabasterd-core/               # All business logic (library crate)
      src/
        util/         # Encoding, base64, regex helpers
        crypto/       # AES-CBC/ECB/CTR, RSA, PBKDF2, MEGA key ops
        db/           # SQLite schema and queries
        config/       # Constants and settings
        mega_api/     # MEGA REST API client (auth, download URLs)
        megacrypter/  # MegaCrypter link resolution
        link_parser/  # URL detection for all MEGA formats
        proxy/        # Smart proxy manager
        download/     # Chunk downloader, writer, orchestrator, throttle
        transfer_manager/  # Queue scheduling and concurrency
        clipboard/    # Clipboard polling (250ms)
    megabasterd-app/                # Tauri desktop application
      src/
        main.rs       # Tauri builder, startup, event loops
        state.rs      # Shared application state (Arc-wrapped)
        commands/     # IPC command handlers
      frontend/       # Svelte 5 + Vite + Tailwind CSS
        src/
          App.svelte
          lib/api/    # Typed Tauri invoke/event wrappers
          lib/stores/ # Reactive download state
          lib/components/  # DownloadItem, LinkGrabber, StatusBar
```

---

## Troubleshooting

**`cargo tauri dev` fails with "cannot find webkit2gtk"**
Install the WebKit development libraries for your Linux distribution (see Platform-Specific Setup above).

**Frontend not loading / blank window**
Make sure `npm install` was run inside `crates/megabasterd-app/frontend/` before starting the app.

**Clipboard monitoring not detecting links on Linux**
Install `xclip` (X11) or `wl-clipboard` (Wayland) and ensure the display server environment variables (`DISPLAY` or `WAYLAND_DISPLAY`) are set.

**Windows: linker errors during `cargo build`**
Ensure the Microsoft C++ Build Tools are installed and that `link.exe` is on your `PATH`. Running from a **Developer Command Prompt for VS** resolves most linker issues.

**App data location**
MegaBasterd stores its SQLite database and settings in the platform default app data directory:
- Linux: `~/.local/share/megabasterd/`
- macOS: `~/Library/Application Support/com.megabasterd.app/`
- Windows: `%APPDATA%\com.megabasterd.app\`
