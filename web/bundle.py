#!/usr/bin/env python3
"""Packs prepared resources for the browser build.

boot.tar.gz holds what the game reads before the title screen and for a
world (everything but other languages, sounds, the world generator's bundle
and caches the game rebuilds); manifest.txt lists the rest, which the game fetches
from res/ the first time it reads a file. The world generator's folders are
read together, hundreds of files when a world starts: each is also packed whole
(worldgen.tar.gz...), which the manifest names for its files, so the first read
fetches all of them in one request.

Usage: bundle.py RESOURCES OUT
"""

import gzip
import hashlib
import os
import sys
import tarfile
from pathlib import Path

# Fetched when first read instead of downloaded up front.
LAZY_DIRS = ("worldgen", "biome-parameters")
LAZY_FILES = ("horizon-sprites.cache",)
KEPT_LANGUAGES = ("en_us.json",)
# Lazy folders fetched whole, as <folder>.tar.gz, when one of their files is first read.
PACKED_DIRS = ("worldgen", "biome-parameters")


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
    packs = {}
    for folder in PACKED_DIRS:
        files = [(rel, path) for rel, path in rest if rel.split("/")[0] == folder]
        if not files:
            continue
        name = f"{folder}.tar.gz"
        # mtime 0 and sorted names: the same files make the same archive, and the same hash.
        with open(out / name, "wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", compresslevel=9, mtime=0) as gz:
            with tarfile.open(fileobj=gz, mode="w", format=tarfile.GNU_FORMAT) as tar:
                for rel, path in files:
                    info = tar.gettarinfo(path, arcname=rel)
                    info.mtime, info.uid, info.gid, info.uname, info.gname = 0, 0, 0, "", ""
                    with open(path, "rb") as f:
                        tar.addfile(info, f)
        digest = hashlib.sha256((out / name).read_bytes()).hexdigest()[:16]
        # Relative to res/, where the manifest's files are fetched from; the
        # hash lets the service worker keep it until it changes.
        packs[folder] = f"../{name}?v={digest}"
    with open(out / "manifest.txt", "w") as manifest:
        for rel, path in rest:
            pack = packs.get(rel.split("/")[0])
            manifest.write(f"{rel}\t{path.stat().st_size}" + (f"\t{pack}" if pack else "") + "\n")
    res = out / "res"
    if res.is_symlink() or res.exists():
        res.unlink()
    res.symlink_to(resources.resolve(), target_is_directory=True)
    size = (out / "boot.tar.gz").stat().st_size
    print(f"boot.tar.gz: {len(boot)} files, {size / 1e6:.1f} MB; {len(rest)} fetched when read")


if __name__ == "__main__":
    main()
