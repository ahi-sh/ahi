"""Create two repeatable, synthetic long workloads in a NEW directory (no inference)."""

import argparse
import hashlib
import json
from pathlib import Path


def cases():
    # Literal part codes rather than a pattern a model can extrapolate.
    codes = [
        hashlib.sha256(f"sensor-table-v1:{i}".encode()).hexdigest()[:10]
        for i in range(384)
    ]
    table = "Calibration catalog (sensor ID => part code):\n" + "\n".join(
        f"S{i:03} => {code}" for i, code in enumerate(codes)
    )
    yield (
        "catalog-lookup",
        dict(
            prompt=table
            + "\nRead request.txt. Return the part code for the sensor ID in the file.",
            input="request.txt",
            before="S283\n",
            marker="S283",
            answer=codes[283],
        ),
    )
    ids = [(i * 73 + 11) % len(codes) for i in range(112)]
    yield (
        "catalog-manifest",
        dict(
            prompt=table
            + "\nRead assembly.csv. Replace each sensor ID line with ID,part-code "
            "(no header, same order, LF after every line). Write the entire manifest, then return the filename.",
            input="assembly.csv",
            before="".join(f"S{i:03}\n" for i in ids),
            marker="S011",
            after="".join(f"S{i:03},{codes[i]}\n" for i in ids),
            answer="assembly.csv",
        ),
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    out = parser.parse_args().out
    out.mkdir(mode=0o700)
    for name, case in cases():
        path = out / (name + ".json")
        path.write_text(json.dumps(case, indent=2) + "\n")
        print(path)
