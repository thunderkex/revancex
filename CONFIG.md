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
| `patch_source` | `string` | Key from `config/sources.yaml` under `patch_sources` specifying the patch bundle (or `none` for unpatched install-mode apps like `microg`). |
| `patch_branch` | `string` *(optional)* | Override branch/tag channel for the patch source (e.g. `dev`; aliases `branch`, `patch-branch`). |
| `cli_source` | `string` *(optional)* | Upstream tool repo used for patching (defaults to `MorpheApp/morphe-desktop`). |
| `apkmirror_url` | `string` *(optional)* | APKMirror URL or path slug used to locate base APKs. |
| `uptodown_url` | `string` *(optional)* | Uptodown URL or package path slug for stock APK downloads. |
| `apkpure_url` | `string` *(optional)* | APKPure application URL slug. |
| `aptoide_url` | `string` *(optional)* | Aptoide package slug or direct package ID. |
| `archive_url` | `string` *(optional)* | Archive.org base URL or direct item URL for stock APKs. |
| `apk_source` | `string` *(optional)* | Preferred APK source identifier (e.g. `github`). |
| `apk_url` | `string` *(optional)* | Direct APK download link or GitHub release URL. |
| `sources` | `map<string, object>` *(optional)* | Per-source overrides keyed by source name (`github`, `gitlab`, `codeberg`, `apkfab`, `apkcombo`, etc.) supporting `enabled`, `url`, `repo`, `project`, `domain`, `keyword`, `slug`, `tag`, `asset_pattern`, and `version_regex`. |
| `source_priority` | `list<string>` *(optional)* | Per-app source order prepended before the global `apk_source_order`. |
| `strict_sources` | `boolean` *(optional)* | If `true`, restricts acquisition to sources explicitly configured for this app (default `false`). |
| `architectures` | `list<string>` | Supported ABIs (typically `[arm64-v8a]`, `[armeabi-v7a]`, `[x86]`, or `[x86_64]`). |
| `mode` | `string` | Build target mode: `full` (alias `patch`, default), `lite` (strips foreign ABIs & unused resources), `both` (non-root APK + root payload), `root` / `module` (root payload only), or `install` (unpatched stock APK). |
| `version` | `string` *(optional)* | Specific version pin (e.g. `14.43.0`) or `auto` to resolve the optimal version dynamically from patch metadata. |
| `pin_policy` | `string` *(optional)* | How to handle a pinned `version` unsupported by patch metadata: `prefer_supported` (default: resolve supported version), `strict` (fail preflight), or `force` (build pinned version anyway, marking `degraded` if patches auto-disable). |
| `version_pin_strict` | `boolean` *(optional)* | Legacy boolean alias for `pin_policy: strict`. |
| `min_version` / `max_version` | `string` *(optional)* | Inclusive version bounds enforced during version resolution. |
| `on_unknown_compat` | `string` *(optional)* | Policy when patch metadata compatibility is `Unknown` and version is `auto` (default `"fail"`). |
| `patches` / `included_patches` | `list<string>` *(optional)* | Explicit list of patches to apply. If omitted or empty, all applicable patches from the source bundle are used. |
| `excluded_patches` | `list<string>` *(optional)* | Explicit list of patches to exclude during patching. |
| `patcher_args` | `string` *(optional)* | Extra CLI arguments forwarded to `morphe-cli`. |
| `patches_version` | `string` *(optional)* | Pin a specific patch bundle release tag/version. |
| `expected_min_patches` | `integer` *(optional)* | Minimum number of patches that must be applied (build fails if fewer apply). |
| `expected_cert_sha256` | `string \| list<string>` *(optional)* | Expected APK signing certificate SHA-256 fingerprint(s) verified after download. |
| `timeout_secs` | `integer` *(optional)* | Per-app build time budget in seconds (overrides `build.app_timeout_secs`). |
| `allow_build_failure` | `boolean` *(optional)* | If `true`, failures for this app do not fail `build` or `report check` exit status (default `false`). |
| `name` / `display_name` | `string` *(optional)* | Clean application display name used for public artifact naming (e.g. `YouTube`, `Termius`). Defaults to title-cased app ID. |
| `description` | `string` *(optional)* | Human-readable app description (used in `apps --json` and catalogue metadata). |
| `bundle_priority` | `integer` *(optional)* | Ordering priority within bundles (default `100`). |
| `keystore` | `object` *(optional)* | Keystore configuration with `alias` (default `revanced`) and `file` path (default `keys/default.jks`) for signing. |
| `module` | `object` *(optional)* | Root module flags: `single` (emit individual flashable zip) and `bundle` (include in categorized bundles). |
| `dependencies` | `list<string> \| object` *(optional)* | Flat list (e.g. `[microg]`) or mode-aware object (`{ non_root: [microg], root: [] }`). |
| `post_patch_hooks` | `list<object>` *(optional)* | Hook scripts executed after patching (`script`, `when`: `root \| nonroot \| always`, `optional`). |

### Example

```yaml
apps:
  youtube:
    name: "YouTube"
    enabled: true
    package: com.google.android.youtube
    patch_source: anddea
    patch_branch: dev
    cli_source: MorpheApp/morphe-cli
    apkmirror_url: https://apkmirror.com/apk/google-inc/youtube
    uptodown_url: https://youtube.en.uptodown.com/android
    archive_url: https://archive.org/download/geologically/apks/com.google.android.youtube
    source_priority:
      - archive
      - apkmirror
      - uptodown
      - apkpure
    architectures:
      - arm64-v8a
      - x86_64
    mode: both
    expected_min_patches: 2
    excluded_patches:
      - "Custom branding"
    keystore:
      alias: revanced
      file: keys/youtube.jks
    module:
      single: true
      bundle: true
    dependencies:
      non_root:
        - microg
      root: []
    post_patch_hooks:
      - script: scripts/custom_hook.py
        when: root
        optional: true

  gboard:
    name: "Gboard"
    enabled: true
    package: com.google.android.inputmethod.latin
    patch_source: jasonwu
    cli_source: MorpheApp/morphe-cli
    sources:
      apkfab:
        slug: gboard-the-google-keyboard
      apkcombo:
        enabled: true
    architectures:
      - arm64-v8a
    mode: patch
    dependencies: []
```

---

## 2. Tools & Patch Sources (`config/sources.yaml`)

Configures upstream CLI patchers, patch bundle repositories, APK download providers, bypass/fetch services, source precedence, and circuit breaker fault tolerance.

### Specification

```yaml
# Patcher CLI tool definition
cli:
  repo: MorpheApp/morphe-desktop
  asset_pattern: "morphe-*-all.jar"
  provider: github                   # github (default) | gitlab | codeberg

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

# Fallback order when acquiring base APKs (app source_priority is prepended)
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

# Optional Cloudflare / anti-bot bypass fetcher settings
fetch:
  flaresolverr:
    enabled: auto                    # auto | true | false
    url: http://localhost:8191/v1
    timeout_secs: 60
  trawl:
    enabled: auto                    # auto | true | false
    url: ""                          # Or TRAWL_URL env var
  per_host_delay_ms: {}

# Circuit breaker parameters to isolate failing or rate-limited providers
# Note: 404 / not_found does not increment transport failure counters;
# 403 / ip_reputation_block opens the breaker immediately.
circuit_breakers:
  default:
    threshold: 3           # Consecutive transport failures before tripping
    cooldown_secs: 60      # Cooldown time before half-open probe
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

Controls JVM requirements, local workspace directories, worker concurrency, per-app timeouts, automated patch selection rules, keystore profiles, bundle limits, and smoke test parameters.

### Specification

```yaml
build:
  java_min_version: 17               # Code default if omitted: 21
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

  # Download resilience, timeouts, and fallback
  download_retries: 3
  download_timeout_secs: 120         # Code default if omitted: 180
  app_timeout_secs: 600              # Per-app budget (also REVANCEX_APP_TIMEOUT_SECS)
  version_fallback_attempts: 3
  tools_check_interval_hours: 6

  version_cache: ./config/version_cache.json
  asset_exclude_patterns:
    - "hw-"

  # Bundle packaging limits
  bundle:
    max_bytes: 1800000000
    include_stock_apks: true

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
      nonroot:
        exclude:
          - contains: "custom branding"
        include_if_dependency:
          microg:
            - contains: "gmscore support"
    # Optional regex patterns for extracting failing patch names on retry
    # failure_patterns:
    #   - 'Failed to apply patch\s+([A-Za-z0-9_-]+)'

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

Specifies the grouping of applications into named category Magisk, KernelSU, and APatch module zip files (`core`, `multimedia`, `productivity`, `social`, `tools`). `config/apps.yaml` is the single source of truth for per-app module eligibility (`enabled`, `module.bundle`, and `module.single`): the `all` bundle and single-app modules are derived directly from `config/apps.yaml`, and `revancex validate` is strictly read-only.

Each category bundle entry supports optional overrides: `name`, `module_id`, and `zip_name` (defaults to `revancex-bundle-<category>.zip`).

```yaml
modules:
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
```

---

## 5. Root Module Metadata (`config/module.yaml`)

Emitted into `module.prop` inside generated Magisk, KernelSU, and APatch module archives. Optional `prop` overrides (`id`, `name`, `author`, `description`, `update_json`) take precedence if provided.

```yaml
module:
  id: revancex
  name: "ReVanceX"
  author: "Thunderkex"
  description: "Patched apps bundle"
  updateJson: ""
  min_magisk: 20400
  min_kernelsu: 10200
```

---

## 6. Build Reporting (`BUILD_REPORT.json` & `BUILD_REPORT.md`)

Running `revancex build` always writes a structured JSON report (default `<output>/BUILD_REPORT.json`, or the path passed to `--report`) alongside a human-readable Markdown summary (`BUILD_REPORT.md` in the same directory). Secret query parameters (`token`, `k`, `secret`, `auth`, `signature`, `sig`) are automatically redacted from recorded URLs.

### Schema Overview (`2.1.0`)

```json
{
  "schema_version": "2.1.0",
  "build_timestamp": "2026-11-10T11:01:01Z",
  "revancex_version": "1.1.0",
  "summary": {
    "total": 31,
    "success": 29,
    "degraded": 1,
    "failed": 1,
    "skipped": 0,
    "warnings_total": 2,
    "failures_by_category": {
      "all_sources_exhausted": 1
    },
    "open_breakers": [],
    "skipped_unconfigured": ["github", "gitlab", "codeberg", "apkeep"]
  },
  "apps": {
    "youtube": {
      "status": "success",
      "requested_version": "20.05.46",
      "version_candidates_tried": ["20.05.46"],
      "version_downgraded": false,
      "target_version": "20.05.46",
      "patch_supported_versions": ["20.05.46"],
      "fetched_version": "20.05.46",
      "resolved_version": "20.05.46",
      "resolved_source": "apkmirror",
      "resolution_strategy": "patch_intersection",
      "patches_included": ["Seekbar tapping", "Spoof client"],
      "patches_auto_disabled_count": 0,
      "attempted_sources": [
        {
          "source": "apkmirror",
          "outcome": "hit",
          "version": "20.05.46",
          "reason": "downloaded valid apk",
          "duration_ms": 2340,
          "breaker_state_at_attempt": "closed",
          "breaker_final_state": "closed"
        }
      ],
      "arch_mode": "full",
      "output_file": "YouTube-20.05.46.apk",
      "sha256": "...",
      "cert_sha256": "...",
      "integrity": {
        "package_ok": true,
        "version_ok": true,
        "cert_ok": true
      },
      "total_duration_ms": 18420
    }
  }
}
```

### App Statuses & Failure Classifications

Per-app `status` values:
- `success`: Built and patched as configured.
- `degraded`: Built under `pin_policy: force` with auto-disabled patches or downgraded fallback.
- `failed`: Build failed (populates `failure_category` and `suggested_action`).
- `skipped` / `success_unpatched`: Skipped or installed unpatched (e.g. `install` mode).

ReVanceX automatically classifies build failures into `failure_category` and populates `suggested_action`:
- `all_sources_exhausted`: None of the configured download providers could supply the base APK.
- `version_not_supported`: No compatible version or pinned version is outside supported patch range under strict policy.
- `no_enabled_patches`: Patch metadata contains zero applicable enabled patches for the target package.
- `integrity_check_failed`: Downloaded file failed ZIP/`classes.dex`/`AndroidManifest.xml`, package/version, or `expected_cert_sha256` checks.
- `patcher_error`: `morphe-cli` failed to apply patches to the target APK bytecode (or fewer than `expected_min_patches` applied).
- `version_conflict`: Two or more enabled patches require mutually disjoint application versions.
- `compatibility_unknown`: Patch metadata compatibility is unknown and `on_unknown_compat` is `fail`.
- `arch_unsupported`: Downloaded APK does not contain native libraries compatible with the target architecture.
- `other`: Time budget exceeded or unclassified error.

### CI/CD Evaluation

Use `revancex report check` to evaluate reports and enforce quality gates:

```bash
# Exit non-zero only if failed/conflicted apps (excluding allow_build_failure) exceed 15%
revancex report check ./output/BUILD_REPORT.json --fail-threshold 15

# Warning mode: always exits 0 to allow releasing successful apps
revancex report check ./output/BUILD_REPORT.json --continue
```

## Build output & logs

Console output is intentionally compact: phases, one line per app, warnings (deduplicated) and a final summary table.

| Want | How |
| --- | --- |
| Detail on console (info/debug, tool output) | `revancex build --verbose` or `VERBOSE=1` / `RUST_LOG=debug` (on Actions: *Re-run with debug logging* → `RUNNER_DEBUG=1`) |
| Full detail after the fact | `build/logs/revancex.log` (everything), `build/logs/<app>.log` (per app: morphe-cli stdout/stderr, hooks), `build/logs/downloads.log` (aria2c). Override with `REVANCEX_LOG_DIR`. On Actions they are uploaded as the `build-logs-*` artifact. |
| ASCII markers (`[OK]/[FAIL]/[WARN]`), no colour | `NO_COLOR=1` or non-TTY output outside GitHub Actions (`REVANCEX_ASCII=1` forces ASCII) |

Exit codes and fail-fast behaviour are unchanged. Secrets (keystore password, tokens) are masked in console and log files.
