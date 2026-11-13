#!/usr/bin/env python3
"""Stages Horizon with native mods applied, for the Android build (docs/android.md).

The launcher builds a player's native mods into the game on the PC; the Android build has no
launcher, so this does the same for it: it copies this checkout's source (tracked and new files,
not ignored ones) to `.local/android/source`, applies the mods there with horizon-mixins (the
commit the mods pin, built once into `.local/android/tools`), and leaves the result in
`.local/android/stage`, where Gradle's `cargoNdk` builds it. `HORIZON_ANDROID_DIR` puts the copy,
the stage and the build folder elsewhere (one per branch, say). Only files that changed are
replaced in the stage, so cargo rebuilds only what changed. The checkout itself is never edited.

Each argument is a native mod's folder as horizon-rust/horizon-mods has it (`native/minimap`):
a `compatibility.json` and a `mixins/<id>/horizon-mixin.toml` bundle.

    python tools/android_native_mods.py ../horizon-mods/native/minimap
"""
import filecmp
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# HORIZON_ANDROID_DIR (absolute or from the repository root) gives a build its own work folder, so
# builds of different branches each keep theirs warm; horizon-mixins stays shared.
WORK = ROOT / os.environ.get("HORIZON_ANDROID_DIR", str(Path(".local") / "android"))
TOOLS = ROOT / ".local" / "android" / "tools"
SOURCE = WORK / "source"
BUNDLES = WORK / "mods"
STAGE = WORK / "stage"
FRESH = WORK / "stage.new"
EXE = ".exe" if os.name == "nt" else ""
# Cargo's outputs, if a build ever ran inside the stage: kept, never compared.
KEEP = {"target"}
# horizon-mixins' record of what it applied (horizon_mod_api_gen::MIXINS_STAGE). The game's build
# script watches it, but reads only the mods it names; its `inputs`, a hash of every file of the
# copy, docs included, change with any commit, and rewriting it for them alone would recompile
# the whole game.
MARKER = ".horizon-mixins-stage.json"


def same_record(fresh, kept):
    """Whether two records name the same mods' injections and modules, whatever their inputs."""
    try:
        a, b = (json.loads(path.read_text(encoding="utf-8")) for path in (fresh, kept))
    except (OSError, ValueError):
        return False
    a.pop("inputs", None)
    b.pop("inputs", None)
    return a == b


class NativeMod:
    def __init__(self, folder):
        self.root = Path(folder).resolve()
        self.compat = json.loads((self.root / "compatibility.json").read_text(encoding="utf-8"))
        bundles = [p for p in (self.root / "mixins").iterdir() if (p / "horizon-mixin.toml").is_file()]
        if len(bundles) != 1:
            sys.exit(f"{self.root}: expected one mixins/<id>/horizon-mixin.toml, found {len(bundles)}")
        self.bundle = bundles[0]
        self.id = self.bundle.name


def tool(mods):
    """horizon-mixins at the commit the mods pin, built once."""
    revs = {m.compat["horizon_mixins_revision"] for m in mods}
    if len(revs) != 1:
        sys.exit(f"the mods pin different horizon-mixins commits: {sorted(revs)}")
    rev = revs.pop()
    root = TOOLS / "horizon-mixins" / rev
    binary = root / "bin" / f"horizon-mixins{EXE}"
    if not binary.is_file():
        print(f"Building horizon-mixins {rev[:7]}", flush=True)
        repo = mods[0].compat["horizon_mixins_repository"]
        subprocess.run(["cargo", "install", "--locked", "--quiet", "--git", repo, "--rev", rev,
                        "--root", str(root), "horizon-mixins-cli"], check=True)
    return binary


def copy_source():
    """This checkout's files, as git sees them (tracked and new, not ignored), links left out."""
    listed = subprocess.run(["git", "ls-files", "-co", "--exclude-standard", "-z"], cwd=ROOT,
                            check=True, capture_output=True).stdout.decode("utf-8").split("\0")
    shutil.rmtree(SOURCE, ignore_errors=True)
    for name in filter(None, listed):
        src = ROOT / name
        if src.is_symlink() or not src.is_file():
            continue
        dst = SOURCE / name
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
    revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, check=True,
                              capture_output=True, text=True).stdout.strip()
    (SOURCE / ".horizon-revision").write_text(revision)
    return revision


def copy_bundles(mods, revision):
    """The bundles, pinned to the commit they build on, as the launcher does."""
    shutil.rmtree(BUNDLES, ignore_errors=True)
    for m in mods:
        shutil.copytree(m.bundle, BUNDLES / m.id)
        licence = m.root / "LICENSE"
        if licence.is_file() and not (BUNDLES / m.id / "LICENSE").exists():
            shutil.copy2(licence, BUNDLES / m.id / "LICENSE")
        toml = BUNDLES / m.id / "horizon-mixin.toml"
        toml.write_text(re.sub(r'horizon_revision = "[0-9a-f]{40}"', f'horizon_revision = "{revision}"',
                               toml.read_text(encoding="utf-8")), encoding="utf-8")


def sync(fresh, out):
    """Make `out` hold what `fresh` holds, replacing only files whose bytes changed."""
    out.mkdir(parents=True, exist_ok=True)
    for entry in out.iterdir():
        if entry.name in KEEP and out == STAGE:
            continue
        if not (fresh / entry.name).exists() or entry.is_dir() != (fresh / entry.name).is_dir():
            shutil.rmtree(entry) if entry.is_dir() else entry.unlink()
    for entry in fresh.iterdir():
        target = out / entry.name
        if entry.is_dir():
            sync(entry, target)
        elif not target.is_file() or not filecmp.cmp(entry, target, shallow=False):
            if entry.name == MARKER and target.is_file() and same_record(entry, target):
                continue
            shutil.copy2(entry, target)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    mods = [NativeMod(folder) for folder in sys.argv[1:]]
    binary = tool(mods)
    revision = copy_source()
    copy_bundles(mods, revision)
    shutil.rmtree(FRESH, ignore_errors=True)
    subprocess.run([str(binary), "apply", "--workspace", str(SOURCE), "--mods", str(BUNDLES),
                    "--out", str(FRESH)], check=True, stdout=subprocess.DEVNULL)
    sync(FRESH, STAGE)
    shutil.rmtree(FRESH)
    print(f"Staged Horizon {revision[:7]} with {', '.join(m.id for m in mods)} in {STAGE}")


if __name__ == "__main__":
    main()
