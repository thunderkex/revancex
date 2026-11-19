#!/usr/bin/env python3
"""Check Google Drive for MiXplorer + add-ons updates and build a systemless KernelSU module."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

DEFAULT_FOLDER_ID = "1BfeK39boriHy-9q76eXLLqbCwfV17-Gv"
USER_AGENT = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"
MANIFEST_RE = re.compile(r"<!--\s*mixplorer-manifest:\s*(\{.*?\})\s*-->", re.S)

ABI_TO_ISA = {
    "arm64-v8a": "arm64",
    "armeabi-v7a": "arm",
    "x86": "x86",
    "x86_64": "x86_64",
}


def http_get_text(url: str, headers: dict[str, str] | None = None, timeout: int = 30) -> str:
    req_headers = {"User-Agent": USER_AGENT}
    if headers:
        req_headers.update(headers)
    req = urllib.request.Request(url, headers=req_headers)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return resp.read().decode("utf-8", errors="ignore")


def list_drive_folder(folder_id: str) -> list[dict[str, str | bool]]:
    url = f"https://drive.google.com/embeddedfolderview?id={folder_id}#list"
    html = http_get_text(url, timeout=30)
    pattern = re.compile(
        r'id="entry-([^"]+)".*?href="([^"]+)".*?class="flip-entry-title">([^<]+)<',
        re.S,
    )
    entries = []
    for eid, href, name in pattern.findall(html):
        entries.append(
            {
                "id": eid.strip(),
                "name": name.strip(),
                "href": href.strip(),
                "is_folder": "/folders/" in href,
            }
        )
    return entries


def normalize_app_dir(folder_name: str) -> str:
    lower = folder_name.lower()
    if lower.startswith("mixplorer"):
        return "MiXplorer"
    if "archive" in lower:
        return "MiXArchive"
    if "encrypt" in lower:
        return "MiXEncrypt"
    if "image" in lower:
        return "MiXImage"
    if "pdf" in lower:
        return "MiXPDF"
    if "codec" in lower:
        return "MiXCodecs"
    if "tagger" in lower:
        return "MiXTagger"
    cleaned = re.sub(r"[^A-Za-z0-9]+", "", folder_name)
    return cleaned or "MiXAddon"


def extract_build_tag(filename: str) -> str:
    m = re.search(r"_(B\d+)", filename)
    return m.group(1) if m else filename


def extract_mixplorer_version(folder_name: str, filename: str) -> str:
    for text in (filename, folder_name):
        m = re.search(r"v(\d+(?:\.\d+)+)", text, re.I)
        if m:
            return m.group(1)
    return "0.0.0"


def compute_version_code(version_str: str, revision: int = 0) -> int:
    parts = [int(p) for p in re.findall(r"\d+", version_str)[:3]]
    while len(parts) < 3:
        parts.append(0)
    major, minor, patch = parts
    return major * 1_000_000 + minor * 10_000 + patch * 100 + revision


def select_apk_for_arch(files: list[dict[str, str | bool]], arch: str) -> dict[str, str | bool] | None:
    apks = [
        f
        for f in files
        if not f["is_folder"] and str(f["name"]).lower().endswith(".apk")
    ]
    if not apks:
        return None

    non_api29 = [f for f in apks if "-api29" not in str(f["name"]).lower()]
    pool = non_api29 or apks

    if arch == "arm64":
        for f in pool:
            if str(f["name"]).lower().endswith("-arm64.apk"):
                return f
        for f in pool:
            if "-universal.apk" in str(f["name"]).lower():
                return f
    elif arch == "universal":
        for f in pool:
            if "-universal.apk" in str(f["name"]).lower():
                return f

    arch_suffixes = ("-arm.apk", "-arm64.apk", "-x86.apk", "-x64.apk")
    for f in pool:
        lname = str(f["name"]).lower()
        if not any(lname.endswith(s) for s in arch_suffixes):
            return f

    return pool[0]


def discover_drive_apps(folder_id: str, arch: str) -> dict:
    root_entries = list_drive_folder(folder_id)
    subfolders = [e for e in root_entries if e["is_folder"]]
    if not subfolders:
        raise RuntimeError(f"No subfolders found in Google Drive folder {folder_id}")

    apps: list[dict[str, str]] = []
    mixplorer_version = "0.0.0"

    for sub in sorted(subfolders, key=lambda x: str(x["name"]).lower()):
        sub_name = str(sub["name"])
        sub_files = list_drive_folder(str(sub["id"]))
        chosen = select_apk_for_arch(sub_files, arch)
        if not chosen:
            continue
        app_dir = normalize_app_dir(sub_name)
        fname = str(chosen["name"])
        build_tag = extract_build_tag(fname)
        if app_dir == "MiXplorer":
            mixplorer_version = extract_mixplorer_version(sub_name, fname)
        apps.append(
            {
                "app_dir": app_dir,
                "folder": sub_name,
                "file_id": str(chosen["id"]),
                "filename": fname,
                "build": build_tag,
            }
        )

    apps.sort(key=lambda a: (0 if a["app_dir"] == "MiXplorer" else 1, a["app_dir"]))

    if not apps:
        raise RuntimeError(f"No APKs discovered in Google Drive folder {folder_id}")

    return {
        "folder_id": folder_id,
        "arch": arch,
        "mixplorer_version": mixplorer_version,
        "apps": apps,
    }


def fetch_latest_mixplorer_release(repo: str, token: str | None) -> dict | None:
    headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"

    url = f"https://api.github.com/repos/{repo}/releases?per_page=30"
    try:
        raw = http_get_text(url, headers=headers, timeout=30)
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return None
        raise
    releases = json.loads(raw)
    for rel in releases:
        tag = rel.get("tag_name", "")
        if tag.startswith("mixplorer-v") and not rel.get("draft", False):
            return rel
    return None


def parse_released_builds(release: dict | None) -> dict[str, str]:
    if not release:
        return {}
    body = release.get("body") or ""
    m = MANIFEST_RE.search(body)
    if m:
        try:
            data = json.loads(m.group(1))
            return {a["app_dir"]: a["build"] for a in data.get("apps", [])}
        except (json.JSONDecodeError, KeyError, TypeError):
            pass

    builds: dict[str, str] = {}
    for line in body.splitlines():
        if not line.startswith("|"):
            continue
        cols = [c.strip() for c in line.strip("|").split("|")]
        if len(cols) >= 2:
            name = re.sub(r"[*_`]", "", cols[0]).strip()
            bmatch = re.search(r"(B\d+)", cols[1])
            if bmatch:
                builds[normalize_app_dir(name)] = bmatch.group(1)
    return builds


def compute_changes(
    discovered: dict, latest_release: dict | None, force: bool
) -> dict:
    released_builds = parse_released_builds(latest_release)
    latest_tag = latest_release.get("tag_name", "") if latest_release else ""
    base_tag = f"mixplorer-v{discovered['mixplorer_version']}"

    changes: list[str] = []
    for app in discovered["apps"]:
        app_dir = app["app_dir"]
        cur_build = app["build"]
        prev_build = released_builds.get(app_dir)
        if prev_build != cur_build:
            changes.append(f"{app_dir}: {prev_build or 'none'} -> {cur_build}")

    has_updates = force or bool(changes) or not latest_release

    revision = 0
    if latest_tag == base_tag and changes and not force:
        revision = 1
        tag = f"{base_tag}-r{revision}"
    elif latest_tag.startswith(f"{base_tag}-r") and changes and not force:
        m = re.search(r"-r(\d+)$", latest_tag)
        revision = (int(m.group(1)) + 1) if m else 1
        tag = f"{base_tag}-r{revision}"
    elif latest_tag.startswith(f"{base_tag}-r") and force and not changes:
        tag = latest_tag
        m = re.search(r"-r(\d+)$", latest_tag)
        revision = int(m.group(1)) if m else 0
    else:
        tag = base_tag

    version_code = compute_version_code(discovered["mixplorer_version"], revision)

    return {
        "has_updates": has_updates,
        "tag": tag,
        "latest_tag": latest_tag,
        "mixplorer_version": discovered["mixplorer_version"],
        "version_code": version_code,
        "changes": changes,
        "released_builds": released_builds,
        "discovered": discovered,
    }


def write_github_output(values: dict[str, str]) -> None:
    gh_out = os.environ.get("GITHUB_OUTPUT")
    if not gh_out:
        return
    with open(gh_out, "a", encoding="utf-8") as f:
        for k, v in values.items():
            f.write(f"{k}={v}\n")


def write_check_summary(result: dict) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return
    disc = result["discovered"]
    released = result["released_builds"]
    status = "Update detected" if result["has_updates"] else "Up to date"
    lines = [
        f"### MiXplorer Suite Check — {status}",
        "",
        f"- **Target tag**: `{result['tag']}` (latest released: `{result['latest_tag'] or 'none'}`)",
        f"- **Architecture**: `{disc['arch']}`",
        "",
        "| App | Drive Build | Released Build | Status |",
        "| --- | --- | --- | --- |",
    ]
    for app in disc["apps"]:
        d_b = app["build"]
        r_b = released.get(app["app_dir"], "—")
        st = "Updated" if d_b != r_b else "Same"
        lines.append(f"| **{app['app_dir']}** | `{d_b}` | `{r_b}` | {st} |")
    lines.append("")
    with open(summary_path, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def download_drive_file(file_id: str, dest_path: Path) -> None:
    url = f"https://drive.usercontent.google.com/download?id={file_id}&export=download&confirm=t"
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    dest_path.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(req, timeout=180) as resp, open(dest_path, "wb") as out:
        while True:
            chunk = resp.read(256 * 1024)
            if not chunk:
                break
            out.write(chunk)
    if not zipfile.is_zipfile(dest_path):
        raise RuntimeError(f"Downloaded file is not a valid APK/ZIP: {dest_path.name}")


def build_module_zip(
    discovered: dict,
    apk_paths: dict[str, Path],
    out_zip: Path,
    version_code: int,
    tag: str,
) -> dict[str, int]:
    arch = discovered["arch"]
    mix_ver = discovered["mixplorer_version"]
    out_zip.parent.mkdir(parents=True, exist_ok=True)

    module_prop = (
        "id=mixplorer_suite\n"
        f"name=MiXplorer + Add-ons Suite ({arch})\n"
        f"version=v{mix_ver}\n"
        f"versionCode={version_code}\n"
        "author=thunderkex\n"
        f"description=Systemless KernelSU/Magisk module bundling MiXplorer v{mix_ver} and all official add-ons ({arch}).\n"
        "minMagisk=24000\n"
        "minKernelSU=11000\n"
    )

    update_binary = (
        "#!/sbin/sh\n"
        'OUTFD="$2"\n\n'
        "ui_print() {\n"
        '    if [ -n "$OUTFD" ] && [ -e "/proc/self/fd/$OUTFD" ]; then\n'
        "        printf 'ui_print %s\\nui_print\\n' \"$1\" > \"/proc/self/fd/$OUTFD\"\n"
        "    else\n"
        '        echo "$1"\n'
        "    fi\n"
        "}\n\n"
        'ui_print "! Flash this module inside KernelSU, KernelSU Next, APatch, or Magisk."\n'
        "exit 1\n"
    )

    updater_script = "#MAGISK\n"

    arch_guard = ""
    if arch == "arm64":
        arch_guard = (
            'if [ "$ARCH" != "arm64" ]; then\n'
            '    abort "! Unsupported architecture: $ARCH (this module is built for arm64-v8a)."\n'
            "fi\n\n"
        )

    app_lines = "\n".join(
        f'ui_print "  * {a["app_dir"]} ({a["build"]})"' for a in discovered["apps"]
    )
    customize_sh = (
        "#!/system/bin/sh\n"
        "# MiXplorer + Add-ons Suite — Systemless KernelSU / Magisk Module\n\n"
        'ui_print "******************************************"\n'
        f'ui_print "*    MiXplorer + Add-ons Suite ({arch})   *"\n'
        'ui_print "*  Systemless KernelSU / Magisk Module   *"\n'
        'ui_print "******************************************"\n\n'
        f"{arch_guard}"
        f'ui_print "- Bundled apps (v{mix_ver}, {tag}):"\n'
        f"{app_lines}\n\n"
        'ui_print "- Setting permissions..."\n'
        'set_perm_recursive "$MODPATH/system" 0 0 0755 0644\n\n'
        "if [ -d /data/system/package_cache ]; then\n"
        '    ui_print "- Clearing Android package cache..."\n'
        "    rm -rf /data/system/package_cache/*\n"
        "fi\n\n"
        'ui_print "- Done! Reboot to activate MiXplorer and all add-ons."\n'
    )

    uninstall_sh = (
        "#!/system/bin/sh\n"
        "rm -rf /data/system/package_cache/* 2>/dev/null || :\n"
    )

    lib_counts: dict[str, int] = {}

    with zipfile.ZipFile(out_zip, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as out:
        out.writestr("META-INF/com/google/android/update-binary", update_binary.encode("utf-8"))
        out.writestr("META-INF/com/google/android/updater-script", updater_script.encode("utf-8"))
        out.writestr("module.prop", module_prop.encode("utf-8"))
        out.writestr("customize.sh", customize_sh.encode("utf-8"))
        out.writestr("uninstall.sh", uninstall_sh.encode("utf-8"))

        for app in discovered["apps"]:
            app_dir = app["app_dir"]
            apk_path = apk_paths[app_dir]
            out.write(
                apk_path,
                f"system/app/{app_dir}/{app_dir}.apk",
                compress_type=zipfile.ZIP_STORED,
            )

            count = 0
            with zipfile.ZipFile(apk_path, "r") as apk_zf:
                for entry in apk_zf.namelist():
                    if not (entry.startswith("lib/") and entry.endswith(".so")):
                        continue
                    parts = entry.split("/")
                    if len(parts) != 3:
                        continue
                    abi, so_name = parts[1], parts[2]
                    if arch == "arm64" and abi != "arm64-v8a":
                        continue
                    isa = ABI_TO_ISA.get(abi)
                    if not isa:
                        continue
                    out.writestr(
                        f"system/app/{app_dir}/lib/{isa}/{so_name}",
                        apk_zf.read(entry),
                        compress_type=zipfile.ZIP_DEFLATED,
                    )
                    count += 1
            lib_counts[app_dir] = count

    with zipfile.ZipFile(out_zip, "r") as zf:
        bad = zf.testzip()
        if bad is not None:
            raise RuntimeError(f"Corrupt zip entry detected: {bad}")
        for script in (
            "module.prop",
            "customize.sh",
            "uninstall.sh",
            "META-INF/com/google/android/update-binary",
            "META-INF/com/google/android/updater-script",
        ):
            if b"\r" in zf.read(script):
                raise RuntimeError(f"CRLF line ending found in {script}")

    return lib_counts


def render_release_notes(
    discovered: dict,
    out_zip: Path,
    lib_counts: dict[str, int],
    tag: str,
    version_code: int,
) -> str:
    sha256 = hashlib.sha256(out_zip.read_bytes()).hexdigest()
    size_mib = out_zip.stat().st_size / (1024 * 1024)
    mix_ver = discovered["mixplorer_version"]
    arch = discovered["arch"]
    arch_label = "arm64-v8a" if arch == "arm64" else "universal"

    rows = []
    for app in discovered["apps"]:
        is_univ = "-universal" in app["filename"].lower()
        row_arch = "universal" if is_univ else arch_label
        ver_col = (
            f"`v{mix_ver} ({app['build']})`"
            if app["app_dir"] == "MiXplorer"
            else f"`{app['build']}`"
        )
        rows.append(
            f"| **{app['app_dir']}** | {ver_col} | `{row_arch}` | {lib_counts.get(app['app_dir'], 0)} |"
        )

    manifest_payload = json.dumps(
        {
            "tag": tag,
            "arch": arch,
            "mixplorer_version": mix_ver,
            "version_code": version_code,
            "built_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "apps": [
                {
                    "app_dir": a["app_dir"],
                    "filename": a["filename"],
                    "build": a["build"],
                    "file_id": a["file_id"],
                }
                for a in discovered["apps"]
            ],
        },
        separators=(",", ":"),
    )

    return "\n".join(
        [
            f"Systemless KernelSU / Magisk module bundling **MiXplorer v{mix_ver}** and all official add-ons for `{arch_label}` devices.",
            "",
            "### Included Apps",
            "| App | APK Build | Architecture | Native `.so` libs |",
            "| --- | --- | --- | --- |",
            *rows,
            "",
            "### Details",
            "- **Install method**: Systemless `/system/app` overlay with native libraries extracted to `lib/<isa>/`",
            f"- **Compatibility**: KernelSU, KernelSU Next, APatch, Magisk (`{arch_label}`)",
            f"- **Size**: `{size_mib:.2f} MiB`",
            f"- **SHA-256**: `{sha256}`",
            "",
            f"<!-- mixplorer-manifest: {manifest_payload} -->",
            "",
        ]
    )


def cmd_check(args: argparse.Namespace) -> int:
    discovered = discover_drive_apps(args.folder_id, args.arch)
    latest_release = None
    if args.repo:
        latest_release = fetch_latest_mixplorer_release(
            args.repo, os.environ.get("GITHUB_TOKEN")
        )
    result = compute_changes(discovered, latest_release, args.force)

    write_github_output(
        {
            "has_updates": "true" if result["has_updates"] else "false",
            "tag": result["tag"],
            "latest_tag": result["latest_tag"],
            "mixplorer_version": result["mixplorer_version"],
            "version_code": str(result["version_code"]),
            "changes_summary": "; ".join(result["changes"]) or "no changes",
        }
    )
    write_check_summary(result)

    print(
        json.dumps(
            {
                "has_updates": result["has_updates"],
                "tag": result["tag"],
                "latest_tag": result["latest_tag"],
                "mixplorer_version": result["mixplorer_version"],
                "version_code": result["version_code"],
                "changes": result["changes"],
                "apps": discovered["apps"],
            },
            indent=2,
        )
    )
    return 0


def cmd_build(args: argparse.Namespace) -> int:
    discovered = discover_drive_apps(args.folder_id, args.arch)
    mix_ver = discovered["mixplorer_version"]
    tag = args.tag or f"mixplorer-v{mix_ver}"
    version_code = args.version_code or compute_version_code(mix_ver)

    work_dir = Path(args.work_dir)
    out_dir = Path(args.output_dir)
    zip_name = f"MiXplorer-Suite-v{mix_ver}-{args.arch}-KSU.zip"
    out_zip = out_dir / zip_name

    apk_paths: dict[str, Path] = {}
    for app in discovered["apps"]:
        dest = work_dir / app["filename"]
        print(f"Downloading {app['filename']}...")
        download_drive_file(app["file_id"], dest)
        apk_paths[app["app_dir"]] = dest

    lib_counts = build_module_zip(discovered, apk_paths, out_zip, version_code, tag)
    notes = render_release_notes(discovered, out_zip, lib_counts, tag, version_code)

    notes_path = out_dir / "mixplorer-release-notes.md"
    notes_path.write_text(notes, encoding="utf-8")

    sha256 = hashlib.sha256(out_zip.read_bytes()).hexdigest()
    print(f"Built module: {out_zip} ({out_zip.stat().st_size:,} bytes)")
    print(f"SHA-256     : {sha256}")

    write_github_output(
        {
            "zip_path": str(out_zip),
            "zip_name": zip_name,
            "notes_path": str(notes_path),
            "sha256": sha256,
            "tag": tag,
            "mixplorer_version": mix_ver,
        }
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    p_check = sub.add_parser("check", help="Check Google Drive folder for updates")
    p_check.add_argument("--folder-id", default=DEFAULT_FOLDER_ID)
    p_check.add_argument("--arch", choices=("arm64", "universal"), default="arm64")
    p_check.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    p_check.add_argument("--force", action="store_true")
    p_check.set_defaults(func=cmd_check)

    p_build = sub.add_parser("build", help="Download APKs and build KernelSU module zip")
    p_build.add_argument("--folder-id", default=DEFAULT_FOLDER_ID)
    p_build.add_argument("--arch", choices=("arm64", "universal"), default="arm64")
    p_build.add_argument("--tag", default="")
    p_build.add_argument("--version-code", type=int, default=0)
    p_build.add_argument("--work-dir", default="tmp/mixplorer-ksu/apks")
    p_build.add_argument("--output-dir", default="output")
    p_build.set_defaults(func=cmd_build)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
