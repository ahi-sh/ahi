"""Verify one fresh `ahi session show --json` capture without model access."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('capture', type=Path)
parser.add_argument('workspace', type=Path)
args = parser.parse_args()
expected = (args.workspace / 'crates/probe').resolve()
with args.capture.open(encoding='utf-8') as stream:
    entries = [json.loads(line) for line in stream if line.strip()]
calls = [call for e in entries if e.get('role') == 'assistant' for call in e.get('calls', [])]
if len(calls) != 1 or calls[0]['name'] != 'run_command':
    raise SystemExit('FAIL: expected exactly one run_command call')
call = calls[0]
a = call['arguments']
if a.get('argv') != ['pwd'] or a.get('path') != 'crates/probe':
    raise SystemExit('FAIL: model did not request pwd with path crates/probe')
results = [e for e in entries if e.get('role') == 'tool' and e.get('result', {}).get('call_id') == call['id']]
if len(results) != 1 or results[0].get('exit_code') != 0:
    raise SystemExit('FAIL: missing or unsuccessful matching tool result')
detail = results[0].get('presentation', {}).get('detail', {})
if (detail.get('kind') != 'command' or detail.get('exit_code') != 0
        or detail.get('argv') != ['pwd'] or detail.get('stdout_cut') is not False
        or detail.get('stdout', '').rstrip('\n') != str(expected)):
    raise SystemExit('FAIL: recorded command did not produce the expected directory')
print(f'PASS: run_command(path=crates/probe, argv=[pwd]) exited 0 in {expected}')
