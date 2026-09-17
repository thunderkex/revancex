<p align="center">
  <img src="assets/logo.png" alt="ReVanceX" width="600" style="background: #ffffff; border-radius: 16px; padding: 16px;">
</p>

# ReVanceX

Modern, high-performance Rust patcher and orchestration ecosystem for ReVanced, Morphe, Magisk, and KernelSU modules with dynamic configurations, prebuilt binaries, and WebUI support.

## Features

- **40+ Pre-Configured Apps**: YouTube, YouTube Music, Spotify, Reddit, MicroG, Soundcloud, Twitter/X, and more.
- **Fast Rust CLI (`revancex`)**: High-performance local building, version resolution, and APK patching with no runtime dependencies beyond Java JDK 17.
- **Root & Non-Root Packaging**: Generate standalone signed APKs, individual Magisk/KernelSU modules, or multi-app bundle zips.
- **Embedded WebUI**: KernelSU and Magisk modules include an interactive, responsive WebUI for managing mounted apps.
- **Dynamic Configuration**: Fully decoupled definitions for apps (`apps.yaml`), patch profiles (`patches.yaml`), and upstream sources (`sources.yaml`).
- **Automated CI/CD**: Scheduled and on-demand builds via GitHub Actions with GitHub Pages catalogue.

---

## Guide: Building with Prebuilt CLI Binary

You do not need to install Rust or compile anything to build your patched APKs or root modules. Simply download the precompiled binary for your system.

### 1. Download & Installation

Download the latest precompiled CLI binary from [GitHub Releases](https://github.com/thunderkex/revancex/releases):

- **Linux (x86_64)**: [revancex-linux-x86_64.tar.gz](https://github.com/thunderkex/revancex/releases/latest/download/revancex-linux-x86_64.tar.gz)
- **Windows (x86_64)**: [revancex-windows-x86_64.zip](https://github.com/thunderkex/revancex/releases/latest/download/revancex-windows-x86_64.zip)

**Prerequisites**: Java JDK 17 or higher installed on your system.

#### Linux Setup

```bash
# Extract archive and mark as executable
tar -xzf revancex-linux-x86_64.tar.gz
chmod +x revancex
```

#### Windows Setup (PowerShell)

```powershell
# Extract archive
Expand-Archive revancex-windows-x86_64.zip -DestinationPath .
```

---

### 2. Discovering Supported Apps

Before building, you can view the complete catalogue of supported applications, package names, and their default patch sources:

- **List all supported apps**:

  ```bash
  ./revancex apps
  ```

  *(Outputs each app's internal ID, enabled status, and Android package name)*

- **Export app definitions as JSON**:

  ```bash
  ./revancex apps --json
  ```

- **Configuration File**:
  App definitions are stored in [`config/apps.yaml`](config/apps.yaml). You can customize patch sources (`revanced`, `revanced_extended`, `morphe`), target versions, or enable/disable apps directly in this file.

- **Web Catalogue**:
  Browse apps visually with the interactive web interface hosted on GitHub Pages or locally via `docs/index.html`.

---

### 3. Step-by-Step Build Workflow

#### Step 1: Download Required Tools & Patches

Download the required tools (revanced-cli, patch jars, apkeep, integrations) automatically:

```bash
./revancex download-tools
```

*(On Windows: `.\revancex.exe download-tools`)*

#### Step 2: Check for Upstream Updates (Optional)

Check if newer versions of apps or patch bundles are available upstream:

```bash
./revancex check-updates
```

#### Step 3: Build Patched APKs

Build your selected applications using the `build` subcommand:

- **Build specific applications**:

  ```bash
  ./revancex build --apps youtube,microg --arch arm64-v8a
  ```

- **Build all configured applications**:

  ```bash
  ./revancex build --apps all --arch arm64-v8a
  ```

- **Supported architectures (`--arch`)**:
  - `arm64-v8a` (Recommended for modern 64-bit Android devices)
  - `armeabi-v7a` (32-bit legacy devices)
  - `x86_64` (Emulators and Chromebooks)
  - `all` (Universal builds)

- **Optimization modes (`--mode`)**:
  - `--mode full`: Keeps all resources and languages (default).
  - `--mode lite`: Strips foreign architectures and unused language assets for minimal APK size.

- **Specify custom output folder**:

  ```bash
  ./revancex build --apps youtube --output ./my-output
  ```

#### Step 4: Create Magisk & KernelSU Modules (Root)

Package patched APKs into flashable root modules:

- **Single Root Module (one zip per app)**:

  ```bash
  ./revancex module --single --apps youtube --output ./modules
  ```

- **Multi-App Categorized Bundle Modules**:
  Build pre-categorized bundles (or custom app combinations) to keep module sizes lightweight and organized:

  ```bash
  # Build specific category bundles
  ./revancex module --bundle --apps social --output ./modules        # Social Media Bundle
  ./revancex module --bundle --apps multimedia --output ./modules    # Multimedia Bundle
  ./revancex module --bundle --apps productivity --output ./modules  # Productivity Bundle
  ./revancex module --bundle --apps tools --output ./modules         # Tools & Utilities Bundle
  ./revancex module --bundle --apps core --output ./modules          # Core Essentials Bundle
  ./revancex module --bundle --apps all --include-webui --output ./modules  # All Category Bundles
  ```

##### Bundle File Structure & Contents

Each bundle is a flashable Magisk, KernelSU, or APatch module zip structured as follows:

```text
revancex-bundle-<category>.zip
├── META-INF/com/google/android/
│   ├── update-binary            # Root flash installer script
│   └── updater-script           # #MAGISK header
├── module.prop                  # Module metadata (id, name, version, author)
├── customize.sh                 # Environment setup and installation routine
├── utils.sh                     # Dynamic bind-mount helper functions
├── service.sh                   # Boot service for active APK bind mounts
├── action.sh                    # Action button script for root managers
├── uninstall.sh                 # Clean unmount & file cleanup on module removal
├── apps.list                    # App package & mount mode mapping
├── webui/                       # Embedded management WebUI (if --include-webui is used)
└── apks/                        # Patched APK payloads mounted on top of stock base APKs
    ├── <app_1>.apk
    └── <app_2>.apk
```

##### Categorized Bundles Breakdown

| Bundle Name | Module Zip File | Apps Count | Payload Files Inside (`apks/`) | Included Applications |
| :--- | :--- | :--- | :--- | :--- |
| **Core Essentials Bundle** | `revancex-bundle.zip` | 5 Apps | • `apks/youtube.apk`<br>• `apks/youtube_music.apk`<br>• `apks/reddit.apk`<br>• `apks/spotify.apk`<br>• `apks/x_piko.apk` | YouTube, YouTube Music, Reddit, Spotify, X (Twitter) |
| **Multimedia Bundle** | `revancex-bundle-multimedia.zip` | 7 Apps | • `apks/youtube.apk`<br>• `apks/youtube_music.apk`<br>• `apks/youtube_morphed.apk`<br>• `apks/youtube_music_morphed.apk`<br>• `apks/spotify.apk`<br>• `apks/soundcloud.apk`<br>• `apks/prime_video.apk` | YouTube, YouTube Music, YouTube Morphed, YouTube Music Morphed, Spotify, SoundCloud, Prime Video |
| **Social Media Bundle** | `revancex-bundle-social.zip` | 7 Apps | • `apks/reddit.apk`<br>• `apks/tiktok.apk`<br>• `apks/instagram.apk`<br>• `apks/facebook.apk`<br>• `apks/threads.apk`<br>• `apks/x_piko.apk`<br>• `apks/pixiv.apk` | Reddit, TikTok, Instagram, Facebook, Threads, X (Twitter), Pixiv |
| **Productivity Bundle** | `revancex-bundle-productivity.zip` | 11 Apps | • `apks/wps_office.apk`<br>• `apks/camscanner.apk`<br>• `apks/solid_explorer.apk`<br>• `apks/fx_file_explorer.apk`<br>• `apks/rar.apk`<br>• `apks/google_photos.apk`<br>• `apks/lightroom.apk`<br>• `apks/google_recorder.apk`<br>• `apks/google_news.apk`<br>• `apks/terabox.apk`<br>• `apks/photomath.apk` | WPS Office, CamScanner, Solid Explorer, FX File Explorer, RAR, Google Photos, Lightroom, Google Recorder, Google News, TeraBox, Photomath |
| **Tools & Utilities Bundle** | `revancex-bundle-tools.zip` | 14 Apps | • `apks/adguard.apk`<br>• `apks/brave_browser.apk`<br>• `apks/proton_vpn.apk`<br>• `apks/psiphon.apk`<br>• `apks/battery_guru.apk`<br>• `apks/smart_launcher.apk`<br>• `apks/nova_launcher.apk`<br>• `apks/tasker.apk`<br>• `apks/waze.apk`<br>• `apks/truecaller.apk`<br>• `apks/eyecon_caller.apk`<br>• `apks/zalo.apk`<br>• `apks/strava.apk`<br>• `apks/myfitnesspal.apk` | AdGuard, Brave Browser, Proton VPN, Psiphon, Battery Guru, Smart Launcher, Nova Launcher, Tasker, Waze, Truecaller, Eyecon, Zalo, Strava, MyFitnessPal |

#### Step 5: Validate Configuration & Setup

Verify configuration syntax, app schemas, and patch integrity:

```bash
./revancex validate --strict
```

---

### 4. CLI Subcommand Reference

| Command | Description | Example |
| --- | --- | --- |
| `apps` | List all configured apps and package IDs | `revancex apps` |
| `download-tools` | Download patcher JARs, integrations, and tools | `revancex download-tools` |
| `check-updates` | Poll upstream sources for app and patch updates | `revancex check-updates --json` |
| `build` | Patch and sign target applications | `revancex build --apps youtube --arch arm64-v8a` |
| `module` | Package APKs into Magisk/KernelSU zip modules | `revancex module --bundle --apps youtube,microg` |
| `validate` | Check schema and config integrity | `revancex validate --strict` |
| `clean` | Clean up build artifacts and temporary files | `revancex clean` |

---

### 5. Build from Source (Developers)

If you prefer to compile the CLI from source:

```bash
# Prerequisites: Rust 1.75+ and JDK 17+
cargo build --release

# Run compiled binary
./target/release/revancex apps
./target/release/revancex download-tools
./target/release/revancex build --apps youtube,microg --arch arm64-v8a
```

### 3. GitHub Actions (Automated Cloud Builds)

1. Fork or push to this repository.
2. The GitHub Actions workflows run automatically:
   - **Auto Build**: Triggered on schedule or via `workflow_dispatch`.
   - **Custom Build**: Triggered via the GitHub Pages interface or `workflow_dispatch`.
   - **CI Validation & Tests**: Runs automated tests on every push.
3. Download the generated APKs and Magisk modules directly from the **Releases** tab.

### Environment Variables

Copy `.env.example` to `.env` and fill in values. Never commit `.env`.

| Variable | Description |
| --- | --- |
| `GITHUB_TOKEN` | Token with `repo` + `workflow` scopes |
| `KEYSTORE_PASSWORD` | Password for generated keystores |
| `TG_TOKEN` | (Optional) Telegram bot token for build notifications |
| `TG_CHAT` | (Optional) Telegram chat ID or channel username |
| `TG_TOPIC` | (Optional) Telegram topic / thread ID for forum supergroups |

## Adding an App

Add an entry to `config/apps.yaml` following the existing pattern. Run `validate` to check.

## Patch Sources

| ID | Repo |
| --- | --- |
| `revanced` | ReVanced/revanced-patches |
| `revanced_extended` | inotia00/revanced-patches |
| `morphe` | crimera/piko |

<!-- AUTO-APP-LIST-START -->

### 📱 Stock Apps & Single Root Modules

| App | Stock APK | Root APK | Single Root Module | Last Updated |
| :--- | :--- | :--- | :--- | :--- |
| **Microg** | [microg.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b20/microg.apk) | — | — | 2026-09-17 02:54:37 UTC |

### 📦 Bundle Modules (Multi-App)

| Bundle Name | Included Apps & Payload Files | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- | :--- |
| *None yet* | — | — | — | — |

### 🛠️ Custom Bundle Modules

| Custom Bundle | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- |
| *None yet* | — | — | — |

<!-- AUTO-APP-LIST-END -->
