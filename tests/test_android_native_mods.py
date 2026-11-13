"""tools/android_native_mods.py: the stage keeps what didn't change, so cargo rebuilds only what did."""
import importlib.util
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

    def test_a_build_folder_in_the_stage_stays(self):
        write(self.fresh / "Cargo.toml", "[workspace]")
        write(self.stage / "target" / "debug" / "game", "built")
        mods.sync(self.fresh, self.stage)
        self.assertEqual((self.stage / "target" / "debug" / "game").read_text(), "built")


if __name__ == "__main__":
    unittest.main()
