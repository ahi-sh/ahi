"""Explicit real-client acceptance check. Never selects a live endpoint."""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--ahi-bin", required=True)
    p.add_argument("--pi-bin", required=True)
    args = p.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        fail = Path(tmp) / "fails.sh"
        fail.write_text("exit 7\n")
        for client, binary in [("ahi", args.ahi_bin), ("pi", args.pi_bin)]:

            def trial(case, *opts):
                proc = subprocess.run(
                    [
                        sys.executable,
                        HERE / "probe.py",
                        "--client",
                        client,
                        "--binary",
                        binary,
                        "--case",
                        HERE / "cases" / (case + ".json"),
                        *opts,
                    ],
                    capture_output=True,
                    text=True,
                    timeout=135,
                )
                row = json.loads(proc.stdout)
                if row["mode"] != "scripted" or proc.returncode != int(not row["pass"]):
                    raise RuntimeError("invalid receipt or exit status")
                return row

            for case, opts in [
                ("base", []),
                ("coding", ["--checker", HERE / "checks/python-bounds.sh"]),
                (
                    "firmware",
                    ["--thinking", "high", "--checker", HERE / "checks/c-mask.sh"],
                ),
            ]:
                row = trial(case, *opts)
                if not row["pass"] or (opts and row["checker_exit"] != 0):
                    raise RuntimeError(row)
            prose = trial("base", "--answer-only")
            failed = trial("base", "--checker", fail)
            capped = trial("base", "--rounds", "1")
            if (
                prose["pass"]
                or not prose["gates"]["answer"]
                or prose["gates"]["read_result"]
                or failed["pass"]
                or failed["errors"]
                or failed["checker_exit"] != 7
                or failed["gates"]["check_process"]
                or capped["pass"]
                or capped["requests"] != 1
                or not capped["errors"]
            ):
                raise RuntimeError([prose, failed, capped])
            print(
                f"{client}: three workflows passed; prose-only, exit 7, and round exhaustion rejected"
            )


if __name__ == "__main__":
    main()
