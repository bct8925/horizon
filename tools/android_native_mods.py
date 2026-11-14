#!/usr/bin/env python3
"""Stages Horizon with native mods applied, for the Android build (docs/android.md).

The launcher builds a player's native mods into the game on the PC; the Android build has no
launcher, so this does the same for it: it copies this checkout's source (tracked and new files,
not ignored ones) to `.local/android/source`, applies the mods there with horizon-mixins (the
commit the mods pin, built once into `.local/android/tools`), and leaves the result in
`.local/android/stage`, where Gradle's `cargoNdk` builds it. `HORIZON_ANDROID_DIR` puts the copy,
the stage and the build folder elsewhere (one per branch, say). Only files that changed are
replaced in the stage, so cargo rebuilds only what changed. The checkout itself is never edited.

Each argument is a mod's folder. Its native code is a `compatibility.json` beside
`mixins/<id>/horizon-mixin.toml` (as horizon-rust/horizon-mods has `native/minimap`), its content
a `horizon-mod.toml` with `assets/` and `data/`, in the folder or in its `content/`; a mod may
have either or both, or it is a registry's `.hnmod` (`horizon-native.json` and the bundle).
`--manifest` takes them from a list instead (packaging/android/mods.toml): each `[[mod]]` a git
repository, its branch and the folder in it, or a file's `url` and `sha256` (an `.hnmod` from
Horizon's registry), fetched into `.local/android/mod-sources/<name>` (a private repository with
this machine's git sign-in); a `settings` folder beside the list holds settings for the APK.
A mod's `retarget` table moves its injections from a game function that has moved since the
Horizon it was made for to where that function is now (`"old path" = "new path"`), and its
`[[mod.replace]]` entries (`file`, `old`, `new`) edit its code where the game's own changed under
it; each must find its text exactly once.
`--content-out DIR` writes the mods' content to `DIR/native-content/<id>/`, which the APK packs
and the game reads at start, as the desktop game reads its version folder's `native-content/`.
When the mods pin different horizon-mixins commits, the newest of them applies them all.

    python tools/android_native_mods.py ../horizon-mods/native/minimap
    python tools/android_native_mods.py --manifest packaging/android/mods.toml --content-out out
"""
import argparse
import filecmp
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
import hashlib
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# HORIZON_ANDROID_DIR (absolute or from the repository root) gives a build its own work folder, so
# builds of different branches each keep theirs warm; horizon-mixins stays shared.
WORK = ROOT / os.environ.get("HORIZON_ANDROID_DIR", str(Path(".local") / "android"))
TOOLS = ROOT / ".local" / "android" / "tools"
SOURCES = ROOT / ".local" / "android" / "mod-sources"
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


MIXINS_REPOSITORY = "https://github.com/horizon-rust/horizon-mixins"


class NativeMod:
    def __init__(self, folder):
        self.root = Path(folder).resolve()
        compat = self.root / "compatibility.json"
        if compat.is_file():
            self.compat = json.loads(compat.read_text(encoding="utf-8"))
        else:
            # A registry `.hnmod`: its horizon-native.json names the horizon-mixins commit.
            native = json.loads((self.root / "horizon-native.json").read_text(encoding="utf-8"))
            self.compat = {"horizon_mixins_revision": native["mixins"],
                           "horizon_mixins_repository": MIXINS_REPOSITORY}
        bundles = [p for p in (self.root / "mixins").iterdir() if (p / "horizon-mixin.toml").is_file()]
        if len(bundles) != 1:
            sys.exit(f"{self.root}: expected one mixins/<id>/horizon-mixin.toml, found {len(bundles)}")
        self.bundle = bundles[0]
        self.id = self.bundle.name
        self.retarget = {}
        self.replace = []


def native_part(folder):
    """The folder's native code, if it has any."""
    folder = Path(folder)
    manifests = (folder / "compatibility.json", folder / "horizon-native.json")
    if any(m.is_file() for m in manifests) and (folder / "mixins").is_dir():
        return NativeMod(folder)
    return None


def content_part(folder):
    """The folder holding the mod's content (`horizon-mod.toml`, `assets/`, `data/`), if any."""
    folder = Path(folder).resolve()
    for candidate in (folder, folder / "content"):
        if (candidate / "horizon-mod.toml").is_file():
            return candidate
    return None


def content_id(folder):
    return tomllib.loads((folder / "horizon-mod.toml").read_text(encoding="utf-8"))["id"]


def download(entry, dest):
    """A `[[mod]]` that is a file (`url`, `sha256`): downloaded once, checked, and unpacked."""
    archive = dest / entry["url"].rsplit("/", 1)[-1]
    unpacked = dest / "unpacked"
    if not archive.is_file() or hashlib.sha256(archive.read_bytes()).hexdigest() != entry["sha256"]:
        shutil.rmtree(dest, ignore_errors=True)
        dest.mkdir(parents=True)
        with urllib.request.urlopen(entry["url"]) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != entry["sha256"]:
            sys.exit(f"{entry['url']}: not the file its sha256 names")
        archive.write_bytes(data)
    shutil.rmtree(unpacked, ignore_errors=True)
    with zipfile.ZipFile(archive) as z:
        for name in z.namelist():
            if name.startswith("/") or ".." in Path(name).parts:
                sys.exit(f"{archive}: {name} would land outside its folder")
        z.extractall(unpacked)
    return unpacked


def fetch(entry):
    """A `[[mod]]` of the list: its repository at its branch (fetched or brought up to date), or
    its file."""
    dest = SOURCES / entry["name"]
    if "url" in entry:
        return download(entry, dest)
    ref = entry.get("ref", "main")
    if (dest / ".git").is_dir():
        subprocess.run(["git", "-C", str(dest), "fetch", "-q", "--depth", "1", "origin", ref], check=True)
        subprocess.run(["git", "-C", str(dest), "checkout", "-q", "--detach", "FETCH_HEAD"], check=True)
    else:
        shutil.rmtree(dest, ignore_errors=True)
        dest.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "clone", "-q", "--depth", "1", "--branch", ref, entry["repo"], str(dest)],
                       check=True)
    return dest / entry.get("path", ".")


def newest(revs, repo):
    """Of horizon-mixins commits on one line of history, the newest: a bundle made for an older one
    applies with a newer one."""
    git = TOOLS / "horizon-mixins.git"
    if not git.is_dir():
        subprocess.run(["git", "init", "-q", "--bare", str(git)], check=True)
    for rev in revs:
        subprocess.run(["git", "-C", str(git), "fetch", "-q", "--filter=blob:none", repo, rev], check=True)
    best = revs[0]
    for rev in revs[1:]:
        if subprocess.run(["git", "-C", str(git), "merge-base", "--is-ancestor", best, rev]).returncode == 0:
            best = rev
        elif subprocess.run(["git", "-C", str(git), "merge-base", "--is-ancestor", rev, best]).returncode != 0:
            sys.exit(f"the mods pin horizon-mixins commits on different lines: {best[:7]}, {rev[:7]}")
    return best


def tool(mods):
    """horizon-mixins at the commit the mods pin (the newest of them), built once."""
    revs = sorted({m.compat["horizon_mixins_revision"] for m in mods})
    repo = mods[0].compat["horizon_mixins_repository"]
    rev = revs[0] if len(revs) == 1 else newest(revs, repo)
    root = TOOLS / "horizon-mixins" / rev
    binary = root / "bin" / f"horizon-mixins{EXE}"
    if not binary.is_file():
        print(f"Building horizon-mixins {rev[:7]}", flush=True)
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
        text = re.sub(r'horizon_revision = "[0-9a-f]{40}"', f'horizon_revision = "{revision}"',
                      toml.read_text(encoding="utf-8"))
        for old, new in m.retarget.items():
            if f'target = "{old}"' not in text:
                sys.exit(f"{m.id}: no injection targets {old} to move")
            text = text.replace(f'target = "{old}"', f'target = "{new}"')
        toml.write_text(text, encoding="utf-8")
        for edit in m.replace:
            path = BUNDLES / m.id / edit["file"]
            code = path.read_text(encoding="utf-8")
            if code.count(edit["old"]) != 1:
                sys.exit(f"{m.id}: {edit['file']} has {code.count(edit['old'])} copies of {edit['old']!r}")
            path.write_text(code.replace(edit["old"], edit["new"]), encoding="utf-8")


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


def export_content(folders, out, settings=None):
    """The mods' content under `out/native-content/<id>/`, as a build's version folder has it, and
    their settings (`settings/<id>/`, as `.horizon-mod-data/<id>/` holds them) under
    `out/mod-settings/`."""
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    for folder in folders:
        shutil.copytree(folder, out / "native-content" / content_id(folder))
    if settings and settings.is_dir():
        shutil.copytree(settings, out / "mod-settings")


def main():
    parser = argparse.ArgumentParser(description="Stages Horizon with native mods applied.")
    parser.add_argument("folders", nargs="*", help="mod folders")
    parser.add_argument("--manifest", help="a list of mods to fetch (packaging/android/mods.toml)")
    parser.add_argument("--content-out", help="where the mods' content goes for the APK")
    args = parser.parse_args()
    folders = [Path(f).resolve() for f in args.folders]
    edits = [({}, []) for _ in folders]
    if args.manifest:
        listed = tomllib.loads(Path(args.manifest).read_text(encoding="utf-8")).get("mod", [])
        folders += [fetch(entry).resolve() for entry in listed]
        edits += [(entry.get("retarget", {}), entry.get("replace", [])) for entry in listed]
    if not folders:
        sys.exit(__doc__)
    mods = []
    for folder, (retarget, replace) in zip(folders, edits):
        if m := native_part(folder):
            m.retarget, m.replace = retarget, replace
            mods.append(m)
    contents = [c for c in map(content_part, folders) if c]
    if args.content_out:
        settings = Path(args.manifest).resolve().parent / "settings" if args.manifest else None
        export_content(contents, Path(args.content_out).resolve(), settings)
        print(f"Content of {', '.join(map(content_id, contents)) or 'no mod'} in {args.content_out}")
    if not mods:
        sys.exit("no native mod to apply")
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
