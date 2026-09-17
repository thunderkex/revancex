import os
import sys
import re
import json
import urllib.request
import urllib.parse
from datetime import datetime, timezone

try:
    import yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False

README_PATH = "README.md"
RELEASES_REGISTRY_PATH = "config/releases_registry.json"
START_MARKER = "<!-- AUTO-APP-LIST-START -->"
END_MARKER = "<!-- AUTO-APP-LIST-END -->"

def send_telegram_message(token, chat_id, message, topic_id=None):
    if not token or not chat_id:
        print("[Telegram] Bot token or chat ID missing. Skipping notification.")
        return
    
    # Allow composite chat_id formatted as "<chat_id>:<topic_id>" or "<chat_id>/<topic_id>" or "<chat_id>#<topic_id>"
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
            print(f"[Telegram] Sending message to topic/thread ID: {payload['message_thread_id']}")
        except (ValueError, TypeError):
            print(f"[Telegram] Warning: Invalid topic ID: {topic_id}")

    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "User-Agent": "ReVanceX-Updater"}
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as res:
            if res.status == 200:
                print("[Telegram] Notification sent successfully.")
            else:
                print(f"[Telegram] Failed with status {res.status}")
    except Exception as e:
        print(f"[Telegram] Error sending message: {e}")

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
                registry = json.load(f)
        except Exception:
            pass

    now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    new_files = []
    
    if os.path.exists(output_dir):
        new_files = os.listdir(output_dir)

    for fname in new_files:
        fpath = os.path.join(output_dir, fname)
        if not os.path.isfile(fpath):
            continue
        
        url = f"https://github.com/{repo_slug}/releases/download/{tag_name}/{fname}" if repo_slug else f"#{fname}"
        
        if fname.endswith(".apk") and not fname.endswith(".idsig"):
            base = fname[:-4].replace("-patched", "")
            registry["stock_apps"][base] = {
                "filename": fname,
                "url": url,
                "tag": tag_name,
                "updated_at": now_iso
            }
        elif fname.endswith(".zip"):
            if "custom" in fname or "custom" in tag_name:
                registry["custom_bundles"][fname] = {
                    "filename": fname,
                    "url": url,
                    "tag": tag_name,
                    "updated_at": now_iso
                }
            elif "bundle" in fname:
                registry["bundles"][fname] = {
                    "filename": fname,
                    "url": url,
                    "tag": tag_name,
                    "updated_at": now_iso
                }
            else:
                base = fname[:-4]
                app_key = (
                    base.replace("revancex-module-", "")
                    .replace("revanced-module-", "")
                    .replace("-module", "")
                    .replace("revancex-", "")
                    .replace("revanced-", "")
                )
                registry["single_modules"][app_key] = {
                    "filename": fname,
                    "url": url,
                    "tag": tag_name,
                    "updated_at": now_iso
                }

    with open(RELEASES_REGISTRY_PATH, "w", encoding="utf-8") as f:
        json.dump(registry, f, indent=2)

    app_names = sorted(list(set(list(registry["stock_apps"].keys()) + list(registry["single_modules"].keys()))))
    
    stock_module_table = [
        "| App | Stock APK | Single Root Module | Last Updated |",
        "| :--- | :--- | :--- | :--- |"
    ]
    for app in app_names:
        stock_info = registry["stock_apps"].get(app)
        module_info = registry["single_modules"].get(app)

        clean_name = app.replace("_", " ").title()
        
        stock_str = f"[{stock_info['filename']}]({stock_info['url']})" if stock_info else "—"
        module_str = f"[{module_info['filename']}]({module_info['url']})" if module_info else "—"
        
        date_str = ""
        if stock_info:
            date_str = stock_info.get("updated_at", "")
        elif module_info:
            date_str = module_info.get("updated_at", "")

        stock_module_table.append(f"| **{clean_name}** | {stock_str} | {module_str} | {date_str} |")

    modules_yaml_path = os.path.join(os.path.dirname(__file__), "..", "config", "modules.yaml")
    modules_cfg = {}
    if _YAML_AVAILABLE and os.path.exists(modules_yaml_path):
        try:
            with open(modules_yaml_path, "r", encoding="utf-8") as f:
                y = yaml.safe_load(f)
                if y and "modules" in y:
                    modules_cfg = y["modules"]
        except Exception as e:
            print(f"[modules.yaml] Failed to load, bundle table will use fallback: {e}")

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
        "### 📱 Stock Apps & Single Root Modules",
        "\n".join(stock_module_table),
        "",
        "### 📦 Bundle Modules (Multi-App)",
        "\n".join(bundle_table),
        "",
        "### 🛠️ Custom Bundle Modules",
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
    print("[README] Updated successfully with persistent releases registry.")

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
