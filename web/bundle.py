#!/usr/bin/env python3
"""Packs prepared resources for the browser build.

boot.tar.gz holds what the game reads before the title screen and for a
world (everything but other languages, sounds, the world generator's bundle
and caches the game rebuilds); manifest.txt lists the rest, which the game fetches
from res/ the first time it reads a file.

Usage: bundle.py RESOURCES OUT
"""

import os
import sys
import tarfile
from pathlib import Path

# Fetched when first read instead of downloaded up front.
LAZY_DIRS = ("worldgen", "biome-parameters")
LAZY_FILES = ("horizon-sprites.cache",)
KEPT_LANGUAGES = ("en_us.json",)


def lazy(rel: str) -> bool:
    parts = rel.split("/")
    if parts[0] in LAZY_DIRS or rel in LAZY_FILES:
        return True
    if parts[:3] == ["assets", "minecraft", "sounds"]:
        return True
    if parts[:3] == ["assets", "minecraft", "lang"] and len(parts) == 4:
        return parts[3] not in KEPT_LANGUAGES
    return False


def main() -> None:
    resources, out = Path(sys.argv[1]), Path(sys.argv[2])
    if not (resources / "horizon-resources.json").is_file():
        sys.exit(f"{resources} holds no prepared resources: run horizon-prepare first")
    out.mkdir(parents=True, exist_ok=True)
    boot, rest = [], []
    for dirpath, _, names in os.walk(resources):
        for name in names:
            path = Path(dirpath) / name
            rel = path.relative_to(resources).as_posix()
            (rest if lazy(rel) else boot).append((rel, path))
    boot.sort()
    rest.sort()
    with tarfile.open(out / "boot.tar.gz", "w:gz", format=tarfile.GNU_FORMAT, compresslevel=6) as tar:
        for rel, path in boot:
            tar.add(path, arcname=rel, recursive=False)
    with open(out / "manifest.txt", "w") as manifest:
        for rel, path in rest:
            manifest.write(f"{rel}\t{path.stat().st_size}\n")
    res = out / "res"
    if res.is_symlink() or res.exists():
        res.unlink()
    res.symlink_to(resources.resolve(), target_is_directory=True)
    size = (out / "boot.tar.gz").stat().st_size
    print(f"boot.tar.gz: {len(boot)} files, {size / 1e6:.1f} MB; {len(rest)} fetched when read")


if __name__ == "__main__":
    main()
