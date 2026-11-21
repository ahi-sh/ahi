#!/usr/bin/env python3
"""Run the scratch worker with a disposable Rex server, no GUI, and fake inference."""

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    ahi = str(p.parse_args().binary.resolve(strict=True))
    rex = shutil.which("rex")
    if not rex:
        p.error("install Rex first")
    requests = []
    in_flight, release = threading.Event(), threading.Event()

    class Gateway(BaseHTTPRequestHandler):
        def do_POST(self):
            requests.append(
                json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            )
            messages = requests[-1]["messages"]
            last = messages[-1]

            def tool(name, args):
                return {
                    "tool_calls": [
                        {
                            "index": 0,
                            "id": "call_fixture",
                            "type": "function",
                            "function": {"name": name, "arguments": json.dumps(args)},
                        }
                    ]
                }

            if last["content"] == "Pause for cancellation":
                in_flight.set()
                assert release.wait(10)
            if last["content"] == "Use the local child":
                delta = tool(
                    "delegate",
                    {
                        "local": {"profile": "rex-scratch", "mode": "plan"},
                        "prompt": "List scratch",
                        "follow_ups": ["Conclude"],
                    },
                )
            elif last["content"] == "Use named child":
                delta = tool(
                    "delegate",
                    {
                        "local": {"profile": "rex-scratch"},
                        "prompt": "Write allowed",
                        "follow_ups": ["Try denied"],
                    },
                )
            elif last["content"] in ("Write allowed", "Try denied"):
                delta = tool(
                    "write_file",
                    {
                        "path": "allowed.txt"
                        if last["content"] == "Write allowed"
                        else "denied.txt",
                        "content": "fixture",
                    },
                )
            elif last["content"] == "List scratch":
                delta = tool("list_dir", {})
            else:
                delta = {"content": "headless worker answered"}
            choices = [{"delta": delta}]
            choices.append(
                {
                    "delta": {},
                    "finish_reason": "tool_calls" if "tool_calls" in delta else "stop",
                }
            )
            data = (
                "".join(
                    "data: " + json.dumps({"choices": [dict(index=0, **c)]}) + "\n\n"
                    for c in choices
                )
                + "data: [DONE]\n\n"
            ).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    with (
        tempfile.TemporaryDirectory(prefix="ahi-rx-", dir="/tmp") as tmp,
        HTTPServer(("127.0.0.1", 0), Gateway) as gateway,
    ):
        root = Path(tmp).resolve()
        # No inherited REX_*, cloud credentials, user config, or live socket.
        env = {
            "HOME": str(root),
            "PATH": str(Path(rex).parent) + os.pathsep + "/usr/bin:/bin",
            "TMPDIR": str(root),
            "XDG_CONFIG_HOME": str(root / "config"),
            "XDG_STATE_HOME": str(root / "state"),
            "XDG_CACHE_HOME": str(root / "cache"),
            "XDG_DATA_HOME": str(root / "data"),
        }
        config = root / "empty.lua"
        config.touch()
        endpoint = "unix://" + str(root / "rex.sock")

        def call(*args):
            return subprocess.check_output(
                args, env=env, cwd=root, text=True, timeout=15
            )

        rx = [rex, "--autostart=false", "--server", endpoint]
        state = root / "ahi"
        state.mkdir()
        prompt = root / "prompt"
        prompt.write_text("List scratch")
        follow = root / "follow"
        follow.write_text("Conclude")

        def rpc(method, **params):
            with socket.socket(socket.AF_UNIX) as s:
                s.settimeout(10)
                s.connect(str(state / "run/serve.sock"))
                s.sendall(
                    (
                        json.dumps({"id": 1, "method": method, "params": params}) + "\n"
                    ).encode()
                )
                return json.loads(s.makefile("rb").readline())

        (state / "models.json").write_text(
            json.dumps(
                {
                    "providers": {
                        "fixture": {
                            "baseUrl": f"http://127.0.0.1:{gateway.server_port}/v1",
                            "api": "openai-completions",
                            "apiKey": "gateway",
                            "models": [{"id": "plain"}],
                        }
                    }
                }
            )
        )
        threading.Thread(target=gateway.serve_forever, daemon=True).start()
        with (root / "rex.log").open("w+") as log:
            server = subprocess.Popen(
                [
                    rex,
                    "server",
                    "run",
                    "--listen",
                    endpoint,
                    "--restore=false",
                    "--state-dir",
                    str(root / "rex-state"),
                    "--config",
                    str(config),
                ],
                env=env,
                cwd=root,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=log,
            )
            try:
                for _ in range(100):
                    if (root / "rex.sock").is_socket():
                        break
                    if server.poll() is not None:
                        raise RuntimeError((root / "rex.log").read_text()[-2000:])
                    time.sleep(0.05)
                session = json.loads(
                    call(
                        *rx,
                        "new",
                        "--json",
                        "--shell",
                        "none",
                        "--",
                        "/bin/sleep",
                        "30",
                    )
                )["session_id"]
                env.update(
                    AHI_DELEGATE_REX_SERVER=endpoint, AHI_DELEGATE_REX_SESSION=session
                )
                prefix = [ahi, "--state-dir", str(state), "delegate"]
                launched = json.loads(
                    call(
                        *prefix,
                        "launch",
                        "--model",
                        "fixture/plain",
                        "--server",
                        endpoint,
                        "--host-session",
                        session,
                        "--prompt-file",
                        str(prompt),
                        "--follow-up",
                        str(follow),
                    )
                )
                assignment = launched["assignment"]["id"]
                status = json.loads(call(*prefix, "status", assignment, "--wait", "10"))
                assert status["result"]["text"] == "headless worker answered", status
                assert (
                    status["claim"]["observed"]["rex_block"]
                    == status["assignment"]["phase"]["terminal"]["id"]
                ), status
                assert status["authority"]["mode"] == "plan" and len(requests) == 3, (
                    status,
                    requests,
                )
                assert [m["role"] for m in requests[2]["messages"]][-5:] == [
                    "user",
                    "assistant",
                    "tool",
                    "assistant",
                    "user",
                ], requests[2]
                assert (
                    rpc("child.finish", **status["result"])["error"]["kind"] == "policy"
                )
                parent = rpc(
                    "session.create", cwd=str(root), gateway="fixture", model="plain"
                )["result"]["id"]
                run = rpc(
                    "turn.submit",
                    session=parent,
                    mode="trusted",
                    prompt="Use the local child",
                    timeout=20,
                    unattended=True,
                )["result"]["run_id"]
                for _ in range(200):
                    result = rpc("turn.get", session=parent, run_id=run)["result"]
                    if result["status"] != "running":
                        break
                    time.sleep(0.05)
                assert result["status"] == "ok" and len(requests) == 8, (
                    result,
                    requests,
                )
                a = json.loads(requests[-1]["messages"][-1]["content"])
                child = json.loads(call(*prefix, "status", a["assignment"]))
                assert (
                    child["authority"]["parent_mode"] == "trusted"
                    and child["authority"]["mode"] == "plan"
                ), child
                assert child["authority"]["source"]["parent"] == {
                    "session": parent,
                    "run": run,
                }
                names = {t["function"]["name"] for t in requests[4]["tools"]}
                assert "list_dir" in names and not names.intersection(
                    {"delegate", "run_command", "write_file"}
                ), names
                assert (
                    a["status"] == "ok"
                    and child["result"]["text"] == "headless worker answered"
                    and child["result"]["turns_completed"] == 2
                )
                (root / "ahi.modes.yaml").write_text("""version: 2
default_mode: editor
modes:
  editor:
    allow:
      local_profiles:
        - rex-scratch
      files:
        - tools:
            - write_file
          paths:
            - allowed.txt
""")
                refused = subprocess.run(
                    [
                        *prefix,
                        "launch",
                        "--model",
                        "fixture/plain",
                        "--server",
                        endpoint,
                        "--host-session",
                        session,
                        "--prompt-file",
                        str(prompt),
                        "--mode",
                        "trusted",
                    ],
                    env=env,
                    cwd=root,
                    capture_output=True,
                )
                assert (
                    refused.returncode == 2 and b"cannot delegate" in refused.stderr
                ), refused
                assert len(list((state / "delegation/assignments").iterdir())) == 2
                second = rpc(
                    "session.create", cwd=str(root), gateway="fixture", model="plain"
                )["result"]["id"]
                second_run = rpc(
                    "turn.submit",
                    session=second,
                    mode="editor",
                    prompt="Use named child",
                    timeout=20,
                    unattended=True,
                )["result"]["run_id"]
                for _ in range(200):
                    res = rpc("turn.get", session=second, run_id=second_run)["result"]
                    if res["status"] != "running":
                        break
                    time.sleep(0.05)
                assert res["status"] == "ok", res
                a = json.loads(requests[-1]["messages"][-1]["content"])
                named = json.loads(call(*prefix, "status", a["assignment"]))
                assert (
                    named["authority"]["mode"] == "editor"
                    and named["result"]["turns_completed"] == 2
                ), named
                scratch = Path(named["assignment"]["scratch"])
                assert (scratch / "allowed.txt").read_text() == "fixture" and not (
                    scratch / "denied.txt"
                ).exists()
                prompt.write_text("Pause for cancellation")
                before = len(requests)
                pending = json.loads(
                    call(
                        *prefix,
                        "launch",
                        "--model",
                        "fixture/plain",
                        "--server",
                        endpoint,
                        "--host-session",
                        session,
                        "--prompt-file",
                        str(prompt),
                        "--follow-up",
                        str(follow),
                    )
                )["assignment"]["id"]
                assert in_flight.wait(5)
                call(*prefix, "cancel", pending)
                release.set()
                stopped = json.loads(call(*prefix, "status", pending, "--wait", "5"))
                assert (
                    stopped["result"]["status"] == "cancelled"
                    and len(requests) == before + 1
                ), stopped
                print(
                    "PASS: operator, Trusted→Plan, inherited named mode, in-flight cancel; 4 headless workers; multi-turn history, path restrictions, escalation refused, connection-bound finish"
                )
            finally:
                if (state / "run/serve.sock").is_socket():
                    call(ahi, "--state-dir", str(state), "serve", "stop")
                server.terminate()
                try:
                    server.wait(8)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
                gateway.shutdown()


if __name__ == "__main__":
    main()
