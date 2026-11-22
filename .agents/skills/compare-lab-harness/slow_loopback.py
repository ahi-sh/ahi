"""Explicit slow-transport regression. Both ends are loopback; never a lab model."""

import argparse
import json
import os
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

from probe import HERE, load_case, scripted


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--client", choices=["ahi", "pi"], required=True)
    p.add_argument("--binary", required=True, type=Path)
    p.add_argument(
        "--delay", type=int, default=126, help="seconds spent in the first request"
    )
    p.add_argument(
        "--timeout",
        type=int,
        default=180,
        help="whole turn; set below delay to test timeout",
    )
    a = p.parse_args()
    if not 1 <= a.delay <= 300 or not 1 <= a.timeout <= 3600 or a.timeout == a.delay:
        p.error("expected delay 1–300, timeout 1–3600, different values")
    case = load_case(HERE / "cases/base.json")
    count = 0
    stop = threading.Event()

    class Stub(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            nonlocal count
            self.rfile.read(int(self.headers["Content-Length"]))
            count += 1
            if count == 1 and stop.wait(a.delay / 2):
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            if count == 1:
                self.wfile.write(b": heartbeat\n\n")
                self.wfile.flush()
                if stop.wait(a.delay / 2):
                    return
            reply = scripted(case, a.client, count, "fixture", False)
            if count == 2:
                # A reported cache miss is different from omitted usage on request 1.
                usage = {
                    "choices": [],
                    "usage": {
                        "prompt_tokens": 200,
                        "completion_tokens": 8,
                        "prompt_tokens_details": {"cached_tokens": 0},
                    },
                }
                reply = reply.replace(
                    b"data: [DONE]",
                    f"data: {json.dumps(usage)}\n\ndata: [DONE]".encode(),
                )
            self.wfile.write(reply)

    with HTTPServer(("127.0.0.1", 0), Stub) as stub:
        thread = threading.Thread(target=stub.serve_forever, daemon=True)
        thread.start()
        try:
            env = os.environ.copy()
            env.pop("HILLCLIMB_API_KEY", None)
            start = time.monotonic()
            proc = subprocess.run(
                [
                    sys.executable,
                    HERE / "probe.py",
                    "--client",
                    a.client,
                    "--binary",
                    a.binary,
                    "--live-url",
                    f"http://127.0.0.1:{stub.server_port}/v1",
                    "--model",
                    "fixture",
                    "--route",
                    "loopback-stub",
                    "--disposable-host",
                    "--timeout",
                    str(a.timeout),
                ],
                env=env,
                capture_output=True,
                text=True,
                timeout=a.timeout + 12,
            )
            elapsed = time.monotonic() - start
            row = json.loads(proc.stdout)
            if a.delay < a.timeout:
                if (
                    proc.returncode
                    or not row["pass"]
                    or count != 2
                    or elapsed < a.delay
                    or [u["cached_prompt_tokens"] for u in row["request_usage"]]
                    != [None, 0]
                ):
                    raise SystemExit(row)
            elif (
                proc.returncode != 1
                or row["pass"]
                or not row["errors"]
                or count != 1
                or elapsed >= a.timeout + 10
            ):
                raise SystemExit(row)
            print(
                f"{a.client}: {count} loopback request(s), delay {a.delay}s, turn budget {a.timeout}s, task pass={row['pass']}"
            )
        finally:
            stop.set()
            stub.shutdown()
            thread.join()


if __name__ == "__main__":
    main()
