#!/usr/bin/env python3
"""Point this fork's Homebrew Channel repository at a published test build.

    python3 homebrew/update.py test-9

Reads the release with `gh`, downloads its .ipk to hash it, and rewrites the three files Homebrew
Channel fetches from this folder: `repo.json` (the index a television adds once),
the package manifest, and `description.html`. Commit and push them to `main` afterwards.

Homebrew Channel resolves `manifestUrl` and `fullDescriptionUrl` against the repository's own
URL, so the index names its neighbours by file name and the folder can move without edits.
"""
from __future__ import annotations

import hashlib
import html
import json
import subprocess
import sys
import urllib.request
from pathlib import Path

REPO = "deten/plx-native"
PACKAGE_ID = "com.beb.plxnative.debug"
TITLE = "PlxNative debug"
MANIFEST_NAME = f"{PACKAGE_ID}.manifest.json"
ICON = f"https://raw.githubusercontent.com/{REPO}/main/pkg/dev/largeIcon.png"
# Homebrew Channel's list shows this; its limit is 80 characters.
SHORT = "Test builds of this PlxNative fork. Installs beside the regular PlxNative app."
HERE = Path(__file__).resolve().parent


def release(tag: str) -> dict:
    out = subprocess.run(
        ["gh", "release", "view", tag, "-R", REPO, "--json", "tagName,name,url,targetCommitish,assets"],
        check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def ipk_asset(rel: dict) -> dict:
    ipks = [a for a in rel["assets"] if a["name"].endswith(".ipk")]
    if len(ipks) != 1:
        raise SystemExit(f"{rel['tagName']} has {len(ipks)} .ipk assets, want 1")
    return ipks[0]


def sha256_of(url: str) -> "tuple[str, int]":
    h, size = hashlib.sha256(), 0
    with urllib.request.urlopen(url) as resp:  # noqa: S310  (a public release asset)
        while chunk := resp.read(1 << 20):
            h.update(chunk)
            size += len(chunk)
    return h.hexdigest(), size


def version_of(ipk_name: str) -> str:
    # `<id>_<version>_arm.ipk`, the build's own file name
    prefix, suffix = f"{PACKAGE_ID}_", "_arm.ipk"
    if not (ipk_name.startswith(prefix) and ipk_name.endswith(suffix)):
        raise SystemExit(f"unexpected package name {ipk_name!r}")
    return ipk_name[len(prefix):-len(suffix)]


def files_for(rel: dict, sha256: str, size: int) -> "dict[str, str]":
    asset = ipk_asset(rel)
    manifest = {
        "id": PACKAGE_ID,
        "version": version_of(asset["name"]),
        "type": "native",
        "title": TITLE,
        "appDescription": SHORT,
        "iconUri": ICON,
        "sourceUrl": f"https://github.com/{REPO}",
        "rootRequired": False,
        "ipkUrl": asset["url"],
        "ipkHash": {"sha256": sha256},
        "ipkSize": size,
    }
    index = {"packages": [{
        "id": PACKAGE_ID,
        "title": TITLE,
        "shortDescription": SHORT,
        "fullDescriptionUrl": "description.html",
        "iconUri": ICON,
        "manifestUrl": MANIFEST_NAME,
    }]}
    esc = html.escape
    commit = rel["targetCommitish"]
    description = "\n".join([
        f"<p><strong>{esc(rel['name'])}</strong> &middot; commit "
        f'<a href="https://github.com/{REPO}/commit/{esc(commit)}">{esc(commit[:7])}</a></p>',
        "<p>A test build of this PlxNative fork. It installs as its own app, "
        "<strong>PlxNative debug</strong>, with its own tile and sign-in, beside the regular "
        "PlxNative and the official Plex app.</p>",
        "<p><strong>Updating:</strong> every test build carries the same version number, so "
        "this page offers Launch rather than Update. To install the newest build, press "
        "<strong>5</strong> on the remote here, then select <strong>Reinstall</strong>. "
        "Your sign-in and settings stay as they are.</p>",
        f'<p><a href="{esc(rel["url"], quote=True)}">What is in this build</a> &middot; '
        f'<a href="https://github.com/{REPO}#readme">About the fork</a></p>',
    ]) + "\n"
    return {
        "repo.json": json.dumps(index, indent=2) + "\n",
        MANIFEST_NAME: json.dumps(manifest, indent=2) + "\n",
        "description.html": description,
    }


def main(argv: "list[str]") -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    rel = release(argv[1])
    sha256, size = sha256_of(ipk_asset(rel)["url"])
    if len(SHORT) > 80:
        raise SystemExit("short description is over Homebrew Channel's 80 characters")
    for name, text in files_for(rel, sha256, size).items():
        (HERE / name).write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote homebrew/{name}")
    print(f"sha256 {sha256}  size {size}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
