"""
Offline smoke test for scripts/summarize_build_report.py.
Run: python scripts/tests/test_summarize_build_report.py
Exit 0 = pass, non-zero = fail.
"""
import os
import sys
import json
import tempfile

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from summarize_build_report import summarize_report


def test_summarize_report():
    tmp_dir = tempfile.mkdtemp(prefix="revancex_test_summary_")
    try:
        report_file = os.path.join(tmp_dir, "BUILD_REPORT.json")
        summary_file = os.path.join(tmp_dir, "step_summary.md")

        data = {
            "schema_version": 1,
            "apps": {
                "youtube": {
                    "status": "success",
                    "resolved_source": "apkmirror",
                    "resolved_version": "21.13.164"
                },
                "facebook": {
                    "status": "failed",
                    "resolved_source": None,
                    "target_version": "400.0.0.0"
                }
            },
            "summary": {
                "total": 2,
                "success": 1,
                "failed": 1,
                "skipped": 0,
                "open_breakers": ["uptodown"],
                "failures_by_category": {
                    "all_candidate_sources_open_breakers": 1
                }
            }
        }

        with open(report_file, "w", encoding="utf-8") as f:
            json.dump(data, f)

        res = summarize_report(report_file, summary_file)
        assert res is True, "summarize_report should return True on success"

        with open(summary_file, "r", encoding="utf-8") as f:
            content = f.read()

        assert "### APK Source Acquisition Summary" in content
        assert "| youtube | success | apkmirror | 21.13.164 |" in content
        assert "| facebook | failed | none | 400.0.0.0 |" in content
        assert "### Build Summary" in content
        assert "- Total: 2" in content
        assert "#### Open Circuit Breakers" in content
        assert "- `uptodown`" in content
        assert "#### Failures by Category" in content
        assert "- all_candidate_sources_open_breakers: 1" in content
        print("test_summarize_report: PASS")
    finally:
        import shutil
        shutil.rmtree(tmp_dir, ignore_errors=True)


if __name__ == "__main__":
    test_summarize_report()
