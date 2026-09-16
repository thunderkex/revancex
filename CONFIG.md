# ReVanceX Configuration Guide

This document describes the schema and options available across the ReVanceX YAML configuration files located in `./config/`.

---

## 1. `config/apps.yaml`

Defines all target applications and their patching rules.

```yaml
apps:
  youtube:
    enabled: true
    package: com.google.android.youtube
    patch_source: anddea               # references a key in sources.yaml
    display_name: "YouTube"
    bundle_priority: 1                 # lower = packed first in bundles
    mode: both                         # full | lite | both | install | module
    architectures:
      - arm64-v8a
    version: auto                      # or specific version e.g. 19.16.39
    included_patches: []
    excluded_patches:
      - "Custom branding"
    post_patch_hooks:
      - script: scripts/fix_root_youtube.py
        when: root                     # root | nonroot | always
        optional: false
    dependencies:
      non_root:
        - microg
      root: []
```

---

## 2. `config/sources.yaml`

Defines tool repositories and patch bundle sources.

```yaml
cli:
  repo: MorpheApp/morphe-desktop
  asset_pattern: "morphe-*-all.jar"

patch_sources:
  anddea:
    repo: anddea/revanced-patches
    branch: dev

apk_sources:
  apkmirror:
    base_url: https://www.apkmirror.com
```

---

## 3. `config/build.yaml`

Build pipeline, JVM, and patcher parameters.

```yaml
build:
  java_min_version: 17
  tools_dir: ./tools
  output_dir: ./output
  keys_dir: ./keys
  temp_dir: ./tmp
  workers: 4
  download_retries: 3
  download_timeout_secs: 120
  version_cache: ./tmp/version_cache.json

  bundle:
    max_bytes: 1800000000
    include_stock_apks: true

  keystore:
    validity_days: 10000
    key_algorithm: RSA
    key_size: 4096
    sig_algorithm: SHA256withRSA
    distinguished_name: "CN=Revanced Extended, OU=Builds, O=Thunderkex, C=XX"

  patcher:
    max_patch_retries: 2
    auto_disable: incompatible         # off | incompatible | aggressive
    rules:
      root:
        exclude:
          - contains: "custom branding"
          - contains: "change package name"
          - contains: "gmscore"
      nonroot:
        exclude:
          - contains: "custom branding"
        include_if_dependency:
          microg:
            - contains: "gmscore support"
```

---

## 4. `config/modules.yaml`

Magisk/KernelSU bundle module definitions.

```yaml
modules:
  core:
    name: "ReVanceX Core Bundle"
    module_id: "revancex"
    zip_name: "revancex-bundle.zip"
    apps:
      - youtube
      - youtube_music
      - microg
```

---

## 5. `config/module.yaml`

Root module metadata emitted into `module.prop`.

```yaml
module:
  id: revancex
  name: ReVanceX
  version: "1.0"
  author: Thunderkex
  description: "ReVanceX Patched Apps Module"
  min_magisk: 20400
  min_kernelsu: 10200
  update_json: ""
```
