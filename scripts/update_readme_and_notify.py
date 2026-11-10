import os
import sys
import re
import json
import hashlib
import zipfile
import urllib.request
import urllib.parse
from datetime import datetime, timezone

import ulog

try:
    import yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False

README_PATH = "README.md"
RELEASES_REGISTRY_PATH = "config/releases_registry.json"
START_MARKER = "<!-- AUTO-APP-LIST-START -->"
END_MARKER = "<!-- AUTO-APP-LIST-END -->"
PATCH_SOURCES_START = "<!-- AUTO-PATCH-SOURCES-START -->"
PATCH_SOURCES_END = "<!-- AUTO-PATCH-SOURCES-END -->"
BUNDLE_BREAKDOWN_START = "<!-- AUTO-BUNDLE-BREAKDOWN-START -->"
BUNDLE_BREAKDOWN_END = "<!-- AUTO-BUNDLE-BREAKDOWN-END -->"

def _replace_block(content, start_marker, end_marker, new_inner):
    pattern = re.compile(
        f"{re.escape(start_marker)}.*?{re.escape(end_marker)}", re.DOTALL
    )
    replacement = f"{start_marker}\n{new_inner}\n{end_marker}"
    if pattern.search(content):
        return pattern.sub(replacement, content)
    return content


def build_patch_sources_table(sources_yaml_path):
    if not (_YAML_AVAILABLE and os.path.exists(sources_yaml_path)):
        return None
    try:
        with open(sources_yaml_path, "r", encoding="utf-8") as f:
            y = yaml.safe_load(f)
    except Exception as e:
        ulog.warn(f"sources.yaml: Failed to load: {e}")
        return None
    patch_sources = (y or {}).get("patch_sources", {})
    if not patch_sources:
        return None
    rows = [
        "| ID | Repository | Asset Pattern |",
        "| :--- | :--- | :--- |",
    ]
    for sid, sdata in patch_sources.items():
        repo = sdata.get("repo", "")
        pattern = sdata.get("asset_pattern", "")
        rows.append(f"| `{sid}` | [{repo}](https://github.com/{repo}) | `{pattern}` |")
    return "\n".join(rows)


def build_bundle_breakdown_table(modules_yaml_path):
    if not (_YAML_AVAILABLE and os.path.exists(modules_yaml_path)):
        return None
    try:
        with open(modules_yaml_path, "r", encoding="utf-8") as f:
            y = yaml.safe_load(f)
    except Exception as e:
        ulog.warn(f"modules.yaml: Failed to load for breakdown: {e}")
        return None
    modules = (y or {}).get("modules", {})
    if not modules:
        return None
    rows = [
        "| Bundle | Zip File | Apps | Included Applications |",
        "| :--- | :--- | :---: | :--- |",
    ]
    for key, data in modules.items():
        apps = data.get("apps") or []
        zip_name = data.get("zip_name") or (
            "revancex-bundle.zip" if key == "core" else f"revancex-bundle-{key}.zip"
        )
        bundle_name = "Core Essentials" if key == "core" else key.replace("-", " ").title()
        included = ", ".join(a.replace("_", " ").title() for a in apps)
        rows.append(f"| **{bundle_name}** | `{zip_name}` | {len(apps)} | {included} |")
    return "\n".join(rows)


def send_telegram_message(token, chat_id, message, topic_id=None):
    if not token or not chat_id:
        ulog.skip("Telegram: Bot token or chat ID missing. Skipping notification.")
        return
    
    if not topic_id:
        for sep in [":", "/", "#"]:
            if sep in str(chat_id):
                parts = str(chat_id).split(sep, 1)
                if parts[1].strip().isdigit():
                    chat_id = parts[0].strip()
                    topic_id = parts[1].strip()
                    break

    url = f"https://api.telegram.org/bot{token}/sendMessage"
    payload = {
        "chat_id": chat_id,
        "text": message,
        "parse_mode": "HTML",
        "disable_web_page_preview": False
    }
    if topic_id:
        try:
            payload["message_thread_id"] = int(topic_id)
            ulog.debug(f"Telegram: Sending message to topic/thread ID: {payload['message_thread_id']}")
        except (ValueError, TypeError):
            ulog.warn(f"Telegram: Warning: Invalid topic ID: {topic_id}")

    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "User-Agent": "ReVanceX-Updater"}
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as res:
            if res.status == 200:
                ulog.ok("Telegram: Notification sent successfully.")
            else:
                ulog.warn(f"Telegram: Failed with status {res.status}")
    except Exception as e:
        ulog.warn(f"Telegram: Error sending message: {e}")

def main():
    repo_slug = os.environ.get("GITHUB_REPOSITORY", "")
    run_number = os.environ.get("GITHUB_RUN_NUMBER", "0")
    run_id = os.environ.get("GITHUB_RUN_ID", "0")
    tag_name = os.environ.get("RELEASE_TAG", f"auto-{run_number}")
    output_dir = os.environ.get("OUTPUT_DIR", "output")
    telegram_token = os.environ.get("TG_TOKEN") or os.environ.get("TELEGRAM_BOT_TOKEN", "")
    telegram_chat_id = os.environ.get("TG_CHAT") or os.environ.get("TELEGRAM_CHAT_ID", "")
    telegram_topic_id = (
        os.environ.get("TG_TOPIC")
        or os.environ.get("TG_THREAD_ID")
        or os.environ.get("TELEGRAM_TOPIC_ID")
        or os.environ.get("TELEGRAM_THREAD_ID", "")
    )

    os.makedirs("tmp", exist_ok=True)
    registry = {
        "stock_apps": {},
        "single_modules": {},
        "bundles": {},
        "custom_bundles": {}
    }
    if os.path.exists(RELEASES_REGISTRY_PATH):
        try:
            with open(RELEASES_REGISTRY_PATH, "r", encoding="utf-8") as f:
                loaded = json.load(f)
            for key, val in loaded.items():
                if key != "root_apps":
                    registry[key] = val
        except Exception:
            pass

    apps_yaml_path = os.path.join(os.path.dirname(__file__), "..", "config", "apps.yaml")
    apps_cfg = {}
    if _YAML_AVAILABLE and os.path.exists(apps_yaml_path):
        try:
            with open(apps_yaml_path, "r", encoding="utf-8") as f:
                y = yaml.safe_load(f)
                if y and "apps" in y:
                    apps_cfg = y["apps"]
        except Exception as e:
            ulog.warn(f"apps.yaml: Failed to load: {e}")
    elif os.path.exists(apps_yaml_path):
        try:
            with open(apps_yaml_path, "r", encoding="utf-8") as f:
                current_app = None
                for line in f:
                    m_app = re.match(r"^  ([a-zA-Z0-9_-]+):\s*$", line)
                    if m_app:
                        current_app = m_app.group(1)
                        apps_cfg[current_app] = {}
                    elif current_app:
                        m_name = re.match(r"^\s+name:\s*[\"']?([^\"'\n\r]+)[\"']?\s*$", line)
                        if m_name:
                            apps_cfg[current_app]["name"] = m_name.group(1).strip()
        except Exception as e:
            ulog.warn(f"apps.yaml: Failed fallback parse: {e}")

    def _resolve_app_key(filename, is_module=False):
        stem = filename[:-4]
        if is_module:
            clean = (
                stem.replace("revancex-module-", "")
                .replace("revanced-module-", "")
                .replace("-module", "")
                .replace("revancex-", "")
                .replace("revanced-", "")
            )
        else:
            clean = stem.replace("-patched", "")
        if clean in apps_cfg:
            return clean
        for app_id, app_data in apps_cfg.items():
            if not isinstance(app_data, dict):
                continue
            app_name = app_data.get("name") or app_data.get("display_name") or app_id.replace("_", " ").title()
            sanitized = re.sub(r"[^a-zA-Z0-9]+", "-", app_name).strip("-")
            if clean == app_id or clean == sanitized or clean.lower() == app_id.lower():
                return app_id

        candidates = []
        for app_id, app_data in apps_cfg.items():
            if not isinstance(app_data, dict):
                continue
            app_name = app_data.get("name") or app_data.get("display_name") or app_id.replace("_", " ").title()
            sanitized = re.sub(r"[^a-zA-Z0-9]+", "-", app_name).strip("-")
            for prefix in [f"{sanitized}-", f"{app_id}-"]:
                if stem.startswith(prefix) or clean.startswith(prefix):
                    candidates.append((len(prefix), app_id))
        if candidates:
            candidates.sort(key=lambda c: c[0], reverse=True)
            return candidates[0][1]
        # Fallback: if filename is like Name-1.2.3, strip version suffix
        m = re.match(r"^([a-zA-Z0-9_\s]+?)-v?\d", clean)
        if m:
            prefix = m.group(1).strip("-")
            prefix_lower = prefix.lower().replace("-", "_").replace(" ", "_")
            if prefix_lower in apps_cfg:
                return prefix_lower
            return prefix_lower
        return clean.lower()

    now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    new_files = []
    
    if os.path.exists(output_dir):
        new_files = sorted(os.listdir(output_dir))

    build_report = {}
    build_report_path = os.path.join(output_dir, "BUILD_REPORT.json")
    if os.path.exists(build_report_path):
        try:
            with open(build_report_path, "r", encoding="utf-8") as f:
                build_report = json.load(f)
        except Exception:
            build_report = {}

    test_report = {}
    test_report_path = os.path.join(output_dir, "TEST_REPORT.json")
    if os.path.exists(test_report_path):
        try:
            with open(test_report_path, "r", encoding="utf-8") as f:
                test_report = json.load(f)
        except Exception:
            test_report = {}

    test_by_app = {
        it.get("app"): it for it in (test_report.get("items") or []) if isinstance(it, dict) and it.get("app")
    }
    report_apps = build_report.get("apps", {}) if isinstance(build_report, dict) else {}

    def _file_sha256_and_size(path):
        h = hashlib.sha256()
        size = 0
        with open(path, "rb") as fp:
            while True:
                chunk = fp.read(65536)
                if not chunk:
                    break
                h.update(chunk)
                size += len(chunk)
        return h.hexdigest(), size

    def _apk_archs(path):
        found = set()
        has_native = False
        try:
            with zipfile.ZipFile(path, "r") as zf:
                for n in zf.namelist():
                    if n.startswith("lib/") and n.endswith(".so"):
                        has_native = True
                        parts = n.split("/")
                        if len(parts) >= 2 and parts[1] in ("arm64-v8a", "armeabi-v7a", "x86_64"):
                            found.add(parts[1])
        except Exception:
            return None
        if not has_native:
            return ["all"]
        return [a for a in ("arm64-v8a", "armeabi-v7a", "x86_64") if a in found] or None

    def _validation_for_app(app_key):
        ti = test_by_app.get(app_key)
        rep = report_apps.get(app_key, {}) if isinstance(report_apps, dict) else {}
        if ti:
            st = ti.get("status")
            if st == "passed":
                if rep.get("status") in ("degraded", "success_unpatched") or rep.get("warnings"):
                    return "warning"
                return "passed"
            if st == "failed":
                return "failed"
            return "unknown"
        if rep:
            if rep.get("validation_status"):
                return rep["validation_status"]
            st = rep.get("status")
            if st == "success":
                return "passed"
            if st in ("degraded", "success_unpatched"):
                return "warning"
            if st == "failed":
                return "failed"
        return "unknown"

    def _inspect_zip_payload(zip_path):
        items = []
        version_code = None
        try:
            with zipfile.ZipFile(zip_path, "r") as zf:
                if "module.prop" in zf.namelist():
                    prop_text = zf.read("module.prop").decode("utf-8", errors="replace")
                    for line in prop_text.splitlines():
                        if line.startswith("versionCode="):
                            raw_vc = line.split("=", 1)[1].strip()
                            if raw_vc.isdigit():
                                version_code = int(raw_vc)
                for n in sorted(zf.namelist()):
                    if n.startswith("apks/") and n.endswith(".apk"):
                        stem = os.path.basename(n)[:-4]
                        app_id = _resolve_app_key(stem + ".apk", is_module=False)
                        pkg = apps_cfg.get(app_id, {}).get("package", "") if isinstance(apps_cfg.get(app_id), dict) else ""
                        rep = report_apps.get(app_id, {}) if isinstance(report_apps, dict) else {}
                        ver = rep.get("resolved_version") or rep.get("fetched_version")
                        items.append({"app": app_id, "package": pkg, "version": ver, "path": n})
        except Exception:
            return None, None
        return items, version_code

    for fname in new_files:
        fpath = os.path.join(output_dir, fname)
        if not os.path.isfile(fpath):
            continue
        
        url = f"https://github.com/{repo_slug}/releases/download/{tag_name}/{fname}" if repo_slug else f"#{fname}"
        sha256, size_bytes = _file_sha256_and_size(fpath)

        if fname.endswith(".apk") and not fname.endswith(".idsig"):
            if "-root" in fname:
                continue
            app_key = _resolve_app_key(fname, is_module=False)
            rep = report_apps.get(app_key, {}) if isinstance(report_apps, dict) else {}
            archs = _apk_archs(fpath) or rep.get("supported_archs")
            opt_mode = rep.get("optimization_mode") or (
                "lite" if (apps_cfg.get(app_key, {}) or {}).get("mode") == "lite" else "full"
            )
            entry = {
                "filename": fname,
                "url": url,
                "tag": tag_name,
                "updated_at": now_iso,
                "sha256": sha256,
                "size_bytes": size_bytes,
                "optimization_mode": opt_mode,
                "validation_status": _validation_for_app(app_key),
            }
            if rep.get("resolved_version"):
                entry["version"] = rep["resolved_version"]
            if archs:
                entry["supported_archs"] = archs
            registry["stock_apps"][app_key] = entry
        elif fname.endswith(".zip"):
            payload, zip_vc = _inspect_zip_payload(fpath)
            env_vc = os.environ.get("VERSION_CODE", "").strip()
            resolved_vc = zip_vc or (int(env_vc) if env_vc.isdigit() else None)
            if "custom" in fname or "custom" in tag_name:
                entry = {
                    "filename": fname,
                    "url": url,
                    "tag": tag_name,
                    "updated_at": now_iso,
                    "sha256": sha256,
                    "size_bytes": size_bytes,
                }
                if resolved_vc is not None:
                    entry["version_code"] = resolved_vc
                if payload is not None:
                    entry["payload"] = payload
                registry["custom_bundles"][fname] = entry
            elif "bundle" in fname:
                entry = {
                    "filename": fname,
                    "url": url,
                    "tag": tag_name,
                    "updated_at": now_iso,
                    "sha256": sha256,
                    "size_bytes": size_bytes,
                }
                if resolved_vc is not None:
                    entry["version_code"] = resolved_vc
                if payload is not None:
                    entry["payload"] = payload
                registry["bundles"][fname] = entry
            else:
                app_key = _resolve_app_key(fname, is_module=True)
                rep = report_apps.get(app_key, {}) if isinstance(report_apps, dict) else {}
                entry = {
                    "filename": fname,
                    "url": url,
                    "tag": tag_name,
                    "updated_at": now_iso,
                    "sha256": sha256,
                    "size_bytes": size_bytes,
                    "optimization_mode": rep.get("optimization_mode") or "full",
                    "validation_status": _validation_for_app(app_key),
                }
                if resolved_vc is not None:
                    entry["version_code"] = resolved_vc
                if rep.get("resolved_version"):
                    entry["version"] = rep["resolved_version"]
                if rep.get("supported_archs"):
                    entry["supported_archs"] = rep["supported_archs"]
                registry["single_modules"][app_key] = entry

    if tag_name.startswith("auto-v"):
        total_t = (test_report.get("passed", 0) + test_report.get("failed", 0) + test_report.get("skipped", 0)) if test_report else 0
        if total_t > 0:
            pass_rate = round(test_report["passed"] / total_t, 4)
        else:
            summ = build_report.get("summary", {}) if isinstance(build_report, dict) else {}
            tot_b = summ.get("total", 0)
            pass_rate = round((summ.get("success", 0) + summ.get("degraded", 0)) / tot_b, 4) if tot_b > 0 else None
        registry["build_health"] = {
            "release_tag": tag_name,
            "cli_version": build_report.get("revancex_version") or "1.1.0",
            "built_at": now_iso,
            "pass_rate": pass_rate,
            "active_sources": [],
        }

    modules_yaml_path = os.path.join(os.path.dirname(__file__), "..", "config", "modules.yaml")
    modules_cfg = {}
    valid_single_modules = None
    if _YAML_AVAILABLE and os.path.exists(modules_yaml_path):
        try:
            with open(modules_yaml_path, "r", encoding="utf-8") as f:
                y = yaml.safe_load(f)
                if y:
                    if "modules" in y:
                        modules_cfg = y["modules"]
                    if "single_modules" in y and isinstance(y["single_modules"], list):
                        valid_single_modules = set(y["single_modules"])
        except Exception as e:
            ulog.warn(f"modules.yaml: Failed to load, bundle table will use fallback: {e}")

    if valid_single_modules is not None:
        registry["single_modules"] = {
            k: v for k, v in registry.get("single_modules", {}).items()
            if k in valid_single_modules
        }

    with open(RELEASES_REGISTRY_PATH, "w", encoding="utf-8") as f:
        json.dump(registry, f, indent=2, sort_keys=True)

    app_names = sorted(list(set(
        list(registry["stock_apps"].keys()) +
        list(registry["single_modules"].keys())
    )))
    
    stock_module_table = [
        "| App | Patched APK | Single Root Module | Last Updated |",
        "| :--- | :--- | :--- | :--- |"
    ]
    for app in app_names:
        stock_info = registry["stock_apps"].get(app)
        module_info = registry["single_modules"].get(app)

        clean_name = apps_cfg.get(app, {}).get("name") if isinstance(apps_cfg.get(app), dict) else None
        if not clean_name:
            clean_name = app.replace("_", " ").title()
        
        stock_str = f"[{stock_info['filename']}]({stock_info['url']})" if stock_info else "—"
        module_str = f"[{module_info['filename']}]({module_info['url']})" if module_info else "—"
        
        timestamps = [
            info["updated_at"]
            for info in (stock_info, module_info)
            if info and "updated_at" in info
        ]
        date_str = max(timestamps) if timestamps else ""

        stock_module_table.append(f"| **{clean_name}** | {stock_str} | {module_str} | {date_str} |")
    if len(stock_module_table) == 2:
        stock_module_table.append("| _None yet_ | — | — | — |")

    bundle_table = [
        "| Bundle Name | Included Apps & Payload Files | Download Link | Release Tag | Updated At |",
        "| :--- | :--- | :--- | :--- | :--- |"
    ]
    for bname, bdata in sorted(registry["bundles"].items()):
        mod_key = "core" if bname == "revancex-bundle.zip" else bname.replace("revancex-bundle-", "").replace(".zip", "")
        mod_info = modules_cfg.get(mod_key)
        if mod_info and "apps" in mod_info:
            apps = mod_info["apps"]
            clean_bname = "Core Essentials Bundle" if mod_key == "core" else f"{mod_key.replace('-', ' ').title()} Bundle"
            list_items = "".join([f"<li><b>{app.replace('_', ' ').title()}</b>: <code>apks/{app}.apk</code></li>" for app in apps])
            included = f"<details><summary><b>{len(apps)} Apps / APKs (Click to view)</b></summary><ul>{list_items}</ul></details>"
        else:
            clean_bname = bname.replace("revancex-bundle-", "").replace(".zip", "").replace("-", " ").title() + " Bundle"
            included = "Configured apps"
        bundle_table.append(f"| **{clean_bname}**<br><code>{bname}</code> | {included} | [Download Zip]({bdata['url']}) | `{bdata['tag']}` | {bdata.get('updated_at', '')} |")
    if len(bundle_table) == 2:
        bundle_table.append("| _None yet_ | — | — | — | — |")

    custom_table = [
        "| Custom Bundle | Download Link | Release Tag | Updated At |",
        "| :--- | :--- | :--- | :--- |"
    ]
    for cname, cdata in sorted(registry["custom_bundles"].items()):
        custom_table.append(f"| **{cname}** | [Download Zip]({cdata['url']}) | `{cdata['tag']}` | {cdata.get('updated_at', '')} |")
    if len(custom_table) == 2:
        custom_table.append("| _None yet_ | — | — | — |")

    content_parts = [
        START_MARKER,
        '### <i class="fa-solid fa-mobile-screen"></i> Patched Apps & Single Root Modules',
        "\n".join(stock_module_table),
        "",
        '### <i class="fa-solid fa-box-archive"></i> Bundle Modules (Multi-App)',
        "\n".join(bundle_table),
        "",
        '### <i class="fa-solid fa-screwdriver-wrench"></i> Custom Bundle Modules',
        "\n".join(custom_table),
        END_MARKER
    ]
    replacement_text = "\n\n".join(content_parts)

    readme_content = ""
    if os.path.exists(README_PATH):
        with open(README_PATH, "r", encoding="utf-8") as f:
            readme_content = f.read()

    if START_MARKER in readme_content and END_MARKER in readme_content:
        pattern = re.compile(f"{re.escape(START_MARKER)}.*?{re.escape(END_MARKER)}", re.DOTALL)
        updated_readme = pattern.sub(replacement_text, readme_content)
    else:
        updated_readme = readme_content.rstrip() + "\n\n" + replacement_text + "\n"

    with open(README_PATH, "w", encoding="utf-8") as f:
        f.write(updated_readme)
    ulog.ok("README: Updated successfully with persistent releases registry.")

    sources_yaml_path = os.path.join(os.path.dirname(__file__), "..", "config", "sources.yaml")
    patch_sources_table = build_patch_sources_table(sources_yaml_path)
    if patch_sources_table:
        with open(README_PATH, "r", encoding="utf-8") as f:
            readme_content = f.read()
        updated = _replace_block(readme_content, PATCH_SOURCES_START, PATCH_SOURCES_END, patch_sources_table)
        with open(README_PATH, "w", encoding="utf-8") as f:
            f.write(updated)
        ulog.ok("README: Patch sources block updated from sources.yaml.")

    bundle_breakdown_table = build_bundle_breakdown_table(modules_yaml_path)
    if bundle_breakdown_table:
        with open(README_PATH, "r", encoding="utf-8") as f:
            readme_content = f.read()
        updated = _replace_block(readme_content, BUNDLE_BREAKDOWN_START, BUNDLE_BREAKDOWN_END, bundle_breakdown_table)
        with open(README_PATH, "w", encoding="utf-8") as f:
            f.write(updated)
        ulog.ok("README: Bundle breakdown block updated from modules.yaml.")

    if new_files:
        built_list_str = "\n".join([f"• <code>{f}</code>" for f in new_files if not f.endswith(".idsig")])
        tg_text = (
            f"🚀 <b>ReVanceX Build Finished!</b>\n\n"
            f"<b>Tag:</b> <code>{tag_name}</code>\n"
            f"<b>Run:</b> #{run_number}\n"
            f"<b>Time:</b> {now_iso}\n\n"
            f"<b>Built Assets:</b>\n{built_list_str}\n\n"
            f"🔗 <a href='https://github.com/{repo_slug}/releases/tag/{tag_name}'>View GitHub Release</a>"
        )
        send_telegram_message(telegram_token, telegram_chat_id, tg_text, topic_id=telegram_topic_id)

if __name__ == "__main__":
    main()
