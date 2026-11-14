"""tools/android_native_mods.py: the stage keeps what didn't change, so cargo rebuilds only what did."""
import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("android_native_mods", ROOT / "tools" / "android_native_mods.py")
mods = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mods)


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class SyncTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        base = Path(self.dir.name)
        self.fresh, self.stage = base / "fresh", base / "stage"
        self.old_stage = mods.STAGE
        mods.STAGE = self.stage

    def tearDown(self):
        mods.STAGE = self.old_stage
        self.dir.cleanup()

    def test_only_changed_files_are_replaced_and_gone_ones_removed(self):
        write(self.fresh / "src" / "same.rs", "same")
        write(self.fresh / "src" / "changed.rs", "new")
        write(self.fresh / "src" / "added.rs", "added")
        write(self.stage / "src" / "same.rs", "same")
        write(self.stage / "src" / "changed.rs", "old")
        write(self.stage / "src" / "gone.rs", "gone")
        write(self.stage / "old" / "x.rs", "x")
        os.utime(self.stage / "src" / "same.rs", (1_000_000, 1_000_000))
        mods.sync(self.fresh, self.stage)
        self.assertEqual((self.stage / "src" / "changed.rs").read_text(), "new")
        self.assertEqual((self.stage / "src" / "added.rs").read_text(), "added")
        self.assertFalse((self.stage / "src" / "gone.rs").exists())
        self.assertFalse((self.stage / "old").exists())
        # Untouched: cargo sees the same file it built.
        self.assertEqual((self.stage / "src" / "same.rs").stat().st_mtime, 1_000_000)

    def test_the_mixins_record_is_kept_when_only_its_inputs_changed(self):
        record = {"schema": 6, "injections": [{"mod_id": "minimap"}], "modules": []}
        write(self.fresh / mods.MARKER, json.dumps({**record, "inputs": {"docs/a.md": "new"}}))
        write(self.stage / mods.MARKER, json.dumps({**record, "inputs": {"docs/a.md": "old"}}))
        os.utime(self.stage / mods.MARKER, (1_000_000, 1_000_000))
        mods.sync(self.fresh, self.stage)
        # Untouched: the game's build script, which watches it, doesn't run again.
        self.assertEqual((self.stage / mods.MARKER).stat().st_mtime, 1_000_000)

    def test_the_mixins_record_is_replaced_when_the_mods_changed(self):
        write(self.fresh / mods.MARKER, json.dumps({"injections": [{"mod_id": "zoom"}], "inputs": {}}))
        write(self.stage / mods.MARKER, json.dumps({"injections": [{"mod_id": "minimap"}], "inputs": {}}))
        mods.sync(self.fresh, self.stage)
        self.assertIn("zoom", (self.stage / mods.MARKER).read_text())

    def test_a_build_folder_in_the_stage_stays(self):
        write(self.fresh / "Cargo.toml", "[workspace]")
        write(self.stage / "target" / "debug" / "game", "built")
        mods.sync(self.fresh, self.stage)
        self.assertEqual((self.stage / "target" / "debug" / "game").read_text(), "built")



class ModFolderTest(unittest.TestCase):
    """A mod's folder: native code, content, or both, as git repositories and the registry ship them."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.base = Path(self.dir.name)
        self.old = (mods.BUNDLES, mods.STAGE)
        mods.BUNDLES = self.base / "bundles"

    def tearDown(self):
        mods.BUNDLES, mods.STAGE = self.old
        self.dir.cleanup()

    def bundle(self, folder, mod_id, target):
        write(folder / "mixins" / mod_id / "horizon-mixin.toml",
              f'[mod]\nid = "{mod_id}"\n[compatibility]\nhorizon_revision = "{"a" * 40}"\n'
              f'[[injection]]\ntarget = "{target}"\n')

    def test_a_registry_hnmod_names_its_horizon_mixins_in_its_manifest(self):
        folder = self.base / "treefall"
        self.bundle(folder, "treefall", "game::Game::break_block")
        write(folder / "horizon-native.json", '{"id": "treefall", "mixins": "0d6b103"}')
        mod = mods.native_part(folder)
        self.assertEqual(mod.id, "treefall")
        self.assertEqual(mod.compat["horizon_mixins_revision"], "0d6b103")
        self.assertIsNone(mods.content_part(folder))

    def test_content_is_found_in_the_folder_or_its_content_folder(self):
        content_only = self.base / "mythril"
        write(content_only / "horizon-mod.toml", 'id = "mythril"\n')
        both = self.base / "botanypots"
        self.bundle(both, "botanypots", "x")
        write(both / "compatibility.json", '{"horizon_mixins_revision": "d9c83d7"}')
        write(both / "content" / "horizon-mod.toml", 'id = "botanypots"\n')
        self.assertIsNone(mods.native_part(content_only))
        self.assertEqual(mods.content_id(mods.content_part(content_only)), "mythril")
        self.assertEqual(mods.content_part(both), (both / "content").resolve())
        self.assertEqual(mods.native_part(both).id, "botanypots")

    def test_a_moved_function_s_injections_are_retargeted_and_the_build_pinned(self):
        folder = self.base / "treefall"
        self.bundle(folder, "treefall", "game::Game::break_block")
        write(folder / "horizon-native.json", '{"mixins": "0d6b103"}')
        mod = mods.native_part(folder)
        mod.retarget = {"game::Game::break_block": "game::block_breaking::Game::break_block"}
        mods.copy_bundles([mod], "b" * 40)
        toml = (mods.BUNDLES / "treefall" / "horizon-mixin.toml").read_text()
        self.assertIn('target = "game::block_breaking::Game::break_block"', toml)
        self.assertIn(f'horizon_revision = "{"b" * 40}"', toml)

    def test_a_retarget_naming_no_injection_stops_the_build(self):
        folder = self.base / "lookat"
        self.bundle(folder, "lookat", "game::Game::native_hud")
        write(folder / "horizon-native.json", '{"mixins": "0d6b103"}')
        mod = mods.native_part(folder)
        mod.retarget = {"game::Game::gone": "game::Game::elsewhere"}
        with self.assertRaises(SystemExit):
            mods.copy_bundles([mod], "b" * 40)


    def test_a_module_s_heading_doc_comments_become_plain_ones(self):
        folder = self.base / "mythril"
        write(folder / "mixins" / "mythril" / "horizon-mixin.toml",
              '[mod]\nid = "mythril"\n[compatibility]\nhorizon_revision = "' + "a" * 40 + '"\n'
              '[[module]]\nsource = "ore.rs"\n')
        write(folder / "mixins" / "mythril" / "ore.rs", "//! The ore.\n//! More.\nfn f() {}\n//! kept\n")
        write(folder / "compatibility.json", '{"horizon_mixins_revision": "d9c83d7"}')
        mod = mods.native_part(folder)
        mod.plain_module_comments = True
        mods.copy_bundles([mod], "b" * 40)
        self.assertEqual((mods.BUNDLES / "mythril" / "ore.rs").read_text(),
                         "// The ore.\n// More.\nfn f() {}\n//! kept\n")


if __name__ == "__main__":
    unittest.main()
