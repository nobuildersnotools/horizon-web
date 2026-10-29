// Loads the game, installs the prepared resources in its virtual filesystem,
// then starts it.
import init, {
    horizon_install_manifest,
    horizon_install_tar,
    horizon_start,
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

    status.textContent = "Loading resources…";
    // Let the status paint before the game blocks the page while it loads.
    await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
    status.textContent = "";
    try {
        horizon_start();
    } catch (e) {
        // winit leaves Rust's stack by throwing once its event loop is set up.
        if (!String(e).includes("Using exceptions for control flow")) throw e;
    }
}

main().catch((e) => {
    console.error(e);
    status.textContent = `Horizon could not start:\n${e}`;
});
