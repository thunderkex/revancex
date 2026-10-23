<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css">

<p align="center">
  <img src="assets/logo.png" alt="ReVanceX" width="600" style="background: #ffffff; border-radius: 16px; padding: 16px;">
</p>

# ReVanceX

Automated patch orchestration ecosystem written in Rust. It streamlines patching Android applications across multiple upstream patch sources, generating standalone signed APKs, individual Magisk/KernelSU modules, and pre-categorized multi-app bundles with an embedded management WebUI.

## Features

- **35+ Pre-Configured Apps**: YouTube, YouTube Music, Spotify, Reddit, MicroG, TikTok, Instagram, Twitter/X, Pinterest, Gboard, Google Photos, and more.
- **Fast Rust CLI (`revancex`)**: High-performance local building, version resolution, and APK patching with no runtime dependencies beyond Java JDK 17 (Rust 1.85+ MSRV).
- **Multi-Source APK Acquisition**: Resilient fallback pipeline (APKMirror, Uptodown, APKPure API, Aptoide, APKCombo, APKFab, Archive.org, GitHub, GitLab, Codeberg, apkeep) protected by domain-aware circuit breakers.
- **Root & Non-Root Packaging**: Generate standalone signed APKs, individual Magisk/KernelSU modules, or multi-app bundle zips.
- **Embedded WebUI**: KernelSU, APatch, and Magisk modules include an interactive, responsive Material 3 Expressive WebUI for inspecting and toggling mounted apps.
- **Testing & Health Diagnostics**: Built-in environment diagnostic suite (`doctor`), static DEX and AndroidManifest validation, and automated smoke testing (`test`) with failure threshold policies.
- **Dynamic Configuration & CI/CD**: Decoupled YAML definitions (`apps.yaml`, `sources.yaml`, `modules.yaml`, `build.yaml`), structured build reporting (`BUILD_REPORT.json`), automated GitHub Actions pipelines, and an interactive GitHub Pages catalogue.

---

## <i class="fa-solid fa-mobile-screen"></i> Supported Applications & Releases

Browse apps visually on our [Web Catalogue](https://thunderkex.github.io/revancex/) or download latest builds below. App configurations are defined in [`config/apps.yaml`](config/apps.yaml).

<!-- AUTO-APP-LIST-START -->

### <i class="fa-solid fa-mobile-screen"></i> Patched Apps & Single Root Modules

| App | Patched APK | Single Root Module | Last Updated |
| :--- | :--- | :--- | :--- |
| **Adguard** | [adguard-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/adguard-patched.apk) | — | 2026-09-28 19:13:02 UTC |
| **Camscanner** | [camscanner-patched.apk](#camscanner-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Capcut** | [capcut-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/capcut-patched.apk) | — | 2026-10-05 01:57:41 UTC |
| **Cloudflare Warp** | [cloudflare_warp-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/cloudflare_warp-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Drama Box** | [drama_box-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/drama_box-patched.apk) | — | 2026-09-28 08:40:43 UTC |
| **Duolingo** | [duolingo-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/duolingo-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Facebook** | [facebook-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b128/facebook-patched.apk) | — | 2026-10-19 08:07:38 UTC |
| **Flightradar24** | [flightradar24-patched.apk](#flightradar24-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Gboard** | [gboard-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.11-b116/gboard-patched.apk) | — | 2026-10-11 02:05:10 UTC |
| **Google Maps** | [google_maps-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.12-b117/google_maps-patched.apk) | — | 2026-10-12 02:29:17 UTC |
| **Google News** | [google_news-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/google_news-patched.apk) | — | 2026-09-28 19:13:02 UTC |
| **Google Phone** | [google_phone-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.12-b117/google_phone-patched.apk) | — | 2026-10-12 02:29:17 UTC |
| **Google Photos** | [google_photos-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/google_photos-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Hungry Shark World** | [hungry_shark_world-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/hungry_shark_world-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Instagram** | [instagram-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.11-b116/instagram-patched.apk) | — | 2026-10-11 02:05:10 UTC |
| **Lightroom** | [lightroom-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/lightroom-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Microg** | [microg.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/microg.apk) | — | 2026-10-19 09:51:05 UTC |
| **Mx Player** | [mx_player-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/mx_player-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Myfitnesspal** | [myfitnesspal-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/myfitnesspal-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Nova Launcher** | [nova_launcher-patched.apk](#nova_launcher-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Pinterest** | [pinterest-patched.apk](#pinterest-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Pixiv** | [pixiv-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/pixiv-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Prime Video** | [prime_video-patched.apk](#prime_video-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Proton Vpn** | [proton_vpn-patched.apk](#proton_vpn-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Pvz Free** | [pvz_free-patched.apk](#pvz_free-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Reddit** | [reddit-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.11-b116/reddit-patched.apk) | — | 2026-10-11 02:05:10 UTC |
| **Serverauditor** | [serverauditor-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.17-b124/serverauditor-patched.apk) | — | 2026-10-17 09:32:45 UTC |
| **Solid Explorer** | [solid_explorer-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.27-b28/solid_explorer-patched.apk) | — | 2026-09-27 09:56:46 UTC |
| **Soundcloud** | [soundcloud-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/soundcloud-patched.apk) | — | 2026-10-05 01:57:41 UTC |
| **Spotify** | [spotify-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/spotify-patched.apk) | — | 2026-10-05 01:57:41 UTC |
| **Stellarium** | [stellarium-patched.apk](#stellarium-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Sticker Maker** | [sticker_maker-patched.apk](#sticker_maker-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Strava** | [strava-patched.apk](#strava-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Subway Surfers** | [subway_surfers-patched.apk](#subway_surfers-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Telegram** | [telegram-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/telegram-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Threads** | [threads-patched.apk](#threads-patched.apk) | — | 2026-10-22 02:06:32 UTC |
| **Tiktok** | [tiktok-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/tiktok-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **Wps Office** | [wps_office-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/wps_office-patched.apk) | — | 2026-10-19 09:51:05 UTC |
| **X Piko** | [x_piko-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.11-b116/x_piko-patched.apk) | — | 2026-10-11 02:05:10 UTC |
| **Youtube** | [youtube-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/youtube-patched.apk) | [revancex-module-youtube.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/revancex-module-youtube.zip) | 2026-09-28 19:13:02 UTC |
| **Youtube Music** | [youtube_music-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/youtube_music-patched.apk) | [revancex-module-youtube_music.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/revancex-module-youtube_music.zip) | 2026-10-19 09:51:05 UTC |



### <i class="fa-solid fa-box-archive"></i> Bundle Modules (Multi-App)

| Bundle Name | Included Apps & Payload Files | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- | :--- |
| **All Bundle**<br><code>revancex-bundle-all.zip</code> | Configured apps | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b128/revancex-bundle-all.zip) | `auto-v1.0.5-2026.10.19-b128` | 2026-10-19 08:07:38 UTC |
| **Core Bundle**<br><code>revancex-bundle-core.zip</code> | Configured apps | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/revancex-bundle-core.zip) | `auto-v1.0.5-2026.10.19-b130` | 2026-10-19 09:51:05 UTC |
| **Multimedia Bundle**<br><code>revancex-bundle-multimedia.zip</code> | Configured apps | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/revancex-bundle-multimedia.zip) | `auto-v1.0.5-2026.10.19-b130` | 2026-10-19 09:51:05 UTC |
| **Productivity Bundle**<br><code>revancex-bundle-productivity.zip</code> | Configured apps | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/revancex-bundle-productivity.zip) | `auto-v1.0.5-2026.10.19-b130` | 2026-10-19 09:51:05 UTC |
| **Social Bundle**<br><code>revancex-bundle-social.zip</code> | Configured apps | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.19-b130/revancex-bundle-social.zip) | `auto-v1.0.5-2026.10.19-b130` | 2026-10-19 09:51:05 UTC |
| **Tools Bundle**<br><code>revancex-bundle-tools.zip</code> | Configured apps | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.12-b117/revancex-bundle-tools.zip) | `auto-v1.0.5-2026.10.12-b117` | 2026-10-12 02:29:17 UTC |



### <i class="fa-solid fa-screwdriver-wrench"></i> Custom Bundle Modules

| Custom Bundle | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- |
| _None yet_ | — | — | — |

<!-- AUTO-APP-LIST-END -->

---

## <i class="fa-solid fa-book"></i> Guides & Reference

<details>
<summary><b><i class="fa-solid fa-download"></i> Download & Installation (Prebuilt CLI)</b></summary>

You do not need to install Rust or compile anything to build your patched APKs or root modules. Simply download the precompiled binary for your system.

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

</details>

<details>
<summary><b><i class="fa-solid fa-rocket"></i> Step-by-Step Build Workflow</b></summary>

#### Step 0: Check Environment & Prerequisites

Verify that your system has all required and recommended tools installed:

```bash
./revancex doctor
```

*(On Windows: `.\revancex.exe doctor`)*

This verifies Java JDK 17+, Android SDK tools (`keytool`, `zipalign`, `adb`), `apkeep`, python runtime, bypass services, and configuration integrity.

#### Step 1: Download Required Tools & Patches

Download the required patcher JARs, integrations, and tools automatically:

```bash
./revancex download-tools
```

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

- **Build all configured applications with failure tolerance and report generation**:

  ```bash
  ./revancex build --apps all --arch arm64-v8a --keep-going --report ./output/BUILD_REPORT.json
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

#### Step 4: Smoke Test Built APKs

Validate built APKs before release or installation:

```bash
# Run static manifest/DEX verification or adb smoke test
./revancex test --apps all --fail-threshold 10 --continue
```

- When devices are connected via ADB, it tests APK installation, launch, and checks logcat for crash loops or fatal exceptions.
- In offline or CI environments without physical devices, it performs deep static inspection (valid ZIP structure, valid `classes.dex`, readable `AndroidManifest.xml`).

#### Step 5: Create Magisk & KernelSU Modules (Root)

Package patched APKs into flashable root modules:

- **Single Root Module (one zip per app)**:

  ```bash
  ./revancex module --single --apps youtube --output ./modules
  ```

- **Multi-App Categorized Bundle Modules**:
  Build pre-categorized bundles (or custom app combinations) with embedded Material 3 WebUI:

  ```bash
  ./revancex module --bundle --apps social --output ./modules        # Social Media Bundle
  ./revancex module --bundle --apps multimedia --output ./modules    # Multimedia Bundle
  ./revancex module --bundle --apps productivity --output ./modules  # Productivity Bundle
  ./revancex module --bundle --apps tools --output ./modules         # Tools & Utilities Bundle
  ./revancex module --bundle --apps core --output ./modules          # Core Essentials Bundle
  ./revancex module --bundle --apps all --include-webui --output ./modules  # All Category Bundles
  ```

- **Verify Root Module Integrity**:

  ```bash
  ./revancex verify-module ./modules/revancex-bundle-core.zip
  ```

#### Step 6: Validate Configuration & Setup

Verify configuration syntax, app schemas, and patch integrity:

```bash
./revancex validate --strict
```

Or perform live reachability checks on upstream sources:

```bash
./revancex validate --online
```

</details>

<details>
<summary><b><i class="fa-solid fa-boxes-stacked"></i> Module Bundle Architecture & Specifications</b></summary>

#### Bundle File Structure & Contents

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

#### Categorized Bundles Breakdown

> Sourced dynamically from [`config/modules.yaml`](config/modules.yaml).

<!-- AUTO-BUNDLE-BREAKDOWN-START -->
| Bundle | Zip File | Apps | Included Applications |
| :--- | :--- | :---: | :--- |
| **All** | `revancex-bundle-all.zip` | 24 | Adguard, Camscanner, Duolingo, Facebook, Flightradar24, Gboard, Google Photos, Hungry Shark World, Instagram, Lightroom, Moviebox, Myfitnesspal, Nova Launcher, Pixiv, Prime Video, Proton Vpn, Reddit, Strava, Threads, Tiktok, Wps Office, X Piko, Youtube, Youtube Music |
| **Core Essentials** | `revancex-bundle.zip` | 4 | Youtube, Youtube Music, Reddit, X Piko |
| **Multimedia** | `revancex-bundle-multimedia.zip` | 3 | Youtube, Youtube Music, Prime Video |
| **Productivity** | `revancex-bundle-productivity.zip` | 3 | Camscanner, Lightroom, Wps Office |
| **Social** | `revancex-bundle-social.zip` | 7 | Reddit, Tiktok, Instagram, Facebook, Threads, X Piko, Pixiv |
| **Tools** | `revancex-bundle-tools.zip` | 2 | Adguard, Proton Vpn |
<!-- AUTO-BUNDLE-BREAKDOWN-END -->

</details>

<details>
<summary><b><i class="fa-solid fa-terminal"></i> CLI Subcommand Reference</b></summary>

| Command | Description | Example |
| --- | --- | --- |
| `doctor` | Inspect local environment, required JDK 17+, Android SDK tools, and network services | `revancex doctor` |
| `apps` | List all configured apps, status, and package IDs | `revancex apps [--json]` |
| `download-tools` | Download patcher JARs, integrations, and tools | `revancex download-tools [--apps youtube]` |
| `check-updates` | Poll upstream sources for app and patch updates | `revancex check-updates [--json]` |
| `build` | Patch, optimize, sign APKs, and generate build reports | `revancex build --apps youtube --arch arm64-v8a [--keep-going] [--report <path>]` |
| `module` | Package APKs into single or bundle Magisk/KernelSU modules | `revancex module --bundle --apps core [--include-webui]` |
| `test` | Smoke test APKs (static DEX/manifest or ADB device) with failure tolerance | `revancex test --apps all [--fail-threshold 20] [--continue]` |
| `validate` | Check schema, app definitions, and patch compatibility | `revancex validate [--strict] [--online] [--apps youtube]` |
| `clean` | Clean build outputs, caches, or stale tool downloads | `revancex clean [--all] [--stale-tools]` |
| `fetch-apk` | Download stock APK using the configured multi-source chain | `revancex fetch-apk youtube --arch arm64-v8a` |
| `verify-module` | Verify structure and integrity of a root module zip | `revancex verify-module ./modules/revancex-bundle-core.zip` |
| `patches` | List available patches and version compatibilities for an app | `revancex patches youtube [--mode root\|nonroot] [--json]` |
| `keystore` | Inspect or initialize signing keystore | `revancex keystore --info` |
| `report` | Inspect build report and enforce failure threshold policies in CI | `revancex report check ./output/BUILD_REPORT.json [--fail-threshold 15]` |
| `sources` | Probe and test APK sources for reachability and version resolution | `revancex sources probe youtube` |
| `gen-apps-json` | Generate web catalogue dataset `docs/apps.json` from release registry | `revancex gen-apps-json` |
| `generate-matrix` | Generate CI build matrix JSON for GitHub Actions | `revancex generate-matrix --apps all` |

</details>

<details>
<summary><b><i class="fa-solid fa-hammer"></i> Build from Source (Developers)</b></summary>

Compile the CLI directly from source:

```bash
# Prerequisites: Rust 1.85+ and JDK 17+
cargo build --release

# Run compiled binary
./target/release/revancex doctor
./target/release/revancex apps
./target/release/revancex download-tools
./target/release/revancex build --apps youtube,microg --arch arm64-v8a
```

</details>

<details>
<summary><b><i class="fa-brands fa-github"></i> GitHub Actions (Automated Cloud Builds)</b></summary>

1. Fork or push to this repository.
2. The GitHub Actions workflows run automatically:
   - **Auto Build**: Triggered on schedule or via `workflow_dispatch`.
   - **Custom Build**: Triggered via the GitHub Pages interface or `workflow_dispatch`.
   - **CI Validation & Tests**: Runs automated Rust tests and script verifications on every push.
3. Download the generated APKs and Magisk modules directly from the **Releases** tab.

</details>

<details>
<summary><b><i class="fa-solid fa-sliders"></i> Environment Variables</b></summary>

Copy `.env.example` to `.env` and configure credentials. Never commit `.env`.

| Variable | Description | Default / Example |
| --- | --- | --- |
| `GITHUB_TOKEN` | Token with `repo` + `workflow` scopes (prevents GitHub API rate limits) | `ghp_...` |
| `GITLAB_TOKEN` | (Optional) GitLab token for fetching GitLab-hosted releases without rate limits | `glpat-...` |
| `KEYSTORE_PASSWORD` | Keystore password for signing generated APKs | `revanced` / `666` |
| `KEYSTORE_FILE` | (Optional) Custom keystore path override | `keys/custom.jks` |
| `KEYSTORE_B64` | (Optional) Base64-encoded keystore for CI pipelines | `<base64>` |
| `TG_TOKEN` | (Optional) Telegram bot token for build notifications | `123456:ABC...` |
| `TG_CHAT` | (Optional) Telegram chat ID or channel username | `-1001234567890` |
| `TG_TOPIC` | (Optional) Telegram forum topic / thread ID for supergroups | `42` |
| `FLARESOLVERR_URL` | (Optional) FlareSolverr endpoint URL | `http://localhost:8191/v1` |
| `TRAWL_URL` | (Optional) Trawl headless browser render service endpoint URL | `http://localhost:8000` |
| `REVANCEX_STATIC_TEST` | (Optional) Force static manifest/DEX testing without ADB device | `1` |
| `REVANCEX_CB_THRESHOLD` | (Optional) Circuit breaker error threshold before tripping | `3` |
| `REVANCEX_CB_COOLDOWN_SECS` | (Optional) Circuit breaker cooldown period in seconds | `60` |
| `REVANCEX_CONFIG_DIR` | (Optional) Override path to config directory | `./config` |
| `REVANCEX_TEMP_DIR` | (Optional) Override path to temporary directory | `./tmp` |

</details>

<details>
<summary><b><i class="fa-solid fa-circle-plus"></i> Adding a New App</b></summary>

1. Add an entry to [`config/apps.yaml`](config/apps.yaml) following the schema documented in [`CONFIG.md`](CONFIG.md).
2. Query available patches and compatible versions:

   ```bash
   ./revancex patches <app_id>
   ```

3. Validate configuration syntax and patch integrity:

   ```bash
   ./revancex validate --strict --apps <app_id>
   ```

4. Build and test locally:

   ```bash
   ./revancex build --apps <app_id>
   ./revancex test --apps <app_id>
   ```

</details>

<details>
<summary><b><i class="fa-solid fa-code-branch"></i> Upstream Patch Sources</b></summary>

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
| `browzomje` | [browzomje/browzomje-patches](https://github.com/browzomje/browzomje-patches) | `*.mpp` |
| `blazeftl` | [BlazeFTL/FTL-Patches](https://github.com/BlazeFTL/FTL-Patches) | `*.mpp` |
| `rikydev` | [riky-dev/morphe-patches](https://github.com/riky-dev/morphe-patches) | `*.mpp` |
| `miguelninja` | [MiguelNinja19/miguel-morphe-patches](https://github.com/MiguelNinja19/miguel-morphe-patches) | `*.mpp` |
| `hushfeed` | [SysAdminDoc/HushFeed](https://github.com/SysAdminDoc/HushFeed) | `*.mpp` |
| `hushfacebook` | [SysAdminDoc/Hushfacebook](https://github.com/SysAdminDoc/Hushfacebook) | `*.mpp` |
| `hushthreads` | [SysAdminDoc/HushThreads](https://github.com/SysAdminDoc/HushThreads) | `*.mpp` |
| `DmoniakPatches` | [SatanMerde/D-moniakPatches](https://github.com/SatanMerde/D-moniakPatches) | `*.mpp` |
| `bearinmindcat` | [bearinmindcat/morphe-patches](https://github.com/bearinmindcat/morphe-patches) | `*.mpp` |
| `jasonwu` | [jasonwu1994/Gboard-patches](https://github.com/jasonwu1994/Gboard-patches) | `*.mpp` |
<!-- AUTO-PATCH-SOURCES-END -->

</details>

