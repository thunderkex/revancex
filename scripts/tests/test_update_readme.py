"""
Offline smoke test for scripts/update_readme_and_notify.py.
Run: python scripts/tests/test_update_readme.py
Exit 0 = pass, non-zero = fail.
"""
import os
import sys
import json
import shutil
import tempfile
import subprocess

SCRIPT = os.path.join(os.path.dirname(__file__), "..", "update_readme_and_notify.py")
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

FIXTURE_APPS_YAML = """\
apps:
  youtube:
    name: YouTube
    enabled: true
    package: com.google.android.youtube
    module:
      single: true
  strava:
    name: Strava
    enabled: true
    package: com.strava
    module:
      single: false
      bundle: true
"""

FIXTURE_MODULES_YAML = """\
modules:
  core:
    apps:
      - youtube
      - microg
"""

FIXTURE_README = """\
# ReVanceX

<!-- AUTO-APP-LIST-START -->
old content
<!-- AUTO-APP-LIST-END -->
"""

FIXTURE_REGISTRY = {
    "stock_apps": {
        "soundcloud": {
            "filename": "soundcloud-patched.apk",
            "url": "https://example.com/soundcloud.apk",
            "tag": "auto-1",
            "updated_at": "2026-01-01 00:00:00 UTC"
        },
        "strava": {
            "filename": "strava-patched.apk",
            "url": "#strava-patched.apk",
            "tag": "auto-0",
            "updated_at": "2026-01-01 00:00:00 UTC"
        }
    },
    "root_apps": {},
    "single_modules": {},
    "bundles": {},
    "custom_bundles": {}
}


def run():
    tmp = tempfile.mkdtemp(prefix="revancex_test_")
    try:
        # Minimal repo layout the script expects
        config_dir = os.path.join(tmp, "config")
        scripts_dir = os.path.join(tmp, "scripts")
        output_dir = os.path.join(tmp, "output")
        os.makedirs(config_dir)
        os.makedirs(scripts_dir)
        os.makedirs(output_dir)
        os.makedirs(os.path.join(tmp, "tmp"))

        # Fixture files
        with open(os.path.join(config_dir, "apps.yaml"), "w") as f:
            f.write(FIXTURE_APPS_YAML)
        with open(os.path.join(config_dir, "modules.yaml"), "w") as f:
            f.write(FIXTURE_MODULES_YAML)
        with open(os.path.join(config_dir, "releases_registry.json"), "w") as f:
            json.dump(FIXTURE_REGISTRY, f)
        with open(os.path.join(tmp, "README.md"), "w") as f:
            f.write(FIXTURE_README)

        open(os.path.join(output_dir, "YouTube-19.16.39.apk"), "w").close()
        open(os.path.join(output_dir, "YouTube-19.16.39-module.zip"), "w").close()

        dest_script = os.path.join(scripts_dir, "update_readme_and_notify.py")
        shutil.copy(SCRIPT, dest_script)
        shutil.copy(os.path.join(os.path.dirname(SCRIPT), "ulog.py"), os.path.join(scripts_dir, "ulog.py"))

        env = os.environ.copy()
        env.update({
            "GITHUB_REPOSITORY": "test/revancex",
            "GITHUB_RUN_NUMBER": "1",
            "GITHUB_RUN_ID": "1",
            "RELEASE_TAG": "test-v0.0.1",
            "OUTPUT_DIR": output_dir,
            "TG_TOKEN": "",
            "TG_CHAT": "",
        })

        result = subprocess.run(
            [sys.executable, dest_script],
            cwd=tmp,
            env=env,
            capture_output=True,
            text=True,
        )

        if result.returncode != 0:
            print("FAIL: script exited non-zero")
            print(result.stdout)
            print(result.stderr)
            sys.exit(1)

        readme = open(os.path.join(tmp, "README.md")).read()
        assert "<!-- AUTO-APP-LIST-START -->" in readme, "Missing START marker"
        assert "<!-- AUTO-APP-LIST-END -->" in readme, "Missing END marker"
        assert "| App | Patched APK | Single Root Module | Last Updated |" in readme, "Missing Patched APK column header"
        assert "**YouTube**" in readme or "**Youtube**" in readme, "Expected '**YouTube**' in README table"
        assert "YouTube-19.16.39.apk" in readme, "Expected YouTube-19.16.39.apk link in README"
        assert "YouTube-19.16.39-module.zip" in readme, "Expected YouTube-19.16.39-module.zip link in README"
        assert "old content" not in readme, "Old content not replaced"
        assert "soundcloud" not in readme, "Stale app should be pruned"
        assert "**Strava** | - | - | -" in readme, "Dead #anchor entry should render '-'"
        assert "#strava-patched.apk" not in readme, "Dead #anchor link must not appear in README"

        reg = json.load(open(os.path.join(config_dir, "releases_registry.json")))
        assert "soundcloud" not in reg.get("stock_apps", {}), "Expected soundcloud pruned from stock_apps"
        assert "youtube" in reg.get("stock_apps", {}), "Expected youtube in stock_apps registry"
        assert "sha256" in reg["stock_apps"]["youtube"], "Expected sha256 in stock_apps entry"
        assert "size_bytes" in reg["stock_apps"]["youtube"], "Expected size_bytes in stock_apps entry"
        assert reg["stock_apps"]["youtube"]["updated_at"].endswith("Z"), "Expected ISO-8601 UTC updated_at"
        assert "root_apps" not in reg, "root_apps should not be present in releases_registry.json"

        # Verify corrupt releases_registry.json fails hard instead of resetting to empty
        with open(os.path.join(config_dir, "releases_registry.json"), "w") as f:
            f.write("{corrupt json")
        corrupt_res = subprocess.run(
            [sys.executable, dest_script],
            cwd=tmp,
            env=env,
            capture_output=True,
            text=True,
        )
        assert corrupt_res.returncode != 0, "Expected non-zero exit on corrupt releases_registry.json"

        print("PASS: update_readme_and_notify.py smoke test OK")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    run()
