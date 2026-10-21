#!/usr/bin/env python3
import json
import os
import sys


def summarize_report(report_path, summary_path):
    if not summary_path or not os.path.exists(report_path):
        return False

    with open(report_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    lines = [
        "### APK Source Acquisition Summary\n\n",
        "| App | Status | Source | Resolved Version |\n",
        "| --- | --- | --- | --- |\n",
    ]

    apps = data.get("apps", data)
    if isinstance(apps, dict):
        for app, rep in apps.items():
            if isinstance(rep, dict):
                src = rep.get("resolved_source") or "none"
                ver = rep.get("resolved_version") or rep.get("target_version") or "none"
                st = rep.get("status") or "unknown"
                lines.append(f"| {app} | {st} | {src} | {ver} |\n")

    summ = data.get("summary", {})
    if isinstance(summ, dict) and summ:
        lines.append("\n### Build Summary\n\n")
        lines.append(
            f"- Total: {summ.get('total', 0)}\n"
            f"- Success: {summ.get('success', 0)}\n"
            f"- Failed: {summ.get('failed', 0)}\n"
            f"- Skipped: {summ.get('skipped', 0)}\n"
        )
        open_breakers = summ.get("open_breakers", [])
        if open_breakers:
            lines.append("\n#### Open Circuit Breakers\n\n")
            for ob in open_breakers:
                lines.append(f"- `{ob}`\n")
        cats = summ.get("failures_by_category", {})
        if cats:
            lines.append("\n#### Failures by Category\n\n")
            for cat, cnt in cats.items():
                lines.append(f"- {cat}: {cnt}\n")

    with open(summary_path, "a", encoding="utf-8") as out:
        out.writelines(lines)
    return True


def main():
    report_path = sys.argv[1] if len(sys.argv) > 1 else "output/BUILD_REPORT.json"
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    summarize_report(report_path, summary_path)


if __name__ == "__main__":
    main()
