<p align="center">
  <img src="assets/logo.png" alt="ReVanceX" width="600" style="background: #ffffff; border-radius: 16px; padding: 16px;">
</p>

# ReVanceX

Automated patch orchestration ecosystem written in Rust. It streamlines patching Android applications across multiple upstream patch sources, generating standalone signed APKs, individual Magisk/KernelSU modules, and pre-categorized multi-app bundles with an embedded management WebUI.

## Features

- **31 Pre-Configured Apps**: YouTube, YouTube Music, Reddit, MicroG, TikTok, Instagram, Facebook, Threads, X Piko, Pinterest, Gboard, Google Maps, Proton VPN, AdGuard, Lightroom, CapCut, Termius, and more.
- **Fast Rust CLI (`revancex`)**: High-performance local building, version resolution, and APK patching with no runtime dependencies beyond Java JDK 17+ (JDK 21 in CI; Rust 1.85+ MSRV).
- **Multi-Source APK Acquisition & Resilience**: Resilient fallback pipeline (GitHub, GitLab, Archive.org, Codeberg, apkeep, APKPure API/WinUDF, APKMirror, Uptodown, Aptoide, APKFab, APKCombo) with per-app `source_priority`, `strict_sources`, negative caching, per-app timeout budgets, and domain-aware circuit breakers.
- **Preflight & Integrity Verification**: Preflight patch-compatibility and `pin_policy` resolution (`prefer_supported`, `strict`, `force`), minimum patch count checks (`expected_min_patches`), and post-download verification of ZIP/DEX/Manifest structure, package ID, version, file SHA-256, and optional signing certificate SHA-256.
- **Root & Non-Root Packaging**: Generate standalone signed APKs, individual Magisk/KernelSU/APatch modules, or multi-app category bundle zips.
- **Embedded WebUI**: KernelSU, APatch, and Magisk modules include an interactive, responsive Material 3 Expressive WebUI for inspecting and toggling mounted apps.
- **Testing & Health Diagnostics**: Built-in environment diagnostic suite (`doctor`), static DEX and AndroidManifest validation, and automated smoke testing (`test`) with failure threshold policies.
- **Dynamic Configuration & Reporting**: Decoupled YAML definitions (`apps.yaml`, `sources.yaml`, `modules.yaml`, `build.yaml`, `module.yaml`), structured build reporting (`BUILD_REPORT.json` schema v2.1.0 + `BUILD_REPORT.md`), compact per-app log files (`build/logs/`), automated GitHub Actions pipelines, and an interactive GitHub Pages catalogue.

---

## Supported Applications & Releases

Browse apps visually on our [Web Catalogue](https://thunderkex.github.io/revancex/) or download latest builds below. App configurations are defined in [`config/apps.yaml`](config/apps.yaml).

<!-- AUTO-APP-LIST-START -->

### Patched Apps & Single Root Modules

| App | Patched APK | Single Root Module | Last Updated |
| :--- | :--- | :--- | :--- |
| **AdGuard** | [AdGuard-4.14.68.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/AdGuard-4.14.68.apk) | - | 2026-11-11T09:29:12Z |
| **CamScanner** | [CamScanner-7.24.5.2608200000.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/CamScanner-7.24.5.2608200000.apk) | - | 2026-11-11T09:29:12Z |
| **CapCut** | [CapCut-19.9.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/CapCut-19.9.0.apk) | - | 2026-11-11T09:29:12Z |
| **Cloudflare WARP** | [Cloudflare-WARP-6.38.11.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Cloudflare-WARP-6.38.11.apk) | - | 2026-11-11T09:29:12Z |
| **DramaBox** | [drama_box-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/drama_box-patched.apk) | - | 2026-09-28 08:40:43 UTC |
| **Duolingo** | [Duolingo-6.98.4.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Duolingo-6.98.4.apk) | - | 2026-11-11T09:29:12Z |
| **Facebook** | [Facebook-586.0.0.52.53.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/Facebook-586.0.0.52.53.apk) | - | 2026-11-13T03:07:45Z |
| **Flightradar24** | [Flightradar24-11.13.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Flightradar24-11.13.0.apk) | - | 2026-11-11T09:29:12Z |
| **Gboard** | [Gboard-18.5.4.993608717-release-arm64-v8a.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/Gboard-18.5.4.993608717-release-arm64-v8a.apk) | - | 2026-11-13T03:07:45Z |
| **Google Maps** | [Google-Maps-26.36.04.973607363.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/Google-Maps-26.36.04.973607363.apk) | - | 2026-11-13T03:07:45Z |
| **Hungry Shark Evolution** | [Hungry-Shark-Evolution-14.5.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Hungry-Shark-Evolution-14.5.0.apk) | - | 2026-11-11T09:29:12Z |
| **Instagram** | [Instagram-453.0.0.57.79.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Instagram-453.0.0.57.79.apk) | - | 2026-11-11T09:29:12Z |
| **Lightroom** | [Lightroom-11.5.31.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Lightroom-11.5.31.apk) | - | 2026-11-11T09:29:12Z |
| **MicroG** | [microg.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/microg.apk) | - | 2026-11-11T09:29:12Z |
| **Moviebox** | [Moviebox-4.0.03.1020.03.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/Moviebox-4.0.03.1020.03.apk) | - | 2026-11-13T03:07:45Z |
| **MX Player** | [MX-Player-3.4.3.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/MX-Player-3.4.3.apk) | - | 2026-11-11T09:29:12Z |
| **Pinterest** | [Pinterest-14.23.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Pinterest-14.23.0.apk) | - | 2026-11-11T09:29:12Z |
| **Pixiv** | [Pixiv-6.196.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Pixiv-6.196.0.apk) | - | 2026-11-11T09:29:12Z |
| **Proton VPN** | [Proton-VPN-5.21.9.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/Proton-VPN-5.21.9.0.apk) | - | 2026-11-13T03:07:45Z |
| **Plants vs Zombies** | [Plants-vs-Zombies-3.18.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Plants-vs-Zombies-3.18.0.apk) | - | 2026-11-11T09:29:12Z |
| **Reddit** | [Reddit-2026.42.0.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Reddit-2026.42.0.apk) | - | 2026-11-11T09:29:12Z |
| **Termius** | [Termius-7.11.2.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Termius-7.11.2.apk) | - | 2026-11-11T09:29:12Z |
| **Stellarium** | [Stellarium-1.16.3.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Stellarium-1.16.3.apk) | - | 2026-11-11T09:29:12Z |
| **Sticker Maker** | [Sticker-Maker-1.0.12-7.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Sticker-Maker-1.0.12-7.apk) | - | 2026-11-11T09:29:12Z |
| **Strava** | [Strava-486.11.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/Strava-486.11.apk) | - | 2026-11-13T03:07:45Z |
| **Threads** | [Threads-454.0.0.54.79.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/Threads-454.0.0.54.79.apk) | - | 2026-11-11T09:29:12Z |
| **TikTok** | [TikTok-47.7.3.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/TikTok-47.7.3.apk) | - | 2026-11-13T03:07:45Z |
| **WPS Office** | [WPS-Office-18.24.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/WPS-Office-18.24.apk) | - | 2026-11-11T09:29:12Z |
| **X Piko** | [X-Piko-12.41.0-prod.01.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/X-Piko-12.41.0-prod.01.apk) | - | 2026-11-11T09:29:12Z |
| **YouTube** | [YouTube-21.13.164.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/YouTube-21.13.164.apk) | [YouTube-21.13.164-module.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/YouTube-21.13.164-module.zip) | 2026-11-11T09:29:12Z |
| **YouTube Music** | [YouTube-Music-9.15.51.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/YouTube-Music-9.15.51.apk) | [YouTube-Music-9.15.51-module.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/YouTube-Music-9.15.51-module.zip) | 2026-11-11T09:29:12Z |



### Bundle Modules (Multi-App)

| Bundle Name | Included Apps & Payload Files | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- | :--- |
| **All Bundle**<br><code>revancex-bundle-all.zip</code> | <details><summary><b>30 Apps / APKs (Click to view)</b></summary><ul><li><b>AdGuard</b>: <code>apks/adguard.apk</code></li><li><b>CamScanner</b>: <code>apks/camscanner.apk</code></li><li><b>CapCut</b>: <code>apks/capcut.apk</code></li><li><b>Cloudflare WARP</b>: <code>apks/cloudflare_warp.apk</code></li><li><b>DramaBox</b>: <code>apks/drama_box.apk</code></li><li><b>Duolingo</b>: <code>apks/duolingo.apk</code></li><li><b>Facebook</b>: <code>apks/facebook.apk</code></li><li><b>Flightradar24</b>: <code>apks/flightradar24.apk</code></li><li><b>Gboard</b>: <code>apks/gboard.apk</code></li><li><b>Google Maps</b>: <code>apks/google_maps.apk</code></li><li><b>Hungry Shark Evolution</b>: <code>apks/hungry_shark_evolution.apk</code></li><li><b>Instagram</b>: <code>apks/instagram.apk</code></li><li><b>Lightroom</b>: <code>apks/lightroom.apk</code></li><li><b>Moviebox</b>: <code>apks/moviebox.apk</code></li><li><b>MX Player</b>: <code>apks/mx_player.apk</code></li><li><b>Pinterest</b>: <code>apks/pinterest.apk</code></li><li><b>Pixiv</b>: <code>apks/pixiv.apk</code></li><li><b>Proton VPN</b>: <code>apks/proton_vpn.apk</code></li><li><b>Plants vs Zombies</b>: <code>apks/pvz_free.apk</code></li><li><b>Reddit</b>: <code>apks/reddit.apk</code></li><li><b>Termius</b>: <code>apks/serverauditor.apk</code></li><li><b>Stellarium</b>: <code>apks/stellarium.apk</code></li><li><b>Sticker Maker</b>: <code>apks/sticker_maker.apk</code></li><li><b>Strava</b>: <code>apks/strava.apk</code></li><li><b>Threads</b>: <code>apks/threads.apk</code></li><li><b>TikTok</b>: <code>apks/tiktok.apk</code></li><li><b>WPS Office</b>: <code>apks/wps_office.apk</code></li><li><b>X Piko</b>: <code>apks/x_piko.apk</code></li><li><b>YouTube</b>: <code>apks/youtube.apk</code></li><li><b>YouTube Music</b>: <code>apks/youtube_music.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/revancex-bundle-all.zip) | `auto-v1.1.0-2026.11.13-b176` | 2026-11-13T03:07:45Z |
| **Core Essentials Bundle**<br><code>revancex-bundle-core.zip</code> | <details><summary><b>4 Apps / APKs (Click to view)</b></summary><ul><li><b>YouTube</b>: <code>apks/youtube.apk</code></li><li><b>YouTube Music</b>: <code>apks/youtube_music.apk</code></li><li><b>Reddit</b>: <code>apks/reddit.apk</code></li><li><b>X Piko</b>: <code>apks/x_piko.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/revancex-bundle-core.zip) | `auto-v1.1.0-2026.11.11-b174` | 2026-11-11T09:29:12Z |
| **Multimedia Bundle**<br><code>revancex-bundle-multimedia.zip</code> | <details><summary><b>2 Apps / APKs (Click to view)</b></summary><ul><li><b>YouTube</b>: <code>apks/youtube.apk</code></li><li><b>YouTube Music</b>: <code>apks/youtube_music.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/revancex-bundle-multimedia.zip) | `auto-v1.1.0-2026.11.11-b174` | 2026-11-11T09:29:12Z |
| **Productivity Bundle**<br><code>revancex-bundle-productivity.zip</code> | <details><summary><b>3 Apps / APKs (Click to view)</b></summary><ul><li><b>CamScanner</b>: <code>apks/camscanner.apk</code></li><li><b>Lightroom</b>: <code>apks/lightroom.apk</code></li><li><b>WPS Office</b>: <code>apks/wps_office.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.11-b174/revancex-bundle-productivity.zip) | `auto-v1.1.0-2026.11.11-b174` | 2026-11-11T09:29:12Z |
| **Social Bundle**<br><code>revancex-bundle-social.zip</code> | <details><summary><b>7 Apps / APKs (Click to view)</b></summary><ul><li><b>Reddit</b>: <code>apks/reddit.apk</code></li><li><b>TikTok</b>: <code>apks/tiktok.apk</code></li><li><b>Instagram</b>: <code>apks/instagram.apk</code></li><li><b>Facebook</b>: <code>apks/facebook.apk</code></li><li><b>Threads</b>: <code>apks/threads.apk</code></li><li><b>X Piko</b>: <code>apks/x_piko.apk</code></li><li><b>Pixiv</b>: <code>apks/pixiv.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/revancex-bundle-social.zip) | `auto-v1.1.0-2026.11.13-b176` | 2026-11-13T03:07:45Z |
| **Tools Bundle**<br><code>revancex-bundle-tools.zip</code> | <details><summary><b>2 Apps / APKs (Click to view)</b></summary><ul><li><b>AdGuard</b>: <code>apks/adguard.apk</code></li><li><b>Proton VPN</b>: <code>apks/proton_vpn.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.1.0-2026.11.13-b176/revancex-bundle-tools.zip) | `auto-v1.1.0-2026.11.13-b176` | 2026-11-13T03:07:45Z |



### Custom Bundle Modules

| Custom Bundle | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- |
| _None yet_ | - | - | - |

<!-- AUTO-APP-LIST-END -->

---

## Guides & Reference

<details>
<summary><b>Download & Installation (Prebuilt CLI)</b></summary>

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
<summary><b>Step-by-Step Build Workflow</b></summary>

#### Step 0: Check Environment & Prerequisites

Verify that your system has all required and recommended tools installed:

```bash
./revancex doctor
```

*(On Windows: `.\revancex.exe doctor`)*

This verifies Java (JDK 17+ per `config/build.yaml`; default `21` if omitted), Android tools (`adb`, `keytool`, `zipalign`), optional helpers (`aria2c`, `apkeep`, `python`), bypass services (`FlareSolverr`, `Trawl`), `GITHUB_TOKEN`, `Cargo.lock` presence, keystore resolution, and configuration summary.

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

  *(By default, `build` always writes `<output>/BUILD_REPORT.json` and a companion `<output>/BUILD_REPORT.md` summary.)*

- **Supported architectures (`--arch`)**:
  - `arm64-v8a` (Recommended for modern 64-bit Android devices)
  - `armeabi-v7a` (32-bit legacy devices)
  - `x86_64` (Emulators and Chromebooks)
  - `all` (Universal builds)

- **Build / optimization modes (`--mode`)**:
  - `--mode full` (or `patch` in config): Keeps all resources and builds standard patched APK (default).
  - `--mode lite`: Strips foreign architectures and unused resources for minimal APK size.
  - `--mode both`: Builds both non-root APK and root module payload.
  - `--mode root` / `--mode module`: Builds root module payload only.
  - `--mode install`: Downloads/re-signs stock APK without patching (e.g. `microg`).

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

- When devices are connected via ADB (or with `--require-device`), it tests APK installation, launch, and checks logcat for crash loops or fatal exceptions.
- Without connected devices (or with `REVANCEX_STATIC_TEST=1`), it performs deep static inspection (valid ZIP structure, valid `classes.dex`, readable `AndroidManifest.xml`).

#### Step 5: Create Magisk & KernelSU Modules (Root)

Package patched APKs into flashable root modules (`--include-webui` defaults to `true`, and `config/modules.yaml` is synced automatically from `config/apps.yaml`):

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
  ./revancex module --bundle --apps all --output ./modules           # All Category Bundles
  ```

- **Verify Root Module Integrity**:

  ```bash
  ./revancex verify-module ./modules/revancex-bundle-core.zip
  ```

#### Step 6: Validate Configuration & Setup

Verify YAML duplicate keys, `modules.yaml` sync/package collisions, app schemas, and patch compatibility:

```bash
./revancex validate --strict
```

Or perform live reachability checks on `uptodown_url` / `apkmirror_url`:

```bash
./revancex validate --online
```

</details>

<details>
<summary><b>Module Bundle Architecture & Specifications</b></summary>

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
| **All** | `revancex-bundle-all.zip` | 30 | AdGuard, CamScanner, CapCut, Cloudflare WARP, DramaBox, Duolingo, Facebook, Flightradar24, Gboard, Google Maps, Hungry Shark Evolution, Instagram, Lightroom, Moviebox, MX Player, Pinterest, Pixiv, Proton VPN, Plants vs Zombies, Reddit, Termius, Stellarium, Sticker Maker, Strava, Threads, TikTok, WPS Office, X Piko, YouTube, YouTube Music |
| **Core Essentials** | `revancex-bundle-core.zip` | 4 | YouTube, YouTube Music, Reddit, X Piko |
| **Multimedia** | `revancex-bundle-multimedia.zip` | 2 | YouTube, YouTube Music |
| **Productivity** | `revancex-bundle-productivity.zip` | 3 | CamScanner, Lightroom, WPS Office |
| **Social** | `revancex-bundle-social.zip` | 7 | Reddit, TikTok, Instagram, Facebook, Threads, X Piko, Pixiv |
| **Tools** | `revancex-bundle-tools.zip` | 2 | AdGuard, Proton VPN |
<!-- AUTO-BUNDLE-BREAKDOWN-END -->

</details>

<details>
<summary><b>CLI Subcommand Reference</b></summary>

Global flags: `-v, --verbose` (detailed console logs), `-c, --config-dir <dir>` (alias `--config`, default `./config`).

| Command | Description | Example |
| --- | --- | --- |
| `doctor` | Inspect local environment, Java, Android tools, bypass services, `Cargo.lock`, keystore, and config summary | `revancex doctor` |
| `apps` | List all configured apps, status, and package IDs | `revancex apps [--json]` |
| `download-tools` | Download patcher JARs and patch bundles | `revancex download-tools [--apps youtube]` |
| `check-updates` | Poll upstream sources for app and patch updates | `revancex check-updates [--json]` |
| `build` | Fetch, verify, patch, sign APKs, and emit `BUILD_REPORT.json` + `BUILD_REPORT.md` | `revancex build --apps youtube --arch arm64-v8a [--mode full\|lite\|both\|root\|install] [--force] [--keep-going] [--report <path>]` |
| `module` | Sync `modules.yaml` and package APKs into single or bundle Magisk/KernelSU/APatch modules | `revancex module --bundle --apps core [--include-webui=true\|false] [--output ./modules]` |
| `test` | Smoke test APKs (static DEX/manifest or ADB device) with failure threshold policies | `revancex test --apps all [--require-device] [--fail-threshold 20] [--continue] [--json]` |
| `validate` | Check YAML duplicate keys, `modules.yaml` sync/collisions, app definitions, and patch compatibility | `revancex validate [--strict] [--online] [--apps youtube]` |
| `clean` | Clean build temp cache (`--all` also clears `tools/`) or prune stale tool files | `revancex clean [--all] [--stale-tools]` |
| `fetch-apk` | Download stock APK using the configured multi-source chain | `revancex fetch-apk youtube [--arch arm64-v8a] [--dry-run] [--force]` |
| `verify-module` | Verify structure and integrity of a root module zip | `revancex verify-module ./modules/revancex-bundle-core.zip` |
| `patches` | List patch plan (included, excluded, auto-disabled) for an app | `revancex patches youtube [--mode root\|nonroot] [--json]` |
| `keystore` | Inspect resolved signing keystore details | `revancex keystore --info` |
| `report` | Inspect `BUILD_REPORT.json` and enforce failure threshold policies in CI | `revancex report check ./output/BUILD_REPORT.json [--fail-threshold 15] [--continue]` |
| `sources` | Probe and benchmark configured APK sources for reachability and version resolution | `revancex sources probe youtube [--arch arm64-v8a] [--version <ver>]` |
| `gen-apps-json` | Generate web catalogue dataset `docs/apps.json` from release registry | `revancex gen-apps-json [--registry config/releases_registry.json]` |
| `generate-matrix` | Generate CI build matrix JSON for GitHub Actions | `revancex generate-matrix [--apps all] [--arch all] [--mode auto]` |

</details>

<details>
<summary><b>Build from Source (Developers)</b></summary>

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
<summary><b>GitHub Actions (Automated Cloud Builds)</b></summary>

1. Fork or push to this repository (default CI branch: `extended`).
2. Automated GitHub Actions workflows handle validation, builds, and releases:
   - **Check Patch Updates** (`check-updates.yml`): Runs every 23 hours or manually to poll upstream patch releases and trigger builds when changes are detected.
   - **Auto Build** (`auto-build.yml`) / **Build and Release** (`build-and-release.yml`): Runs FlareSolverr + Trawl services, builds APKs and single/bundle modules, runs `report check` & static smoke tests, uploads `build-logs-*` and `build-report-*` artifacts, and publishes GitHub Releases.
   - **Custom Bundle Build** (`custom-build.yml`): Triggered via `repository_dispatch` from the GitHub Pages catalogue or `workflow_dispatch`.
   - **Update GitHub Pages** (`update-pages.yml`): Syncs `config/releases_registry.json`, `README.md`, and `docs/apps.json`.
   - **CI Validation** (`ci.yml`) & **Release CLI Binaries** (`release-cli.yml`): Runs `cargo fmt`, `cargo clippy`, Rust/script tests, and builds precompiled Linux/Windows CLI binaries.
3. Download the generated APKs and Magisk/KernelSU modules directly from the **Releases** tab.

</details>

<details>
<summary><b>Environment Variables</b></summary>

Copy `.env.example` to `.env` and configure credentials. Never commit `.env`.

| Variable | Description | Default / Example |
| --- | --- | --- |
| `GITHUB_TOKEN` | Token with `repo` + `workflow` scopes (prevents GitHub API rate limits) | `ghp_...` |
| `GITLAB_TOKEN` | (Optional) GitLab token for fetching GitLab-hosted releases without rate limits | `glpat-...` |
| `KEYSTORE_PASSWORD` | Keystore password for signing generated APKs | `revanced` / `666` |
| `KEYSTORE_FILE` | (Optional) Custom keystore path override | `keys/custom.jks` |
| `KEYSTORE_B64` | (Optional) Base64-encoded keystore for CI pipelines | `<base64>` |
| `TG_TOKEN` | (Optional) Telegram bot token | `123456:ABC...` |
| `TG_CHAT` | (Optional) Telegram chat ID or channel username | `-1001234567890` |
| `TG_TOPIC` | (Optional) Telegram forum topic / thread ID | `42` |
| `FLARESOLVERR_URL` | (Optional) FlareSolverr endpoint URL | `http://localhost:8191/v1` |
| `FLARESOLVERR_ENABLED` | (Optional) Override FlareSolverr mode (`auto`, `true`, `false`) | `auto` |
| `TRAWL_URL` | (Optional) Trawl headless browser render service endpoint URL | `http://localhost:8000` |
| `REVANCEX_STATIC_TEST` | (Optional) Force static manifest/DEX testing without ADB device | `1` |
| `REVANCEX_CB_THRESHOLD` | (Optional) Circuit breaker consecutive failure threshold (`REVANCEX_UTD_CB_THRESHOLD` fallback) | `3` |
| `REVANCEX_CB_COOLDOWN_SECS` | (Optional) Circuit breaker cooldown period in seconds | `60` |
| `REVANCEX_APP_TIMEOUT_SECS` | (Optional) Default per-app build timeout budget in seconds | `600` |
| `REVANCEX_APKEEP_ACKNOWLEDGE_DANGERS` | (Optional) Acknowledge `apkeep` danger prompt (`true`/`1`) | `false` |
| `REVANCEX_CONFIG_DIR` | (Optional) Override path to config directory | `./config` |
| `REVANCEX_TEMP_DIR` | (Optional) Override path to temporary directory | `./tmp` |
| `REVANCEX_LOG_DIR` | (Optional) Override path to log output directory | `build/logs` |
| `VERBOSE` / `RUST_LOG` | (Optional) Increase console log detail (`VERBOSE=1`, `RUST_LOG=debug`, or `RUNNER_DEBUG=1`) | `0` |
| `NO_COLOR` / `REVANCEX_ASCII` | (Optional) Disable ANSI colors / force ASCII `[OK]`/`[FAIL]`/`[WARN]` markers | unset |

</details>

<details>
<summary><b>Adding a New App</b></summary>

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
<summary><b>Upstream Patch Sources</b></summary>

> Sourced dynamically from [`config/sources.yaml`](config/sources.yaml).

<!-- AUTO-PATCH-SOURCES-START -->
| ID | Repository | Asset Pattern |
| :--- | :--- | :--- |
| `anddea` | [anddea/revanced-patches](https://github.com/anddea/revanced-patches) | `*.mpp` |
| `de_revanced` | [RookieEnough/De-Vanced](https://github.com/RookieEnough/De-Vanced) | `*.mpp` |
| `piko` | [crimera/piko](https://github.com/crimera/piko) | `*.mpp` |
| `piko_ig_lite` | [crimera/piko-ig-lite](https://github.com/crimera/piko-ig-lite) | `*.mpp` |
| `piko_newx` | [crimera/piko-newx](https://github.com/crimera/piko-newx) | `*.mpp` |
| `morphe_patches` | [MorpheApp/morphe-patches](https://github.com/MorpheApp/morphe-patches) | `*.mpp` |
| `icysymmetra` | [icysymmetra/tiktok-patches-for-morphe](https://github.com/icysymmetra/tiktok-patches-for-morphe) | `*.mpp` |
| `hoodles` | [hoo-dles/morphe-patches](https://github.com/hoo-dles/morphe-patches) | `*.mpp` |
| `rushiranpise` | [rushiranpise/morphe-patches](https://github.com/rushiranpise/morphe-patches) | `*.mpp` |
| `durgesh` | [durgesh0505/chiggi_morphe_patches](https://github.com/durgesh0505/chiggi_morphe_patches) | `*.mpp` |
| `arandomhooman` | [arandomhooman/hoomans-morphe-patches](https://github.com/arandomhooman/hoomans-morphe-patches) | `*.mpp` |
| `hxreborn` | [hxreborn/morphe-patches](https://github.com/hxreborn/morphe-patches) | `*.mpp` |
| `bholey` | [BholeyKaBhakt/android-patches-xtra](https://github.com/BholeyKaBhakt/android-patches-xtra) | `*.mpp` |
| `jkennethcarino` | [jkennethcarino/adobo](https://github.com/jkennethcarino/adobo) | `*.mpp` |
| `sapitosucio` | [SapitoSucio/FroggoMorphePatches](https://github.com/SapitoSucio/FroggoMorphePatches) | `*.mpp` |
| `browzomje` | [browzomje/browzomje-patches](https://github.com/browzomje/browzomje-patches) | `*.mpp` |
| `blazeftl` | [BlazeFTL/FTL-Patches](https://github.com/BlazeFTL/FTL-Patches) | `*.mpp` |
| `byehi98` | [byehi98/okish-morphe-patches](https://github.com/byehi98/okish-morphe-patches) | `*.mpp` |
| `hushfeed` | [SysAdminDoc/HushFeed](https://github.com/SysAdminDoc/HushFeed) | `*.mpp` |
| `hushfacebook` | [SysAdminDoc/Hushfacebook](https://github.com/SysAdminDoc/Hushfacebook) | `*.mpp` |
| `hushthreads` | [SysAdminDoc/HushThreads](https://github.com/SysAdminDoc/HushThreads) | `*.mpp` |
| `legendsciber` | [legendsciber/morphe-patches](https://github.com/legendsciber/morphe-patches) | `*.mpp` |
| `bearinmindcat` | [bearinmindcat/morphe-patches](https://github.com/bearinmindcat/morphe-patches) | `*.mpp` |
| `jasonwu` | [jasonwu1994/Gboard-patches](https://github.com/jasonwu1994/Gboard-patches) | `*.mpp` |
| `kveld9` | [kveld9/kveld-extra-morphe-patches](https://github.com/kveld9/kveld-extra-morphe-patches) | `*.mpp` |
| `frip` | [Fripe070/PixivPatches](https://github.com/Fripe070/PixivPatches) | `*.mpp` |
| `madush` | [madushas/morphe-patches-custom](https://github.com/madushas/morphe-patches-custom) | `*.mpp` |
<!-- AUTO-PATCH-SOURCES-END -->

</details>

