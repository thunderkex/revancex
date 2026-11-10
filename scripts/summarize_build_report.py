#!/usr/bin/env python3
import json
import os
import sys


def summarize_report(report_path, summary_path):
    if not summary_path or not os.path.exists(report_path):
        return False

    with open(report_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    summ = data.get("summary", {})
    if not isinstance(summ, dict) or not summ:
        return True

    lines = []
    cats = summ.get("failures_by_category", {})
    if isinstance(cats, dict) and cats:
        items = ", ".join(f"`{cat}` ({cnt})" for cat, cnt in cats.items())
        lines.append(f"**Failures:** {items}\n")

    open_breakers = summ.get("open_breakers", [])
    if open_breakers:
        breakers = ", ".join(f"`{ob}`" for ob in open_breakers)
        lines.append(f"**Open circuit breakers:** {breakers}\n")

    if lines:
        with open(summary_path, "a", encoding="utf-8") as out:
            out.write("\n")
            out.writelines(lines)
    return True


def main():
    report_path = sys.argv[1] if len(sys.argv) > 1 else "output/BUILD_REPORT.json"
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    summarize_report(report_path, summary_path)


if __name__ == "__main__":
    main()
