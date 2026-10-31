#!/usr/bin/env python3
"""Packages the browser build for players, as Eaglercraft ships: a web folder
to put on any static host, and one offline .html file that runs from the disk.

  web/out/horizon-web/           the web folder: upload it as is (GitHub Pages,
                                 Netlify, any server). Its service worker gives
                                 the page the headers the game's threads need;
                                 where it can't, the page runs the
                                 single-threaded build (pkg-st/).
  web/out/horizon-offline.html   one file, opened with a double click
                                 (file://): the single-threaded build and the
                                 resources inside it. A file:// page can't be
                                 cross-origin isolated, so it has no threads.

Usage:
  web/package.py [folder|offline|all] [--no-build] [--sounds none|effects|all] [--languages]

It builds first (web/build.sh into web/dist, and THREADS=0 into web/dist-st,
whose cargo target is target/web-st); PROFILE=fast and the other build.sh
variables pass through. --sounds and --languages pick what the offline file
holds: every sound but the music and music discs by default (the music is
250 MB), and English only. The web folder always has everything: its files
are fetched when the game first reads them.
"""

import argparse
import base64
import gzip
import hashlib
import io
import os
import re
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

WEB = Path(__file__).resolve().parent
REPO = WEB.parent
sys.path.insert(0, str(WEB))
import bundle  # noqa: E402

DIST = WEB / "dist"
DIST_ST = WEB / "dist-st"
PAGE_FILES = ("index.html", "main.js", "audio.js", "worker.js", "sw.js")


def build(threaded: bool, resources: bool) -> None:
    env = dict(os.environ)
    if not threaded:
        env.update(THREADS="0", DIST=str(DIST_ST), CARGO_TARGET_DIR=str(REPO / "target/web-st"))
    if not resources:
        env["BUNDLE"] = "0"
    subprocess.run([str(WEB / "build.sh")], env=env, check=True)


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()[:16]


def folder(out: Path) -> None:
    site = out / "horizon-web"
    if site.exists():
        shutil.rmtree(site)
    site.mkdir(parents=True)
    for name in PAGE_FILES:
        shutil.copy2(WEB / name, site / name)
    shutil.copytree(DIST / "pkg", site / "pkg")
    shutil.copytree(DIST_ST / "pkg", site / "pkg-st")
    fs = site / "fs"
    fs.mkdir()
    for path in (DIST / "fs").iterdir():
        if path.is_file():
            shutil.copy2(path, fs / path.name)
    # The files fetched one by one: a packed folder's come with its pack.
    res, count = DIST / "fs" / "res", 0
    for line in (fs / "manifest.txt").read_text().splitlines():
        rel, *rest = line.split("\t")
        if len(rest) > 1:
            continue
        (fs / "res" / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(res / rel, fs / "res" / rel)
        count += 1
    files = ("pkg/horizon-client_bg.wasm", "pkg-st/horizon-client_bg.wasm", "fs/boot.tar.gz")
    (site / "build.json").write_text("{" + ", ".join(f'"{f}": "{digest(site / f)}"' for f in files) + "}\n")
    size = sum(p.stat().st_size for p in site.rglob("*") if p.is_file())
    print(f"{site}: {size / 1e6:.0f} MB ({count} files fetched when read)")


def held(rel: str, sounds: str, languages: bool) -> bool:
    """Whether the offline file holds a resource the web folder fetches when read."""
    parts = rel.split("/")
    if parts[:3] == ["assets", "minecraft", "sounds"]:
        return sounds == "all" or (sounds == "effects" and parts[3] not in ("music", "records"))
    if parts[:3] == ["assets", "minecraft", "lang"]:
        return languages
    return True


def gz(data: bytes) -> bytes:
    return gzip.compress(data, compresslevel=9, mtime=0)


def tar_bytes(files: list[tuple[str, Path]]) -> bytes:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.GNU_FORMAT) as tar:
        for rel, path in files:
            tar.add(path, arcname=rel, recursive=False)
    return raw.getvalue()


def data_url(js: str) -> str:
    return "data:text/javascript;base64," + base64.b64encode(js.encode()).decode()


def glue_url(pkg: Path) -> str:
    """The game's glue as one data: URL module, its snippets inlined as data: URLs."""
    glue = (pkg / "horizon-client.js").read_text()
    snippets = {}

    def inline(m: re.Match) -> str:
        rel = m.group(2)
        snippets.setdefault(rel, data_url((pkg / rel).read_text()))
        return f"from {m.group(1)}{snippets[rel]}{m.group(1)}"

    # Only the imports: the module's import names keep the paths.
    glue = re.sub(r"""from\s*(['"])\./(snippets/[^'"]+\.js)\1""", inline, glue)
    return data_url(glue)


OFFLINE_LOADER = """
// The offline page's files: base64 in the script elements below the page,
// each read once and dropped from the page.
function held(id) {
    const element = document.getElementById(id);
    const text = element.textContent;
    element.remove();
    if (Uint8Array.fromBase64) return Uint8Array.fromBase64(text);
    const binary = atob(text);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
}

async function gunzip(bytes) {
    return new Uint8Array(await new Response(
        new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"))).arrayBuffer());
}

globalThis.horizonOffline = async () => {
    status.textContent = "Loading the game…";
    const glue = await import(GLUE);
    // Runs the game's main, which only registers the game to start.
    await glue.default({ module_or_path: await gunzip(held("horizon-wasm")) });
    status.textContent = "Unpacking resources…";
    const tar = await gunzip(held("horizon-boot"));
    // The files the game reads later: one block, cut by the manifest's lengths.
    const store = await gunzip(held("horizon-store"));
    const manifest = document.getElementById("horizon-manifest").textContent;
    const index = new Map();
    let at = 0;
    for (const line of manifest.split("\\n")) {
        if (!line) continue;
        const [rel, length] = line.split("\\t");
        index.set(rel, [at, Number(length)]);
        at += Number(length);
    }
    // horizon-os's horizon_fetch_sync asks here before it fetches.
    globalThis.horizonHeldFile = (url) => {
        const file = url.startsWith("held:") && index.get(url.slice(5));
        return file ? store.subarray(file[0], file[0] + file[1]) : undefined;
    };
    return { glue, tar, manifest, base: "held:" };
};
"""


def offline(out: Path, resources: Path, sounds: str, languages: bool) -> None:
    boot, store = [], []
    for dirpath, _, names in os.walk(resources):
        for name in names:
            path = Path(dirpath) / name
            rel = path.relative_to(resources).as_posix()
            if not bundle.lazy(rel) or rel.split("/")[0] in bundle.PACKED_DIRS:
                boot.append((rel, path))
            elif held(rel, sounds, languages):
                store.append((rel, path))
    boot.sort()
    store.sort()
    manifest = "".join(f"{rel}\t{path.stat().st_size}\n" for rel, path in store)

    def block(id: str, data: bytes) -> str:
        return f'<script type="application/octet-stream" id="{id}">{base64.b64encode(data).decode()}</script>\n'

    pkg = DIST_ST / "pkg"
    page = (WEB / "index.html").read_text()
    main = (WEB / "main.js").read_text()
    if 'import "./audio.js";' not in main:
        sys.exit("main.js no longer imports audio.js as package.py expects")
    main = main.replace('import "./audio.js";', "")
    script = f"<script>\n{(WEB / 'audio.js').read_text()}</script>\n" + \
        f'<script type="module">\nconst GLUE = "{glue_url(pkg)}";\n{OFFLINE_LOADER}\n{main}</script>\n'
    tag = '<script type="module" src="main.js"></script>\n'
    if tag not in page:
        sys.exit("index.html no longer loads main.js as package.py expects")
    head, tail = page.split(tag)
    target = out / "horizon-offline.html"
    with open(target, "w") as f:
        f.write(head)
        f.write(block("horizon-wasm", gz((pkg / "horizon-client_bg.wasm").read_bytes())))
        f.write(block("horizon-boot", gz(tar_bytes(boot))))
        f.write(block("horizon-store", gz(b"".join(path.read_bytes() for _, path in store))))
        f.write(f'<script type="text/plain" id="horizon-manifest">{manifest}</script>\n')
        f.write(script)
        f.write(tail)
    print(f"{target}: {target.stat().st_size / 1e6:.0f} MB ({len(boot)} files, {len(store)} held for later)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("what", nargs="?", choices=("folder", "offline", "all"), default="all")
    parser.add_argument("--no-build", action="store_true", help="package what web/dist and web/dist-st hold")
    parser.add_argument("--sounds", choices=("none", "effects", "all"), default="effects")
    parser.add_argument("--languages", action="store_true", help="every language, not only English")
    parser.add_argument("--out", type=Path, default=WEB / "out")
    args = parser.parse_args()
    resources = REPO / os.environ.get("HORIZON_RESOURCES", ".local/horizon-client/26.3")
    if not args.no_build:
        if args.what != "offline":
            build(threaded=True, resources=True)
        build(threaded=False, resources=False)
    args.out.mkdir(parents=True, exist_ok=True)
    if args.what != "offline":
        folder(args.out)
    if args.what != "folder":
        offline(args.out, resources, args.sounds, args.languages)


if __name__ == "__main__":
    main()
