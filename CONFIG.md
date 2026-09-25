# ReVanceX Configuration Guide

Comprehensive reference and schema specification for all configuration files located in `./config/`.

---

## 1. Target Applications (`config/apps.yaml`)

Defines all target Android applications, upstream download sources, patch sets, keystore profiles, and module packaging rules.

### Schema Specification

| Field | Type | Description |
| :--- | :--- | :--- |
| `enabled` | `boolean` | Whether this application is built during batch builds (`build --apps all`). |
| `package` | `string` | Official Android package identifier (e.g. `com.google.android.youtube`). |
| `patch_source` | `string` | Key from `config/sources.yaml` under `patch_sources` specifying the patch bundle. |
| `cli_source` | `string` | Upstream tool repo used for patching (e.g. `MorpheApp/morphe-cli` or `MorpheApp/morphe-desktop`). |
| `apkmirror_url` | `string` *(optional)* | APKMirror URL or path slug used to locate base APKs. |
| `uptodown_url` | `string` *(optional)* | Uptodown URL or package path slug for stock APK downloads. |
| `apkpure_url` | `string` *(optional)* | APKPure application URL slug. |
| `aptoide_url` | `string` *(optional)* | Aptoide package slug or direct package ID. |
| `apk_url` | `string` *(optional)* | Direct APK download link or GitHub release asset URL. |
| `architectures` | `list<string>` | Supported ABIs (typically `[arm64-v8a]`, `[armeabi-v7a]`, or `[x86_64]`). |
| `mode` | `string` | Build target mode: `patch` (patch and build APK), `stock` (download unpatched base APK), or `bundle` (package into module). |
| `version` | `string` *(optional)* | Specific version pin (e.g. `19.16.39`) or `auto` to resolve the optimal version dynamically from patch metadata. |
| `version_pin_strict` | `boolean` *(optional)* | If `true`, fails if the pinned version cannot be fetched, rather than falling back to compatible alternatives. |
| `patches` | `list<string>` *(optional)* | Explicit list of patches to apply. If omitted or empty, all applicable patches from the source bundle are used. |
| `excluded_patches` | `list<string>` *(optional)* | Explicit list of patches to exclude during patching. |
| `name` | `string` *(optional)* | Clean application display name used for public artifact naming (e.g. `YouTube`, `Nova Launcher`). Defaults to title-cased app id. |
| `keystore` | `object` *(optional)* | Keystore configuration with `alias` and `file` path for signing. |
| `module` | `object` *(optional)* | Root module flags: `single` (emit individual flashable zip) and `bundle` (include in categorized bundles). |
| `dependencies` | `list<string>` *(optional)* | Dependent applications required by this app (e.g. `[microg]`). |
| `post_patch_hooks` | `list<object>` *(optional)* | Hook scripts executed after patching (e.g. `scripts/custom_hook.py`). |

### Example

```yaml
apps:
  youtube:
    name: YouTube
    enabled: true
    package: com.google.android.youtube
    patch_source: anddea
    cli_source: MorpheApp/morphe-cli
    apkmirror_url: https://www.apkmirror.com/apk/google-inc/youtube/
    uptodown_url: https://youtube.en.uptodown.com/android
    apkpure_url: https://apkpure.com/youtube/com.google.android.youtube
    aptoide_url: com.google.android.youtube
    architectures:
      - arm64-v8a
    mode: patch
    version: auto
    excluded_patches:
      - "Custom branding"
    keystore:
      alias: revanced
      file: keys/youtube.jks
    module:
      single: true
      bundle: true
    dependencies:
      - microg
    post_patch_hooks:
      - script: scripts/custom_hook.py
        when: root
        optional: true

  telegram:
    enabled: true
    package: org.telegram.messenger
    patch_source: rushiranpise
    cli_source: MorpheApp/morphe-cli
    uptodown_url: https://telegram.en.uptodown.com/android
    architectures:
      - arm64-v8a
    mode: patch
    keystore:
      alias: revanced
      file: keys/telegram.jks
    module:
      single: false
      bundle: true
    dependencies: []
```

---

## 2. Tools & Patch Sources (`config/sources.yaml`)

Configures upstream CLI patchers, patch bundle repositories, APK download providers, source precedence, and circuit breaker fault tolerance.

### Specification

```yaml
# Patcher CLI tool definition
cli:
  repo: MorpheApp/morphe-desktop
  asset_pattern: "morphe-*-all.jar"

# Patch sources repository catalog
patch_sources:
  anddea:
    repo: anddea/revanced-patches
    asset_pattern: "*.mpp"
    branch: dev

  rushiranpise:
    repo: rushiranpise/morphe-patches
    asset_pattern: "*.mpp"

  piko:
    repo: crimera/piko
    asset_pattern: "*.mpp"

# Downloader providers and API settings
apk_sources:
  uptodown:
    base_url: https://uptodown.com
  apkmirror:
    base_url: https://www.apkmirror.com
  archive:
    base_url: https://archive.org/download/geologically/apks
  apkpure_api:
    enabled: true
    headers:
      User-Agent: "Dalvik/2.1.0 (Linux; U; Android 13; Pixel 5 Build/TQ3A.230901.001); APKPure/3.20.34 (Aegon)"
      Ual-Access-Businessid: "projecta"
      Ual-Access-ProjectA: '{"device_info":{"abis":["x86_64","arm64-v8a","x86","armeabi-v7a","armeabi"],"android_id":"50f838123d9a9c94","brand":"google","country":"United States","country_code":"US","imei":"","language":"en-US","manufacturer":"Google","mode":"Pixel 5","os_ver":"33","os_ver_name":"13","platform":1,"product":"redfin","screen_height":1080,"screen_width":1920},"host_app_info":{"build_no":"468","channel":"","md5":"6756e53158d6f6c013650a40d8f1147b","pkg_name":"com.apkpure.aegon","sdk_ver":"3.20.34","version_code":3203427,"version_name":"3.20.34"}}'
  apkeep:
    acknowledge_dangers: false

# Fallback order when acquiring base APKs
apk_source_order:
  - github
  - gitlab
  - archive
  - codeberg
  - apkeep
  - apkpure
  - apkmirror
  - uptodown
  - aptoide
  - apkfab
  - apkcombo

# Circuit breaker parameters to isolate failing or rate-limited providers
circuit_breakers:
  default:
    threshold: 3           # Consecutive failures before tripping
    cooldown_secs: 60      # Cooldown time before half-open retry
    cooldown_apps: 5       # Alternatively trip for N successive applications
  sources:
    uptodown:
      threshold: 3
      cooldown_secs: 60
      cooldown_apps: 5
    apkmirror:
      threshold: 2
      cooldown_secs: 120
      cooldown_apps: 3
```

---

## 3. Build & Patcher Parameters (`config/build.yaml`)

Controls JVM requirements, local workspace directories, worker concurrency, automated patch selection rules, keystore profiles, and smoke test parameters.

### Specification

```yaml
build:
  java_min_version: 17
  tools_dir: ./tools
  output_dir: ./output
  keys_dir: ./keys
  temp_dir: ./tmp

  # Keystore defaults for signing generated APKs
  keystore:
    validity_days: 10000
    key_algorithm: RSA
    key_size: 4096
    sig_algorithm: SHA256withRSA
    distinguished_name: "CN=Revancex, OU=Ministry of Silly Builds, O=Thunderkex, L=CyberSpace, ST=Somewhere Over The Galaxy, C=XX"

  # Parallel build concurrency
  workers: 4

  # Download resilience and fallback
  download_retries: 3
  download_timeout_secs: 120
  version_fallback_attempts: 3

  version_cache: ./config/version_cache.json
  asset_exclude_patterns:
    - "hw-"

  # Automated patch exclusion and dependency injection
  patcher:
    max_patch_retries: 2
    auto_disable: incompatible       # incompatible | off | aggressive
    rules:
      root:
        exclude:
          - contains: "custom branding"
          - contains: "change package name"
          - contains: "change app icon"
          - contains: "gmscore"
          - contains: "microg"
          - equals: "GmsCore support"
          - equals: "Change package name"
          - equals: "MicroG support"
          - contains: "gmscore support"
          - contains: "microg support"
      nonroot:
        exclude:
          - contains: "custom branding"
        include_if_dependency:
          microg:
            - contains: "gmscore support"

  # Smoke test suite parameters
  testing:
    enabled: auto                    # auto (tests when available) | true | false
    startup_grace_secs: 8            # Wait duration after app launch
    install_timeout_secs: 180        # ADB install timeout
    logcat_lines: 80                 # Log lines captured on crash
    crash_patterns:
      - "FATAL EXCEPTION"
      - "ANR in"
      - "E AndroidRuntime"
```

---

## 4. Root Module Bundles (`config/modules.yaml`)

Specifies the grouping of applications into categorized, flashable Magisk, KernelSU, and APatch module zip files, as well as the list of standalone single modules.

```yaml
modules:
  all:
    apps:
      - adguard
      - camscanner
      - duolingo
      - facebook
      - flightradar24
      - gboard
      - google_photos
      - hungry_shark_world
      - instagram
      - lightroom
      - moviebox
      - myfitnesspal
      - nova_launcher
      - pixiv
      - prime_video
      - proton_vpn
      - reddit
      - strava
      - threads
      - tiktok
      - wps_office
      - x_piko
      - youtube
      - youtube_music

  core:
    apps:
      - youtube
      - youtube_music
      - reddit
      - x_piko

  multimedia:
    apps:
      - youtube
      - youtube_music
      - prime_video

  productivity:
    apps:
      - camscanner
      - lightroom
      - wps_office

  social:
    apps:
      - reddit
      - tiktok
      - instagram
      - facebook
      - threads
      - x_piko
      - pixiv

  tools:
    apps:
      - adguard
      - proton_vpn

# Apps packaged as standalone single-app modules
single_modules:
  - youtube
  - youtube_music
```

---

## 5. Root Module Metadata (`config/module.yaml`)

Emitted into `module.prop` inside generated Magisk, KernelSU, and APatch module archives.

```yaml
module:
  id: revancex
  name: "ReVanceX"
  version: "1.0"
  author: "Thunderkex"
  description: "Patched apps bundle"
  updateJson: ""
  min_magisk: 20400
  min_kernelsu: 10200
```

---

## 6. Build Reporting (`BUILD_REPORT.json`)

When running `revancex build` with `--report <path>`, ReVanceX emits a structured build report tracking execution status, source download outcomes, applied patches, and failure diagnostics.

### Schema Overview

```json
{
  "schema_version": "1.0",
  "generated_at": "2026-10-21T13:40:00Z",
  "summary": {
    "total": 38,
    "passed": 36,
    "failed": 2,
    "duration_secs": 420
  },
  "apps": {
    "youtube": {
      "status": "passed",
      "target_version": "19.16.39",
      "fetched_version": "19.16.39",
      "resolved_version": "19.16.39",
      "resolved_source": "apkmirror",
      "patches_included": ["Seekbar tapping", "Spoof client"],
      "attempted_sources": [
        {
          "source": "apkmirror",
          "outcome": "success",
          "version": "19.16.39",
          "reason": "downloaded valid apk",
          "duration_ms": 2340
        }
      ]
    }
  }
}
```

### Failure Classifications

ReVanceX automatically classifies build failures for automated triage:
- `all_sources_exhausted`: None of the configured download providers could supply the base APK.
- `version_not_supported`: No upstream source has an APK compatible with the enabled patch bundle range.
- `integrity_check_failed`: Downloaded file failed ZIP header, `classes.dex`, or `AndroidManifest.xml` inspection.
- `patcher_error`: The patcher JAR failed to apply one or more patches to the target APK bytecode.
- `version_conflict`: Two or more enabled patches require mutually disjoint application versions.
- `arch_unsupported`: Downloaded APK does not contain libraries compatible with the target architecture.

### CI/CD Evaluation

Use `revancex report check` to evaluate reports and enforce quality gates:

```bash
# Exit non-zero only if failed apps exceed 15%
revancex report check ./output/BUILD_REPORT.json --fail-threshold 15

# Warning mode: always exits 0 to allow releasing successful apps
revancex report check ./output/BUILD_REPORT.json --continue
```
