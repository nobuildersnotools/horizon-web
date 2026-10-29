// Loads the game, installs the prepared resources and the player's saved
// files in its virtual filesystem, then starts it and keeps saving the
// player's files in the browser's storage (OPFS).
import init, {
    horizon_install_manifest,
    horizon_install_saved_dir,
    horizon_install_saved_file,
    horizon_install_tar,
    horizon_start,
    horizon_take_changes,
} from "./pkg/horizon-client.js";

const ROOT = ".local/horizon-client/26.3";
const status = document.getElementById("status");

async function download(url, label) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
    const total = Number(response.headers.get("Content-Length")) || 0;
    let received = 0;
    const counted = response.body.pipeThrough(new TransformStream({
        transform(chunk, controller) {
            received += chunk.length;
            if (total) status.textContent = `${label} ${Math.round((received / total) * 100)}%`;
            controller.enqueue(chunk);
        },
    }));
    return counted;
}

// The player's files (options, saves) live under this folder of the
// origin's private file system, laid out as in the virtual filesystem.
async function savedRoot() {
    try {
        return await (await navigator.storage.getDirectory()).getDirectoryHandle("horizon", { create: true });
    } catch (e) {
        console.warn(`Horizon: no browser storage, nothing will be saved: ${e}`);
        return null;
    }
}

async function loadSaved(dir, path) {
    let files = 0;
    for await (const [name, handle] of dir.entries()) {
        const at = `${path}/${name}`;
        if (handle.kind === "directory") {
            horizon_install_saved_dir(at);
            files += await loadSaved(handle, at);
        } else {
            const file = await handle.getFile();
            horizon_install_saved_file(at, new Uint8Array(await file.arrayBuffer()), file.lastModified);
            files += 1;
        }
    }
    return files;
}

// The folder holding `parts`, made when `create`; null when it's missing.
async function folder(root, parts, create) {
    let dir = root;
    for (const part of parts) {
        try {
            dir = await dir.getDirectoryHandle(part, { create });
        } catch (e) {
            if (create && e.name === "TypeMismatchError") {
                // A file where a folder goes now.
                await dir.removeEntry(part);
                dir = await dir.getDirectoryHandle(part, { create });
            } else if (e.name === "NotFoundError") {
                return null;
            } else {
                throw e;
            }
        }
    }
    return dir;
}

async function apply(root, path, kind, data) {
    const parts = path.split("/").filter(Boolean);
    if (kind === "gone") {
        const dir = await folder(root, parts.slice(0, -1), false);
        await dir?.removeEntry(parts.at(-1), { recursive: true }).catch((e) => {
            if (e.name !== "NotFoundError") throw e;
        });
    } else if (kind === "file") {
        const dir = await folder(root, parts.slice(0, -1), true);
        const name = parts.at(-1);
        let handle;
        try {
            handle = await dir.getFileHandle(name, { create: true });
        } catch (e) {
            if (e.name !== "TypeMismatchError") throw e;
            // A folder where a file goes now.
            await dir.removeEntry(name, { recursive: true });
            handle = await dir.getFileHandle(name, { create: true });
        }
        // Written to a scratch file and swapped in on close: never half a file.
        const writable = await handle.createWritable();
        await writable.write(data);
        await writable.close();
    } else {
        if (kind === "cleardir") {
            const dir = await folder(root, parts.slice(0, -1), false);
            await dir?.removeEntry(parts.at(-1), { recursive: true }).catch(() => {});
        }
        await folder(root, parts, true);
    }
}

// Saves what the game changed since the last time, one batch at a time.
let saving = null;
function saveChanges(root) {
    saving ??= (async () => {
        for (const [path, kind, data] of horizon_take_changes()) {
            try {
                await apply(root, path, kind, data);
            } catch (e) {
                console.error(`Horizon: could not save ${path}: ${e}`);
            }
        }
    })().finally(() => {
        saving = null;
    });
    return saving;
}

async function main() {
    if (!navigator.gpu) {
        status.textContent = "Horizon needs WebGPU, which this browser doesn't offer.";
        return;
    }
    status.textContent = "Loading the game…";
    // Runs the game's main, which only registers the game to start.
    await init();

    const t0 = performance.now();
    const gz = await download("fs/boot.tar.gz", "Downloading resources");
    const tar = new Uint8Array(
        await new Response(gz.pipeThrough(new DecompressionStream("gzip"))).arrayBuffer());
    status.textContent = "Installing resources…";
    const files = horizon_install_tar(ROOT, tar);
    const manifest = await (await fetch("fs/manifest.txt")).text();
    horizon_install_manifest(ROOT, "fs/res/", manifest);
    console.log(`Horizon: ${files} files installed in ${Math.round(performance.now() - t0)} ms`);

    const root = await savedRoot();
    if (root) {
        const t1 = performance.now();
        const saved = await loadSaved(root, "");
        console.log(`Horizon: ${saved} saved files loaded in ${Math.round(performance.now() - t1)} ms`);
        setInterval(() => saveChanges(root), 2000);
        document.addEventListener("visibilitychange", () => {
            if (document.hidden) saveChanges(root);
        });
        addEventListener("pagehide", () => saveChanges(root));
    }

    status.textContent = "Loading resources…";
    // Let the status paint before the game blocks the page while it loads.
    await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
    status.textContent = "";
    // The game's command-line arguments, from the address: ?args=--world%20T
    const args = (new URLSearchParams(location.search).get("args") ?? "").split(" ").filter(Boolean);
    try {
        horizon_start(args);
    } catch (e) {
        // winit leaves Rust's stack by throwing once its event loop is set up.
        if (!String(e).includes("Using exceptions for control flow")) throw e;
    }
}

main().catch((e) => {
    console.error(e);
    status.textContent = `Horizon could not start:\n${e}`;
});
