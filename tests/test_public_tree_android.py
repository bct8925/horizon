"""The public-tree check lets the Android app's own sources in, and nothing else of its kind."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("check_public_tree", ROOT / "tools/check_public_tree.py")
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)


class AndroidAppTests(unittest.TestCase):
    def test_the_app_s_activities_and_gradle_wrapper_are_allowed(self):
        app = "packaging/android/app/src/main/java/dev/playhorizon/android/"
        self.assertFalse(check.prohibited(app + "HorizonActivity.java"))
        self.assertFalse(check.prohibited("packaging/android/gradle/wrapper/gradle-wrapper.jar"))

    def test_other_java_and_jars_stay_private(self):
        self.assertTrue(check.prohibited("packaging/android/app/src/main/java/net/minecraft/Foo.java"))
        self.assertTrue(check.prohibited("packaging/android/app/libs/client.jar"))
        self.assertTrue(check.prohibited("packaging/android/gradle/wrapper/other.jar"))


if __name__ == "__main__":
    unittest.main()
