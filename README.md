<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css">

<p align="center">
  <img src="assets/logo.png" alt="ReVanceX" width="600" style="background: #ffffff; border-radius: 16px; padding: 16px;">
</p>

# ReVanceX

Automated patch orchestration ecosystem written in Rust. It streamlines patching Android applications across multiple upstream patch sources, generating standalone signed APKs, individual Magisk/KernelSU modules, and pre-categorized multi-app bundles with an embedded management WebUI.

## Features

- **40+ Pre-Configured Apps**: YouTube, YouTube Music, Spotify, Reddit, MicroG, Soundcloud, Twitter/X, and more.
- **Fast Rust CLI (`revancex`)**: High-performance local building, version resolution, and APK patching with no runtime dependencies beyond Java JDK 17.
- **Root & Non-Root Packaging**: Generate standalone signed APKs, individual Magisk/KernelSU modules, or multi-app bundle zips.
- **Embedded WebUI**: KernelSU and Magisk modules include an interactive, responsive WebUI for managing mounted apps.
- **Dynamic Configuration**: Fully decoupled definitions for apps (`apps.yaml`), patch profiles (`patches.yaml`), and upstream sources (`sources.yaml`).
- **Automated CI/CD**: Scheduled and on-demand builds via GitHub Actions with GitHub Pages catalogue.

---

## <i class="fa-solid fa-mobile-screen"></i> Supported Applications & Releases

Browse apps visually on our [Web Catalogue](https://thunderkex.github.io/revancex/) or download latest builds below. App configurations are defined in [`config/apps.yaml`](config/apps.yaml).

<!-- AUTO-APP-LIST-START -->

### <i class="fa-solid fa-mobile-screen"></i> Stock Apps & Single Root Modules

| App | Stock APK | Root APK | Single Root Module | Last Updated |
| :--- | :--- | :--- | :--- | :--- |
| **Adguard** | [adguard-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/adguard-patched.apk) | — | — | 2026-09-28 19:13:02 UTC |
| **Camscanner** | [camscanner-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/camscanner-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Capcut** | [capcut-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/capcut-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Drama Box** | [drama_box-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/drama_box-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Duolingo** | [duolingo-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/duolingo-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Facebook** | [facebook-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.09-b114/facebook-patched.apk) | — | — | 2026-10-09 03:01:49 UTC |
| **Flightradar24** | [flightradar24-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/flightradar24-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Gboard** | [gboard-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/gboard-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Google Maps** | [google_maps-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.10-b115/google_maps-patched.apk) | — | — | 2026-10-10 02:18:05 UTC |
| **Google News** | [google_news-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/google_news-patched.apk) | — | — | 2026-09-28 19:13:02 UTC |
| **Google Phone** | [google_phone-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/google_phone-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Google Photos** | [google_photos-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/google_photos-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Hungry Shark World** | [hungry_shark_world-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/hungry_shark_world-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Instagram** | [instagram-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.10-b115/instagram-patched.apk) | — | — | 2026-10-10 02:18:05 UTC |
| **Lightroom** | [lightroom-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.30-b94/lightroom-patched.apk) | — | — | 2026-09-30 21:49:44 UTC |
| **Microg** | [microg.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/microg.apk) | — | — | 2026-09-28 19:13:02 UTC |
| **Mx Player** | [mx_player-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.08-b113/mx_player-patched.apk) | — | — | 2026-10-08 02:42:38 UTC |
| **Myfitnesspal** | [myfitnesspal-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/myfitnesspal-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Nova Launcher** | [nova_launcher-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/nova_launcher-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Pixiv** | [pixiv-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/pixiv-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Prime Video** | [prime_video-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/prime_video-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Proton Vpn** | [proton_vpn-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/proton_vpn-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Pvz Free** | [pvz_free-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/pvz_free-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Reddit** | [reddit-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/reddit-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Serverauditor** | [serverauditor-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/serverauditor-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Solid Explorer** | [solid_explorer-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.27-b28/solid_explorer-patched.apk) | — | — | 2026-09-27 09:56:46 UTC |
| **Soundcloud** | [soundcloud-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/soundcloud-patched.apk) | [soundcloud-root.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.03-b106/soundcloud-root.apk) | — | 2026-10-05 01:57:41 UTC |
| **Spotify** | [spotify-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/spotify-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Stellarium** | [stellarium-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/stellarium-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Sticker Maker** | [sticker_maker-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/sticker_maker-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Strava** | [strava-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.03-b106/strava-patched.apk) | — | — | 2026-10-03 03:12:04 UTC |
| **Subway Surfers** | [subway_surfers-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/subway_surfers-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **Telegram** | [telegram-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/telegram-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Threads** | [threads-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.04-b109/threads-patched.apk) | — | — | 2026-10-04 02:39:38 UTC |
| **Tiktok** | [tiktok-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.09-b114/tiktok-patched.apk) | — | — | 2026-10-09 03:01:49 UTC |
| **Wps Office** | [wps_office-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/wps_office-patched.apk) | — | — | 2026-10-05 01:57:41 UTC |
| **X Piko** | [x_piko-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b84/x_piko-patched.apk) | — | — | 2026-09-28 08:40:43 UTC |
| **Youtube** | [youtube-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/youtube-patched.apk) | [youtube-root.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/youtube-root.apk) | [revancex-module-youtube.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/revancex-module-youtube.zip) | 2026-09-28 19:13:02 UTC |
| **Youtube Music** | [youtube_music-patched.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/youtube_music-patched.apk) | [youtube_music-root.apk](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/youtube_music-root.apk) | [revancex-module-youtube_music.zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.09.28-b86/revancex-module-youtube_music.zip) | 2026-09-28 19:13:02 UTC |



### <i class="fa-solid fa-box-archive"></i> Bundle Modules (Multi-App)

| Bundle Name | Included Apps & Payload Files | Download Link | Release Tag | Updated At |
| :--- | :--- | :--- | :--- | :--- |
| **All Bundle**<br><code>revancex-bundle-all.zip</code> | <details><summary><b>26 Apps / APKs (Click to view)</b></summary><ul><li><b>Adguard</b>: <code>apks/adguard.apk</code></li><li><b>Camscanner</b>: <code>apks/camscanner.apk</code></li><li><b>Duolingo</b>: <code>apks/duolingo.apk</code></li><li><b>Facebook</b>: <code>apks/facebook.apk</code></li><li><b>Flightradar24</b>: <code>apks/flightradar24.apk</code></li><li><b>Gboard</b>: <code>apks/gboard.apk</code></li><li><b>Google Photos</b>: <code>apks/google_photos.apk</code></li><li><b>Hungry Shark World</b>: <code>apks/hungry_shark_world.apk</code></li><li><b>Instagram</b>: <code>apks/instagram.apk</code></li><li><b>Lightroom</b>: <code>apks/lightroom.apk</code></li><li><b>Moviebox</b>: <code>apks/moviebox.apk</code></li><li><b>Myfitnesspal</b>: <code>apks/myfitnesspal.apk</code></li><li><b>Nova Launcher</b>: <code>apks/nova_launcher.apk</code></li><li><b>Pixiv</b>: <code>apks/pixiv.apk</code></li><li><b>Prime Video</b>: <code>apks/prime_video.apk</code></li><li><b>Proton Vpn</b>: <code>apks/proton_vpn.apk</code></li><li><b>Reddit</b>: <code>apks/reddit.apk</code></li><li><b>Soundcloud</b>: <code>apks/soundcloud.apk</code></li><li><b>Spotify</b>: <code>apks/spotify.apk</code></li><li><b>Strava</b>: <code>apks/strava.apk</code></li><li><b>Threads</b>: <code>apks/threads.apk</code></li><li><b>Tiktok</b>: <code>apks/tiktok.apk</code></li><li><b>Wps Office</b>: <code>apks/wps_office.apk</code></li><li><b>X Piko</b>: <code>apks/x_piko.apk</code></li><li><b>Youtube</b>: <code>apks/youtube.apk</code></li><li><b>Youtube Music</b>: <code>apks/youtube_music.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.10-b115/revancex-bundle-all.zip) | `auto-v1.0.5-2026.10.10-b115` | 2026-10-10 02:18:05 UTC |
| **Core Essentials Bundle**<br><code>revancex-bundle-core.zip</code> | <details><summary><b>5 Apps / APKs (Click to view)</b></summary><ul><li><b>Youtube</b>: <code>apks/youtube.apk</code></li><li><b>Youtube Music</b>: <code>apks/youtube_music.apk</code></li><li><b>Reddit</b>: <code>apks/reddit.apk</code></li><li><b>Spotify</b>: <code>apks/spotify.apk</code></li><li><b>X Piko</b>: <code>apks/x_piko.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/revancex-bundle-core.zip) | `auto-v1.0.5-2026.10.05-b110` | 2026-10-05 01:57:41 UTC |
| **Multimedia Bundle**<br><code>revancex-bundle-multimedia.zip</code> | <details><summary><b>5 Apps / APKs (Click to view)</b></summary><ul><li><b>Youtube</b>: <code>apks/youtube.apk</code></li><li><b>Youtube Music</b>: <code>apks/youtube_music.apk</code></li><li><b>Spotify</b>: <code>apks/spotify.apk</code></li><li><b>Soundcloud</b>: <code>apks/soundcloud.apk</code></li><li><b>Prime Video</b>: <code>apks/prime_video.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/revancex-bundle-multimedia.zip) | `auto-v1.0.5-2026.10.05-b110` | 2026-10-05 01:57:41 UTC |
| **Productivity Bundle**<br><code>revancex-bundle-productivity.zip</code> | <details><summary><b>3 Apps / APKs (Click to view)</b></summary><ul><li><b>Camscanner</b>: <code>apks/camscanner.apk</code></li><li><b>Lightroom</b>: <code>apks/lightroom.apk</code></li><li><b>Wps Office</b>: <code>apks/wps_office.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/revancex-bundle-productivity.zip) | `auto-v1.0.5-2026.10.05-b110` | 2026-10-05 01:57:41 UTC |
| **Social Bundle**<br><code>revancex-bundle-social.zip</code> | <details><summary><b>7 Apps / APKs (Click to view)</b></summary><ul><li><b>Reddit</b>: <code>apks/reddit.apk</code></li><li><b>Tiktok</b>: <code>apks/tiktok.apk</code></li><li><b>Instagram</b>: <code>apks/instagram.apk</code></li><li><b>Facebook</b>: <code>apks/facebook.apk</code></li><li><b>Threads</b>: <code>apks/threads.apk</code></li><li><b>X Piko</b>: <code>apks/x_piko.apk</code></li><li><b>Pixiv</b>: <code>apks/pixiv.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.10-b115/revancex-bundle-social.zip) | `auto-v1.0.5-2026.10.10-b115` | 2026-10-10 02:18:05 UTC |
| **Tools Bundle**<br><code>revancex-bundle-tools.zip</code> | <details><summary><b>2 Apps / APKs (Click to view)</b></summary><ul><li><b>Adguard</b>: <code>apks/adguard.apk</code></li><li><b>Proton Vpn</b>: <code>apks/proton_vpn.apk</code></li></ul></details> | [Download Zip](https://github.com/thunderkex/revancex/releases/download/auto-v1.0.5-2026.10.05-b110/revancex-bundle-tools.zip) | `auto-v1.0.5-2026.10.05-b110` | 2026-10-05 01:57:41 UTC |



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

#### Step 5: Validate Configuration & Setup

Verify configuration syntax, app schemas, and patch integrity:

```bash
./revancex validate --strict
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
| **All** | `revancex-bundle-all.zip` | 26 | Adguard, Camscanner, Duolingo, Facebook, Flightradar24, Gboard, Google Photos, Hungry Shark World, Instagram, Lightroom, Moviebox, Myfitnesspal, Nova Launcher, Pixiv, Prime Video, Proton Vpn, Reddit, Soundcloud, Spotify, Strava, Threads, Tiktok, Wps Office, X Piko, Youtube, Youtube Music |
| **Core Essentials** | `revancex-bundle.zip` | 5 | Youtube, Youtube Music, Reddit, Spotify, X Piko |
| **Multimedia** | `revancex-bundle-multimedia.zip` | 5 | Youtube, Youtube Music, Spotify, Soundcloud, Prime Video |
| **Productivity** | `revancex-bundle-productivity.zip` | 3 | Camscanner, Lightroom, Wps Office |
| **Social** | `revancex-bundle-social.zip` | 7 | Reddit, Tiktok, Instagram, Facebook, Threads, X Piko, Pixiv |
| **Tools** | `revancex-bundle-tools.zip` | 2 | Adguard, Proton Vpn |
<!-- AUTO-BUNDLE-BREAKDOWN-END -->

</details>

<details>
<summary><b><i class="fa-solid fa-terminal"></i> CLI Subcommand Reference</b></summary>

| Command | Description | Example |
| --- | --- | --- |
| `apps` | List all configured apps and package IDs | `revancex apps` |
| `download-tools` | Download patcher JARs, integrations, and tools | `revancex download-tools` |
| `check-updates` | Poll upstream sources for app and patch updates | `revancex check-updates --json` |
| `build` | Patch and sign target applications | `revancex build --apps youtube --arch arm64-v8a` |
| `module` | Package APKs into Magisk/KernelSU zip modules | `revancex module --bundle --apps youtube,microg` |
| `validate` | Check schema and config integrity | `revancex validate --strict` |
| `clean` | Clean up build artifacts and temporary files | `revancex clean` |

</details>

<details>
<summary><b><i class="fa-solid fa-hammer"></i> Build from Source (Developers)</b></summary>

Compile the CLI directly from source:

```bash
# Prerequisites: Rust 1.75+ and JDK 17+
cargo build --release

# Run compiled binary
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
   - **CI Validation & Tests**: Runs automated tests on every push.
3. Download the generated APKs and Magisk modules directly from the **Releases** tab.

</details>

<details>
<summary><b><i class="fa-solid fa-sliders"></i> Environment Variables</b></summary>

Copy `.env.example` to `.env` and configure credentials. Never commit `.env`.

| Variable | Description |
| --- | --- |
| `GITHUB_TOKEN` | Token with `repo` + `workflow` scopes |
| `KEYSTORE_PASSWORD` | Password for generated keystores |
| `TG_TOKEN` | (Optional) Telegram bot token for build notifications |
| `TG_CHAT` | (Optional) Telegram chat ID or channel username |
| `TG_TOPIC` | (Optional) Telegram topic / thread ID for forum supergroups |

</details>

<details>
<summary><b><i class="fa-solid fa-circle-plus"></i> Adding a New App</b></summary>

Add an entry to [`config/apps.yaml`](config/apps.yaml) following the existing schema. Run `validate` to check:

```bash
./revancex validate --strict
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
<!-- AUTO-PATCH-SOURCES-END -->

</details>

