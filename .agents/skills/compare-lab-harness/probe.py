"""One headless harness turn on synthetic files. Stdout: one portable receipt."""

import argparse
import hashlib
import http.client
import json
import os
import re
import secrets
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from contextlib import closing
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from urllib.parse import urlsplit

HERE = Path(__file__).resolve().parent
ROUNDS, TOKENS, SECONDS = 5, 768, 90
READ = {"ahi": "read_file", "pi": "read"}
WRITE = {"ahi": "write_file", "pi": "write"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load_case(path):
    c = json.loads(path.read_bytes())
    required = {"input", "before", "prompt", "marker", "answer"}
    if (
        not required <= c.keys() <= required | {"after"}
        or not all(
            isinstance(v, str) and 0 < len(v.encode()) <= 16384 for v in c.values()
        )
        or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,79}", c["input"])
        or c["input"] == "keep.txt"
        or not c["marker"].strip()
        or c["marker"] not in c["before"].strip()
        or c.get("after") == c["before"]
    ):
        raise ValueError("invalid case: see SKILL.md")
    return c


def result_contains(messages, name, matches, needle):
    """Match provider-visible result content to the preceding structured call ID."""
    calls = set()
    for m in messages:
        for t in m.get("tool_calls", []):
            f = t["function"]
            if f["name"] == name:
                try:
                    args = json.loads(f["arguments"])
                except (ValueError, AttributeError):
                    continue
                if isinstance(args, dict) and matches(args):
                    calls.add(t["id"])
        if m["role"] == "tool" and m.get("tool_call_id") in calls:
            content = m["content"]
            if isinstance(content, list):
                content = "".join(p.get("text", "") for p in content)
            if needle in content:
                return True
    return False


def scripted(case, client, turn, model, check):
    call = None
    if turn == 1:
        call = READ[client], {"path": case["input"]}
    elif turn == 2 and "after" in case:
        args = {"path": case["input"], "content": case["after"]}
        if client == "ahi":
            args["overwrite"] = True
        call = WRITE[client], args
    elif check and turn == (3 if "after" in case else 2):
        call = (
            ("bash", {"command": "check-fixture"})
            if client == "pi"
            else ("run_command", {"argv": ["check-fixture"]})
        )
    if call:
        fn = {"name": call[0], "arguments": json.dumps(call[1])}
        delta = {
            "tool_calls": [
                {"index": 0, "type": "function", "id": f"call_{turn}", "function": fn}
            ]
        }
        finish = "tool_calls"
    else:
        delta, finish = {"content": case["answer"]}, "stop"
    chunks = [
        {
            "id": f"mock-{turn}",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": model,
            "choices": [{"index": 0, "delta": d, "finish_reason": stop}],
        }
        for d, stop in [(dict(role="assistant", **delta), None), ({}, finish)]
    ]
    return (
        "".join(f"data: {json.dumps(ch)}\n\n" for ch in chunks) + "data: [DONE]\n\n"
    ).encode()


def relay(url, raw):
    cls = (
        http.client.HTTPSConnection
        if url.scheme == "https"
        else http.client.HTTPConnection
    )
    headers = {"Content-Type": "application/json"}
    if key := os.environ.get("HILLCLIMB_API_KEY"):
        headers["Authorization"] = "Bearer " + key
    with closing(cls(url.hostname, url.port, timeout=25)) as conn:
        conn.connect()
        sock = conn.sock

        def interrupt():
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

        deadline = threading.Timer(25, interrupt)
        deadline.start()
        try:
            conn.request(
                "POST", url.path.rstrip("/") + "/chat/completions", raw, headers
            )
            res = conn.getresponse()
            if res.status != 200:
                raise ValueError(f"upstream HTTP {res.status}")
            data = res.read(2**20 + 1)
            if len(data) > 2**20 or b"data: " not in data:
                raise ValueError("expected bounded SSE reply")
            return data
        finally:
            deadline.cancel()


def run(args, case, url, root, binary):
    work, state = root / "work", root / "state"
    work.mkdir()
    state.mkdir()
    source = work / case["input"]
    source.write_bytes(case["before"].encode())
    guard = work / "keep.txt"
    guard.write_bytes(b"unrelated\n")
    client = args.client
    names = [READ[client]] + ([WRITE[client]] if "after" in case else [])
    checker_hash = None
    check_marker = secrets.token_hex(16)
    check_status = root / "check.status"
    bin_dir = root / "bin"
    bin_dir.mkdir()
    prompt = (
        case["prompt"] + " Reply with just the requested identifier, no explanation."
    )
    if args.checker:
        # Explicit operator-supplied checker, never a path from the model or case.
        checker = root / "checker.sh"
        checker.write_bytes(args.checker.read_bytes())
        checker_hash = digest(checker.read_bytes())
        wrapper = bin_dir / "check-fixture"
        wrapper.write_text(
            "#!/bin/sh\nset -u\ncd "
            + shlex.quote(str(work))
            + " || exit 2\n"
            + "/bin/sh "
            + shlex.quote(str(checker))
            + "\nrc=$?\n"
            + 'echo "$rc" >'
            + shlex.quote(str(check_status))
            + "\n"
            + 'echo "'
            + check_marker
            + ':$rc"\nexit "$rc"\n'
        )
        wrapper.chmod(0o700)
        prompt += " Before answering, run check-fixture with no arguments and make sure it passes."
        names.append("bash" if client == "pi" else "run_command")
    seen, errors = [], []
    submitted = set()
    nonce = secrets.token_urlsafe(24)
    read = check_result = False

    class Proxy(BaseHTTPRequestHandler):
        timeout = 5

        def log_message(self, *args):
            pass

        def do_POST(self):
            nonlocal read, check_result
            try:
                if not secrets.compare_digest(
                    self.headers.get("Authorization", ""), "Bearer " + nonce
                ):
                    self.send_error(403)
                    return
                size = int(self.headers.get("Content-Length", 0))
                if (
                    self.path != "/v1/chat/completions"
                    or not 0 < size <= 2**20
                    or errors
                    or len(seen) >= args.rounds
                ):
                    raise ValueError("unexpected request or round budget exceeded")
                raw = self.rfile.read(size)
                body = json.loads(raw)
                advertised = sorted(
                    t["function"]["name"] for t in body.get("tools", [])
                )
                if (
                    body["model"] != args.model
                    or body.get("stream") is not True
                    or advertised != sorted(names)
                    or body.get("reasoning_effort")
                    != (None if args.thinking == "off" else args.thinking)
                    or body.get("max_completion_tokens", body.get("max_tokens"))
                    != args.max_tokens
                ):
                    raise ValueError("wire model/tools/stream/output cap mismatch")
                fingerprint = digest(raw)
                if fingerprint in submitted:
                    raise ValueError("duplicate request; no replay")
                submitted.add(fingerprint)
                seen.append(body.get("reasoning_effort"))
                messages = body["messages"]
                read |= result_contains(
                    messages,
                    READ[client],
                    lambda a: (
                        a.get("path")
                        in (case["input"], "./" + case["input"], str(source))
                    ),
                    case["before"].strip(),
                )
                if args.checker:
                    check_result |= result_contains(
                        messages,
                        "bash" if client == "pi" else "run_command",
                        lambda a: (
                            a.get("command") == "check-fixture"
                            if client == "pi"
                            else a.get("argv") == ["check-fixture"]
                        ),
                        check_marker + ":0",
                    )
                reply = (
                    relay(url, raw)
                    if url
                    else scripted(
                        case,
                        client,
                        None if args.answer_only else len(seen),
                        args.model,
                        args.checker,
                    )
                )
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                self.wfile.write(reply)
            except (ValueError, OSError, KeyError, http.client.HTTPException) as exc:
                # Never put upstream bodies, keys, model output or local paths in receipts.
                errors.append(
                    str(exc)
                    if type(exc) is ValueError
                    and not isinstance(exc, json.JSONDecodeError)
                    else type(exc).__name__
                )
                self.send_error(400, "probe refused request")

    env = dict(
        HOME=str(root),
        XDG_CONFIG_HOME=str(root / "config"),
        XDG_DATA_HOME=str(root / "data"),
        XDG_STATE_HOME=str(state),
        PI_CODING_AGENT_DIR=str(state),
        PI_OFFLINE="1",
        TRIAL_NONCE=nonce,
        PATH=os.defpath,
    )
    if node := shutil.which("node"):
        env["PATH"] = str(Path(node).parent) + os.pathsep + os.defpath
    env["PATH"] = str(bin_dir) + os.pathsep + env["PATH"]
    version = subprocess.run(
        [binary, "--version"],
        cwd=work,
        env=env,
        capture_output=True,
        text=True,
        timeout=10,
        check=True,
    ).stdout.strip()
    with HTTPServer(("127.0.0.1", 0), Proxy) as server:
        catalog = {
            "providers": {
                "bench": {
                    "api": "openai-completions",
                    "apiKey": "$TRIAL_NONCE",
                    "baseUrl": f"http://127.0.0.1:{server.server_port}/v1",
                    "models": [
                        {
                            "id": args.model,
                            "reasoning": args.thinking != "off",
                            "compat": {"supportsReasoningEffort": True},
                            "contextWindow": 32768,
                            "maxTokens": args.max_tokens,
                        }
                    ],
                }
            }
        }
        (state / "models.json").write_text(json.dumps(catalog))
        if client == "pi":
            cmd = [
                "--offline",
                "--no-extensions",
                "--no-context-files",
                "--no-mcp",
                "--no-skills",
                "--no-prompt-templates",
                "--no-themes",
                "--no-approve",
                "--no-session",
                "--tools",
                ",".join(names),
            ]
        else:
            rules = [
                {
                    "id": "read",
                    "effect": "approve",
                    "action": "tool_call",
                    "match": {"tool": READ[client]},
                }
            ]
            if "after" in case:
                rules.append(
                    {
                        "id": "write",
                        "effect": "approve",
                        "action": "file_write",
                        "match": {"path": case["input"], "tool": WRITE[client]},
                    }
                )
            if args.checker:
                rules.append(
                    {
                        "id": "check",
                        "effect": "approve",
                        "action": "command",
                        "match": {"argv": ["check-fixture"]},
                    }
                )
            policy = root / "policy.yaml"
            policy.write_text(
                json.dumps({"version": 1, "default": "deny", "rules": rules})
            )
            cmd = [
                "--state-dir",
                str(state),
                "exec",
                "--standalone",
                "--format",
                "json",
                "--tools",
                str(policy),
            ]
        cmd += [
            "--model",
            "bench/" + args.model,
            "--thinking",
            args.thinking,
            "-p",
            prompt,
        ]
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        started_ns = time.time_ns()
        start = time.monotonic()
        try:
            child = subprocess.Popen(
                [binary, *cmd],
                cwd=work,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            try:
                output, _ = child.communicate(timeout=SECONDS)
            except BaseException:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait()
                raise
            elapsed = time.monotonic() - start
            exit_code = child.returncode
            answer = output.decode(errors="replace")
            if client == "ahi":
                result = json.loads(answer or "{}")
                answer = result.get("text", "") if result.get("status") == "ok" else ""
        except subprocess.TimeoutExpired:
            elapsed, exit_code, answer = time.monotonic() - start, 124, ""
            errors.append("turn timeout")
        finally:
            server.shutdown()
            thread.join()

    def matches(path, data):
        return (
            not path.is_symlink()
            and path.is_file()
            and path.stat().st_size == len(data)
            and path.read_bytes() == data
        )

    disk = (
        set(work.iterdir()) == {source, guard}
        and matches(guard, b"unrelated\n")
        and matches(source, case.get("after", case["before"]).encode())
    )
    gates = {
        "read_result": read,
        "disk": disk,
        "answer": case["answer"] == answer.strip(),
        "completed": exit_code == 0,
    }
    checker_exit = None
    if args.checker:
        if (
            not check_status.is_symlink()
            and check_status.is_file()
            and check_status.stat().st_size <= 4
        ):
            raw_status = check_status.read_bytes()
            if re.fullmatch(rb"[0-9]{1,3}\n", raw_status):
                checker_exit = int(raw_status)
        gates["check_process"] = checker_exit == 0 and check_result
    return {
        "schema": "harness-trial/1",
        "id": str(uuid.uuid4()),
        "started_ns": started_ns,
        "mode": "live" if url else "scripted",
        "client": client,
        "version": version,
        "launcher_sha256": digest(binary.read_bytes()),
        "model": args.model,
        "route": args.route,
        "endpoint_sha256": digest(args.live_url.encode()) if url else None,
        "case_sha256": digest(json.dumps(case, sort_keys=True).encode()),
        "driver_sha256": digest(Path(__file__).read_bytes()),
        "case": args.case.stem,
        "checker_sha256": checker_hash,
        "checker_exit": checker_exit,
        "thinking": args.thinking,
        "limits": [args.rounds, args.max_tokens, SECONDS],
        "tools": names,
        "wire_effort": seen,
        "requests": len(seen),
        "elapsed_s": round(elapsed, 4),
        "exit": exit_code,
        "gates": gates,
        "errors": errors[:2],
        "pass": all(gates.values()) and bool(seen) and not errors,
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--client", required=True, choices=READ)
    p.add_argument(
        "--binary", help="path to real ahi or pi executable, not a version-manager shim"
    )
    p.add_argument(
        "--case",
        type=Path,
        default=HERE / "cases/base.json",
        help="trusted synthetic case JSON",
    )
    p.add_argument(
        "--thinking", choices=["off", "low", "medium", "high"], default="off"
    )
    p.add_argument(
        "--rounds", type=int, default=ROUNDS, help="maximum provider requests, 1–10"
    )
    p.add_argument(
        "--max-tokens",
        type=int,
        default=TOKENS,
        help="wire output budget per request, 1–4096",
    )
    p.add_argument(
        "--checker",
        type=Path,
        help="trusted POSIX sh test to expose as check-fixture; enables command tool",
    )
    p.add_argument("--model", help="exact served ID; required for live trials")
    mode = p.add_mutually_exclusive_group()
    mode.add_argument(
        "--answer-only",
        action="store_true",
        help="scripted negative control: correct prose but no tool requests",
    )
    mode.add_argument(
        "--live-url",
        help="opt in to one turn against this existing Chat Completions base URL",
    )
    p.add_argument(
        "--disposable-host",
        action="store_true",
        help="confirm that live file tools run in a disposable VM/container",
    )
    p.add_argument(
        "--route", help="non-secret serving manifest ID (required with --live-url)"
    )
    args = p.parse_args()
    if not 1 <= args.rounds <= 10 or not 1 <= args.max_tokens <= 4096:
        p.error("expected 1–10 rounds and 1–4096 tokens/request")
    url = urlsplit(args.live_url) if args.live_url else None
    if url and (
        not args.route
        or not args.model
        or url.scheme not in ("http", "https")
        or not url.hostname
        or url.username is not None
        or url.password is not None
        or url.query
        or url.fragment
    ):
        p.error(
            "live requires --route, --model and an http(s) base URL without credentials, query or fragment"
        )
    if url and not args.disposable_host:
        p.error(
            "live trials require --disposable-host; temp directories are not containment"
        )
    if url and url.scheme != "https" and os.environ.get("HILLCLIMB_API_KEY"):
        p.error("use HTTPS for an authenticated upstream")
    args.model = args.model or "scripted"
    args.route = args.route or "loopback"
    binary = shutil.which(args.binary or args.client)
    if not binary:
        p.error("client missing; specify --binary")
    with tempfile.TemporaryDirectory(prefix="harness-trial-") as tmp:
        row = run(args, load_case(args.case), url, Path(tmp), Path(binary).absolute())
    print(json.dumps(row, allow_nan=False))
    return 0 if row["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
