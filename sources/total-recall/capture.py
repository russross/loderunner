#!/usr/bin/env python3
"""Observe a local Total Recall demo in an isolated Chrome session."""

import argparse
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit


class CaptureServer(http.server.HTTPServer):
    result: bytes | None = None


class Handler(http.server.SimpleHTTPRequestHandler):
    server: CaptureServer

    # The upstream checkout is served read-only; instrumentation is a separate script.
    def do_GET(self) -> None:
        route = urlsplit(self.path).path
        if route == "/observer.js":
            body = Path(__file__).with_name("observer.js").read_bytes()
            content_type = "text/javascript"
        elif route == "/capture.html":
            assert self.directory is not None
            body = (Path(self.directory) / "lodeRunner.html").read_bytes()
            body = body.replace(b'onload="init();"', b'onload="captureInit();"')
            body = body.replace(b"</head>", b'<script src="/observer.js"></script></head>')
            content_type = "text/html"
        else:
            super().do_GET()
            return
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    # Only the instrumented local page submits evidence to this loopback server.
    def do_POST(self) -> None:
        if self.path != "/capture":
            self.send_error(404)
            return
        size = int(self.headers.get("Content-Length", "0"))
        self.server.result = self.rfile.read(size)
        self.send_response(204)
        self.end_headers()

    def log_message(self, format: str, *args: object) -> None:
        if len(args) > 1 and str(args[1]) not in ("200", "204", "304"):
            super().log_message(format, *args)


def capture(source: Path, level: int, output: Path, headless: bool) -> None:
    chrome = shutil.which("google-chrome") or shutil.which("chromium")
    if chrome is None:
        raise ValueError("Chrome or Chromium is required")
    if not (source / "lodeRunner.html").is_file():
        raise ValueError(f"{source}: Total Recall checkout not found")
    if output.exists():
        raise ValueError(f"{output}: destination already exists")
    handler = functools.partial(Handler, directory=str(source))
    with CaptureServer(("127.0.0.1", 0), handler) as server:
        port = server.server_port
        # A new profile prevents Chrome from reusing the player's ordinary session.
        with tempfile.TemporaryDirectory(prefix="lode-recall-chrome-") as profile:
            command = [chrome, f"--user-data-dir={profile}", "--no-first-run", "--no-default-browser-check", "--disable-background-networking", "--disable-dev-shm-usage", f"http://127.0.0.1:{port}/capture.html?level={level}"]
            if headless or not os.environ.get("DISPLAY"):
                command.append("--headless=new")
            with tempfile.TemporaryFile(mode="w+") as log:
                browser = subprocess.Popen(command, stdout=log, stderr=log, start_new_session=True)
                try:
                    # Preloading and observation finish by posting a single result.
                    server.timeout = 0.25
                    deadline = time.monotonic() + 45
                    while server.result is None and time.monotonic() < deadline:
                        if browser.poll() is not None:
                            raise RuntimeError("Chrome exited before returning a capture")
                        server.handle_request()
                    if server.result is None:
                        log.seek(0)
                        raise RuntimeError(f"capture timed out; Chrome output:\n{log.read()[-4000:]}")
                    # Failed observations do not create a misleading capture artifact.
                    result = json.loads(server.result)
                    if "error" in result:
                        raise RuntimeError(result["error"])
                    revision = subprocess.run(["git", "-C", str(source), "rev-parse", "HEAD"], capture_output=True, text=True, check=False)
                    result["sourceRevision"] = revision.stdout.strip() if revision.returncode == 0 else None
                    output.parent.mkdir(parents=True, exist_ok=True)
                    output.write_text(json.dumps(result, indent=2) + "\n")
                    print(f"Level {level}: {len(result['ticks'])} ticks, state {result['final']['state']}; {output}")
                finally:
                    # Signal the complete process group even if the browser exited first.
                    try:
                        os.killpg(browser.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        browser.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(browser.pid, signal.SIGKILL)
                        browser.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--level", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--headless", action="store_true")
    args = parser.parse_args()
    try:
        capture(args.source.resolve(), args.level, args.output, args.headless)
    except (OSError, ValueError, RuntimeError) as error:
        print(f"capture: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
