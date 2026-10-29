#!/usr/bin/env python3
"""Serves web/dist on http://localhost:8080 for testing.

The headers make the page cross-origin isolated, which the browser requires
before it lets a page share memory between threads (needed once the game's
workers run in Web Workers).
"""

import functools
import http.server
import sys
from pathlib import Path


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
    }

    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()

    def log_message(self, format, *args):
        pass


def main() -> None:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    root = Path(__file__).resolve().parent / "dist"
    handler = functools.partial(Handler, directory=str(root))
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as server:
        print(f"Serving {root} on http://localhost:{port}")
        server.serve_forever()


if __name__ == "__main__":
    main()
