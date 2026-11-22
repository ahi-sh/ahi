#!/usr/bin/env python3
"""Compare two ahi binaries on synthetic workflows. Default: scripted loopback, no inference."""
import argparse
from contextlib import closing, contextmanager
from http.client import HTTPConnection, HTTPSConnection
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
from threading import Thread
from urllib.parse import urlsplit

from merge_eval import merge_answer_supported, merge_topics, seed_merges


ROUNDS, TOKENS = 5, 768
SKILLS = ['repo/amber', 'repo/cobalt', 'repo/mica']
FACT_PATH = 'docs/station.conf'
QUESTIONS = {
    'merges': 'what were the last two git merges about?',
    'question': 'Which beacon is configured for this station? Give its value and repository path. Do not edit.',
    'edit': 'Change only port=8010 to port=8020 in app.conf. Run verify-fixture afterwards. Report the result as JSON with exit_code and diagnostic fields.',
    'catalog': 'Which skills were already loaded for this turn (not merely available)? Do not load more. Return a JSON array of scoped IDs.',
}
QUESTIONS['preload'] = QUESTIONS['catalog']


def scripted(case, n, fact, diagnostic):
    def call(name, arguments):
        return {'tool_calls': [{'index': 0, 'id': f'call_{n}', 'type': 'function', 'function': {
            'name': name, 'arguments': json.dumps(arguments)}}]}
    if case == 'merges' and n == 1:
        delta = call('git_inspect', {'operation': 'log'})
    elif case == 'question' and n == 1:
        delta = call('read_file', {'path': FACT_PATH})
    elif case == 'edit' and n == 1:
        delta = call('edit_file', {'path': 'app.conf', 'old': '8010', 'new': '8020'})
    elif case == 'edit' and n == 2:
        delta = call('run_command', {'argv': ['verify-fixture']})
    elif case == 'merges':
        beacon, relay = merge_topics(fact)
        delta = {'content': f'{relay} limits retries to 3; {beacon} sets the interval to 30.'}
    else:
        delta = {'content': {'question': f'{FACT_PATH}: {fact}', 'edit': json.dumps({'exit_code': 7, 'diagnostic': diagnostic}),
                             'catalog': '[]', 'preload': '["repo/cobalt"]'}[case]}
    finish = 'tool_calls' if 'tool_calls' in delta else 'stop'
    return (f'data: {json.dumps({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})}\n\ndata: [DONE]\n\n').encode()


def live_reply(endpoint, body):
    # No redirects or retries; never forward credentials to another origin.
    conn = HTTPSConnection if endpoint.scheme == 'https' else HTTPConnection
    headers = {'Content-Type': 'application/json'}
    if key := os.environ.get('AHI_EVAL_API_KEY'):
        headers['Authorization'] = 'Bearer ' + key
    with closing(conn(endpoint.hostname, endpoint.port, timeout=30)) as client:
        client.request('POST', endpoint.path.rstrip('/') + '/chat/completions', json.dumps(body), headers)
        response = client.getresponse()
        if response.status != 200:
            raise ValueError(f'upstream HTTP {response.status}')
        data = response.read(2**20 + 1)
        if len(data) > 2**20:
            raise ValueError('oversized upstream reply')
        return data


def snapshot(work):
    return {str(p.relative_to(work)): p.read_bytes() for p in work.rglob('*') if p.is_file()}


@contextmanager
def fresh_fixture(path):
    # Recreate the exact same absolute catalog paths on both arms, never reuse state.
    path.mkdir()
    try:
        yield path
    finally:
        shutil.rmtree(path)


def json_answer(text):
    try:
        return json.loads(text.strip().removeprefix('```json').removesuffix('```').strip())
    except ValueError:
        return None


def run(binary, arm, case, model, endpoint, out, fact, diagnostic):
    capture = out / arm / case
    capture.mkdir(parents=True)
    requests = []
    errors = []

    class Gateway(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            try:
                if self.path != '/v1/chat/completions' or len(requests) >= ROUNDS or errors:
                    raise ValueError('unexpected route or request budget exhausted')
                body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                if body['model'] != model or body.get('max_completion_tokens', body.get('max_tokens')) != TOKENS:
                    raise ValueError('model/output budget mismatch')
                if case == 'merges' and body.get('reasoning_effort') != 'high':
                    raise ValueError('merge fixture requires high reasoning effort')
                system = [m for m in body['messages'] if m['role'] == 'system']
                if len(system) != int(arm == 'candidate') or (system and not system[0]['content']):
                    raise ValueError(f'{arm}: wrong binary or unexpected prompt count')
                requests.append(body)
                n = len(requests)
                (capture / f'{n}.request.json').write_text(json.dumps(body))
                data = live_reply(endpoint, body) if endpoint else scripted(case, n, fact, diagnostic)
                with (capture / f'{n}.delta.jsonl').open('w') as fields:
                    for line in data.splitlines():
                        if line.startswith(b'data:') and line[5:].strip() != b'[DONE]':
                            for choice in json.loads(line[5:]).get('choices', []):
                                print(json.dumps(choice), file=fields)  # reasoning, text and structured calls stay separate
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.end_headers()
                self.wfile.write(data)
            except (OSError, ValueError, KeyError) as exc:
                errors.append(str(exc))
                self.send_error(400, 'eval request refused')

    with fresh_fixture(out.resolve() / 'scratch') as root, HTTPServer(('127.0.0.1', 0), Gateway) as server:
        state, work, bins = root / 'state', root / 'work', root / 'bin'
        for directory in [state, work, bins]:
            directory.mkdir()
        files = {FACT_PATH: f'beacon={fact}\n', 'app.conf': 'port=8010\nkeep=true\n', 'unrelated': 'pending change\n'}
        for skill in SKILLS:
            name = skill.split('/')[1]
            files[f'.agents/skills/{name}/SKILL.md'] = f'---\nname: {name}\ndescription: Synthetic {name} skill.\n---\nSynthetic instructions for {name}.\n'
        for name, text in files.items():
            path = work / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        check = bins / 'verify-fixture'
        check.write_text(f'#!/bin/sh\necho {diagnostic} >&2\nexit 7\n')
        check.chmod(0o700)
        rules = [('tool_call', {'tool': t}) for t in ('read_file', 'find_files', 'list_dir', 'search_files')]
        if case == 'merges':
            seed_merges(work, fact)
            rules.append(('tool_call', {'tool': 'git_inspect'}))
        if case == 'edit':
            rules += [('file_write', {'path': 'app.conf'}), ('command', {'argv': ['verify-fixture']})]
        if case in ('catalog', 'preload'):
            rules += [('skill', {'skill': s}) for s in SKILLS]
        policy = root / 'policy.yaml'
        policy.write_text(json.dumps({'version': 1, 'default': 'deny', 'rules': [dict(
            id=f'r{i}', action=a, match=m, effect='approve') for i, (a, m) in enumerate(rules)]}))
        metadata = {'id': model, 'maxTokens': TOKENS}
        if case == 'merges':
            metadata.update(reasoning=True, compat={'supportsReasoningEffort': True})
        (state / 'models.json').write_text(json.dumps({'providers': {'eval': {
            'baseUrl': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'gateway', 'api': 'openai-completions',
            'models': [metadata]}}}))
        before = snapshot(work)
        runner = Thread(target=server.serve_forever)
        runner.start()
        try:
            proc = subprocess.run([binary, '--state-dir', state, 'exec', '--standalone', '--format', 'json',
                                   '--timeout', '75', '--model', f'eval/{model}', '--thinking', 'high' if case == 'merges' else 'off', '--tools', policy,
                                   '--workdir', work, *(['--skill', 'repo/cobalt'] if case == 'preload' else []),
                                   '-p', QUESTIONS[case]], cwd=work, env={'HOME': str(root), 'PATH': str(bins)},
                                  text=True, capture_output=True, timeout=90)
        finally:
            server.shutdown()
            runner.join()
        result = json.loads(proc.stdout or '{}')
        session = state / 'sessions' / (result.get('session', '') + '.jsonl')
        entries = [json.loads(line) for line in session.read_text().splitlines()] if session.is_file() else []
        records = [e for e in entries if e.get('role') == 'tool' and 'result' in e]
        answer = result.get('text', '')
        evidence = [e['result']['content'] for e in records if e.get('exit_code') == 0]
        ok = not proc.returncode and result.get('status') == 'ok' and not errors and bool(requests)
        if case == 'merges':
            ok &= merge_answer_supported(answer, fact, records)
        elif case == 'question':
            ok &= fact in answer and FACT_PATH in answer and any(fact in s for s in evidence)
        elif case == 'edit':
            before['app.conf'] = b'port=8020\nkeep=true\n'
            report = json_answer(answer)
            ok &= isinstance(report, dict) and report.get('exit_code') == 7 and diagnostic in str(report.get('diagnostic')) and any(e['result']['name'] == 'run_command' and
                e.get('exit_code') == 7 and diagnostic in e['result']['content'] for e in records)
        else:
            expected = ['repo/cobalt'] if case == 'preload' else []
            schema = next((t['function']['description'] for t in requests[0].get('tools', [])
                           if t['function']['name'] == 'load_skill'), '') if requests else ''
            loads = [e['result']['content'] for e in records if e['result']['name'] == 'load_skill' and e['exit_code'] == 0]
            ids = json_answer(answer)
            ok &= all(s in schema for s in SKILLS) and ids == expected and len(loads) == len(expected)
            if expected:
                ok &= any('Synthetic instructions for cobalt.' in text for text in loads)
        ok &= snapshot(work) == before
        (capture / 'score.json').write_text(json.dumps({'pass': bool(ok), 'answer': answer, 'errors': errors,
            'exit': proc.returncode, 'stderr': proc.stderr, 'status': result.get('status'), 'error': result.get('error'),
            'requests': len(requests), 'results': records}, indent=2))
        # All per-arm request controls and schemas, excluding history/prompt and generated call IDs.
        controls = {k: v for k, v in requests[0].items() if k != 'messages'} if requests else None
        print(f'{arm}/{case}: {"PASS" if ok else "FAIL"} ({len(requests)} requests)', flush=True)
        return bool(ok), controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('baseline', 'candidate', 'out'):
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--live-url', help='opt in to paid inference; Chat Completions gateway base, e.g. https://host/v1')
    parser.add_argument('--case', choices=QUESTIONS, help='run just this workflow pair')
    parser.add_argument('--model', help='explicit served model ID (required for live)')
    args = parser.parse_args()
    endpoint = urlsplit(args.live_url) if args.live_url else None
    if endpoint and (not args.model or endpoint.scheme not in ('http', 'https') or not endpoint.hostname or
                     endpoint.username or endpoint.password or endpoint.query or endpoint.fragment):
        parser.error('--live-url requires an HTTP(S) base URL without credentials/query/fragment and --model')
    if endpoint and endpoint.scheme == 'http' and endpoint.hostname not in ('localhost', '127.0.0.1', '::1'):
        parser.error('use HTTPS for non-loopback inference')
    args.out.mkdir(mode=0o700, parents=True, exist_ok=False)
    fact, diagnostic = secrets.token_hex(6), 'CHECK-' + secrets.token_hex(5)
    print('LIVE inference' if endpoint else 'SCRIPTED loopback (not model evidence)', flush=True)
    good = True
    for case in ([args.case] if args.case else QUESTIONS):
        controls = []
        for arm in ('baseline', 'candidate'):
            passed, control = run(getattr(args, arm).resolve(), arm, case, args.model or 'fixture', endpoint,
                                  args.out, fact, diagnostic)
            good &= passed
            controls.append(control)
        if controls[0] != controls[1]:
            raise ValueError(f'{case}: incomparable request schemas/settings')
    return int(not good)


if __name__ == '__main__':
    raise SystemExit(main())
