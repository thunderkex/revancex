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

> Sourced dynamically from [`config/modules.yaml`](config/modules.yaml).

<!-- AUTO-BUNDLE-BREAKDOWN-START -->
| Bundle | Zip File | Apps | Included Applications |
| :--- | :--- | :---: | :--- |
| **Core Essentials** | `revancex-bundle.zip` | 5 | YouTube, YouTube Music, Reddit, Spotify, X (Twitter) |
| **Multimedia** | `revancex-bundle-multimedia.zip` | 5 | YouTube, YouTube Music, Spotify, SoundCloud, Prime Video |
| **Social Media** | `revancex-bundle-social.zip` | 7 | Reddit, TikTok, Instagram, Facebook, Threads, X (Twitter), Pixiv |
| **Productivity** | `revancex-bundle-productivity.zip` | 3 | CamScanner, Lightroom, WPS Office |
| **Tools & Utilities** | `revancex-bundle-tools.zip` | 3 | AdGuard, Proton VPN, Solid Explorer |
<!-- AUTO-BUNDLE-BREAKDOWN-END -->

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

> Sourced dynamically from [`config/sources.yaml`](config/sources.yaml).

<!-- AUTO-PATCH-SOURCES-START -->
| ID | Repository | Asset Pattern |
| :--- | :--- | :--- |
| `anddea` | [anddea/revanced-patches](https://github.com/anddea/revanced-patches) | `*.mpp` |
| `de_revanced` | [RookieEnough/De-Vanced](https://github.com/RookieEnough/De-Vanced) | `*.mpp` |
| `piko` | [crimera/piko](https://github.com/crimera/piko) | `*.mpp` |
| `morphe_patches` | [MorpheApp/morphe-patches](https://github.com/MorpheApp/morphe-patches) | `*.mpp` |
| `icysymmetra` | [icysymmetra/tiktok-patches-for-morphe](https://github.com/icysymmetra/tiktok-patches-for-morphe) | `*.mpp` |
| `hoodles` | [hoo-dles/morphe-patches](https://github.com/hoo-dles/morphe-patches) | `*.mpp` |
| `rushiranpise` | [rushiranpise/morphe-patches](https://github.com/rushiranpise/morphe-patches) | `*.mpp` |
| `durgesh` | [durgesh0505/chiggi_morphe_patches](https://github.com/durgesh0505/chiggi_morphe_patches) | `*.mpp` |
| `arandomhooman` | [arandomhooman/hoomans-morphe-patches](https://github.com/arandomhooman/hoomans-morphe-patches) | `*.mpp` |
| `hxreborn` | [hxreborn/morphe-patches](https://github.com/hxreborn/morphe-patches) | `*.mpp` |
| `bholey` | [BholeyKaBhakt/android-patches-xtra](https://github.com/BholeyKaBhakt/android-patches-xtra) | `*.mpp` |
| `jkennethcarino` | [jkennethcarino/adobo](https://github.com/jkennethcarino/adobo) | `*.mpp` |
| `dhrubonai` | [dhrubonai/morphe-patches](https://github.com/dhrubonai/morphe-patches) | `*.mpp` |
| `sapitosucio` | [SapitoSucio/FroggoMorphePatches](https://github.com/SapitoSucio/FroggoMorphePatches) | `*.mpp` |
| `sysdmindoc` | [SysAdminDoc/hushfeed](https://github.com/SysAdminDoc/hushfeed) | `*.mpp` |
| `browzomje` | [browzomje/browzomje-patches](https://github.com/browzomje/browzomje-patches) | `*.mpp` |
| `blazeftl` | [BlazeFTL/FTL-Patches](https://github.com/BlazeFTL/FTL-Patches) | `*.mpp` |
| `rikydev` | [riky-dev/morphe-patches](https://github.com/riky-dev/morphe-patches) | `*.mpp` |
| `miguelninja` | [MiguelNinja19/miguel-morphe-patches](https://github.com/MiguelNinja19/miguel-morphe-patches) | `*.mpp` |
<!-- AUTO-PATCH-SOURCES-END -->

<!-- AUTO-APP-LIST-START -->

### 📱 Stock Apps & Single Root Modules

| App | Stock APK | Root APK | Single Root Module | Last Updated |
| :--- | :--- | :--- | :--- | :--- |
| **Adguard** | [adguard-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/adguard-patched.apk) | — | [revancex-module-adguard.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-adguard.zip) | 2026-09-17 05:01:45 UTC |
| **Camscanner** | [camscanner-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/camscanner-patched.apk) | — | [revancex-module-camscanner.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-camscanner.zip) | 2026-09-17 05:01:45 UTC |
| **Capcut** | [capcut-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/capcut-patched.apk) | — | [revancex-module-capcut.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-capcut.zip) | 2026-09-17 05:01:45 UTC |
| **Drama Box** | [drama_box-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/drama_box-patched.apk) | — | [revancex-module-drama_box.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-drama_box.zip) | 2026-09-17 05:01:45 UTC |
| **Duolingo** | [duolingo-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/duolingo-patched.apk) | — | [revancex-module-duolingo.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-duolingo.zip) | 2026-09-17 05:01:45 UTC |
| **Facebook** | [facebook-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/facebook-patched.apk) | — | [revancex-module-facebook.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-facebook.zip) | 2026-09-17 05:01:45 UTC |
| **Flightradar24** | [flightradar24-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/flightradar24-patched.apk) | — | [revancex-module-flightradar24.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-flightradar24.zip) | 2026-09-17 05:01:45 UTC |
| **Gboard** | [gboard-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/gboard-patched.apk) | — | [revancex-module-gboard.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-gboard.zip) | 2026-09-17 05:01:45 UTC |
| **Google News** | [google_news-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/google_news-patched.apk) | — | [revancex-module-google_news.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-google_news.zip) | 2026-09-17 05:01:45 UTC |
| **Google Photos** | [google_photos-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/google_photos-patched.apk) | — | [revancex-module-google_photos.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-google_photos.zip) | 2026-09-17 05:01:45 UTC |
| **Instagram** | [instagram-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/instagram-patched.apk) | — | [revancex-module-instagram.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-instagram.zip) | 2026-09-17 05:01:45 UTC |
| **Lightroom** | [lightroom-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/lightroom-patched.apk) | — | [revancex-module-lightroom.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-lightroom.zip) | 2026-09-17 05:01:45 UTC |
| **Microg** | [microg.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/microg.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Mx Player** | [mx_player-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/mx_player-patched.apk) | — | [revancex-module-mx_player.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-mx_player.zip) | 2026-09-17 05:01:45 UTC |
| **Myfitnesspal** | [myfitnesspal-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/myfitnesspal-patched.apk) | — | [revancex-module-myfitnesspal.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-myfitnesspal.zip) | 2026-09-17 05:01:45 UTC |
| **Nova Launcher** | [nova_launcher-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/nova_launcher-patched.apk) | — | [revancex-module-nova_launcher.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-nova_launcher.zip) | 2026-09-17 05:01:45 UTC |
| **Pixiv** | [pixiv-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/pixiv-patched.apk) | — | [revancex-module-pixiv.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-pixiv.zip) | 2026-09-17 05:01:45 UTC |
| **Prime Video** | [prime_video-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/prime_video-patched.apk) | — | [revancex-module-prime_video.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-prime_video.zip) | 2026-09-17 05:01:45 UTC |
| **Proton Vpn** | [proton_vpn-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/proton_vpn-patched.apk) | — | [revancex-module-proton_vpn.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-proton_vpn.zip) | 2026-09-17 05:01:45 UTC |
| **Pvz Free** | [pvz_free-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/pvz_free-patched.apk) | — | [revancex-module-pvz_free.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-pvz_free.zip) | 2026-09-17 05:01:45 UTC |
| **Reddit** | [reddit-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/reddit-patched.apk) | — | [revancex-module-reddit.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-reddit.zip) | 2026-09-17 05:01:45 UTC |
| **Serverauditor** | [serverauditor-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/serverauditor-patched.apk) | — | [revancex-module-serverauditor.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-serverauditor.zip) | 2026-09-17 05:01:45 UTC |
| **Solid Explorer** | [solid_explorer-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/solid_explorer-patched.apk) | — | [revancex-module-solid_explorer.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-solid_explorer.zip) | 2026-09-17 05:01:45 UTC |
| **Soundcloud** | [soundcloud-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/soundcloud-patched.apk) | [soundcloud-root.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/soundcloud-root.apk) | [revancex-module-soundcloud.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/revancex-module-soundcloud.zip) | 2026-09-17 05:01:45 UTC |
| **Spotify** | [spotify-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/spotify-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Stellarium** | [stellarium-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/stellarium-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Sticker Maker** | [sticker_maker-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/sticker_maker-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Strava** | [strava-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/strava-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Telegram** | [telegram-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/telegram-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Threads** | [threads-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/threads-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Tiktok** | [tiktok-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/tiktok-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Wps Office** | [wps_office-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/wps_office-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **X Piko** | [x_piko-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/x_piko-patched.apk) | — | — | 2026-09-17 05:01:45 UTC |
| **Youtube** | [youtube-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/youtube-patched.apk) | [youtube-root.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/youtube-root.apk) | — | 2026-09-17 05:01:45 UTC |
| **Youtube Music** | [youtube_music-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/youtube_music-patched.apk) | [youtube_music-root.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.4-2026.09.17-b22/youtube_music-root.apk) | — | 2026-09-17 05:01:45 UTC |



### 📦 Bundle Modules (Multi-App)

| Bundle Name | Included Apps & Payload Files | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- | :--- |
| _None yet_ | — | — | — | — |



### 🛠️ Custom Bundle Modules

| Custom Bundle | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- |
| _None yet_ | — | — | — |

<!-- AUTO-APP-LIST-END -->
