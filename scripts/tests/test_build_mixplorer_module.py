#!/usr/bin/env python3
"""Unit tests for scripts/build_mixplorer_module.py."""

import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import build_mixplorer_module as mod


class TestMixplorerModule(unittest.TestCase):
    def test_select_apk_for_arm64_and_universal(self):
        files = [
            {"id": "1", "name": "MiXplorer_v6.71.15-API29_B26090430.apk", "is_folder": False},
            {"id": "2", "name": "MiXplorer_v6.71.15_B26090420.apk", "is_folder": False},
            {"id": "3", "name": "MiXplorer_v6.71.15_B26090421-arm.apk", "is_folder": False},
            {"id": "4", "name": "MiXplorer_v6.71.15_B26090422-arm64.apk", "is_folder": False},
            {"id": "5", "name": "MiXplorer_v6.71.15_B26090423-x86.apk", "is_folder": False},
        ]
        self.assertEqual(mod.select_apk_for_arch(files, "arm64")["id"], "4")
        self.assertEqual(mod.select_apk_for_arch(files, "universal")["id"], "2")

        encrypt_only = [
            {"id": "9", "name": "MiXEncrypt_B2512251-universal.apk", "is_folder": False}
        ]
        self.assertEqual(mod.select_apk_for_arch(encrypt_only, "arm64")["id"], "9")

    def test_compute_changes_up_to_date_and_addon_bump(self):
        discovered = {
            "folder_id": "root",
            "arch": "arm64",
            "mixplorer_version": "6.71.15",
            "apps": [
                {"app_dir": "MiXplorer", "build": "B26090422", "filename": "MiXplorer_v6.71.15_B26090422-arm64.apk", "file_id": "a"},
                {"app_dir": "MiXArchive", "build": "B2608212", "filename": "MiXArchive_B2608212-arm64.apk", "file_id": "b"},
            ],
        }
        same_release = {
            "tag_name": "mixplorer-v6.71.15",
            "body": (
                "| **MiXplorer** | `v6.71.15 (B26090422)` | `arm64-v8a` |\n"
                "| **MiX Archive** | `B2608212` | `arm64-v8a` |\n"
            ),
        }
        res_same = mod.compute_changes(discovered, same_release, force=False)
        self.assertFalse(res_same["has_updates"])
        self.assertEqual(res_same["tag"], "mixplorer-v6.71.15")

        discovered_addon_bump = {
            **discovered,
            "apps": [
                discovered["apps"][0],
                {**discovered["apps"][1], "build": "B2609999"},
            ],
        }
        res_bump = mod.compute_changes(discovered_addon_bump, same_release, force=False)
        self.assertTrue(res_bump["has_updates"])
        self.assertEqual(res_bump["tag"], "mixplorer-v6.71.15-r1")
        self.assertEqual(res_bump["version_code"], 6711501)

    def test_build_module_zip_bundles_apks_for_pm_install(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_apk = tmp_path / "MiXplorer_v6.71.15_B26090422-arm64.apk"
            with zipfile.ZipFile(fake_apk, "w") as zf:
                zf.writestr("AndroidManifest.xml", b"manifest")
                zf.writestr("lib/arm64-v8a/libmixbox.so", b"arm64so")

            discovered = {
                "folder_id": "root",
                "arch": "arm64",
                "mixplorer_version": "6.71.15",
                "apps": [
                    {
                        "app_dir": "MiXplorer",
                        "folder": "MiXplorer-v6.71.15",
                        "file_id": "id1",
                        "filename": fake_apk.name,
                        "build": "B26090422",
                    }
                ],
            }
            out_zip = tmp_path / "out.zip"
            pkg_map = mod.build_module_zip(
                discovered, {"MiXplorer": fake_apk}, out_zip, 6711500, "mixplorer-v6.71.15"
            )
            self.assertEqual(pkg_map, {"MiXplorer": "com.mixplorer"})
            with zipfile.ZipFile(out_zip, "r") as zf:
                names = set(zf.namelist())
                self.assertIn("apks/MiXplorer.apk", names)
                self.assertIn("apks.list", names)
                self.assertIn("service.sh", names)
                self.assertIn("customize.sh", names)
                self.assertIn("uninstall.sh", names)
                self.assertFalse(any(n.startswith("system/") for n in names))
                self.assertEqual(zf.read("apks.list").decode("utf-8"), "MiXplorer:com.mixplorer\n")
                self.assertIn(b"pm install -r -d -S", zf.read("customize.sh"))
                self.assertIn(b"pm install -r -d -S", zf.read("service.sh"))
                self.assertIn(b"pm uninstall", zf.read("uninstall.sh"))
                for script in ("customize.sh", "service.sh", "uninstall.sh", "module.prop", "apks.list"):
                    self.assertNotIn(b"\r", zf.read(script))


if __name__ == "__main__":
    unittest.main()
