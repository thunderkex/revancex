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
    "stock_apps": {},
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
        with open(os.path.join(config_dir, "modules.yaml"), "w") as f:
            f.write(FIXTURE_MODULES_YAML)
        with open(os.path.join(config_dir, "releases_registry.json"), "w") as f:
            json.dump(FIXTURE_REGISTRY, f)
        with open(os.path.join(tmp, "README.md"), "w") as f:
            f.write(FIXTURE_README)

        # One fake APK so the script has something to process
        open(os.path.join(output_dir, "youtube-patched.apk"), "w").close()

        # Copy the script into the tmp scripts/ dir so relative paths resolve
        dest_script = os.path.join(scripts_dir, "update_readme_and_notify.py")
        shutil.copy(SCRIPT, dest_script)

        env = os.environ.copy()
        env.update({
            "GITHUB_REPOSITORY": "test/revancex",
            "GITHUB_RUN_NUMBER": "1",
            "GITHUB_RUN_ID": "1",
            "RELEASE_TAG": "test-v0.0.1",
            "OUTPUT_DIR": output_dir,
            # No Telegram secrets — should skip silently
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
        assert "youtube" in readme.lower(), "Expected 'youtube' in README table"
        assert "old content" not in readme, "Old content not replaced"

        print("PASS: update_readme_and_notify.py smoke test OK")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    run()
