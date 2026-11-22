"""Exercise both trial adapters with a scripted loopback reply, not a lab model."""
import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from threading import Thread


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arm in ('pi', 'ahi'):
        parser.add_argument('--' + arm, required=True, type=Path, help='path to installed executable')
    args = parser.parse_args()
    requests = []

    class Fixture(BaseHTTPRequestHandler):
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(body)
            if (self.path != '/v1/chat/completions' or body['model'] != 'fixture' or body.get('tools')
                    or 'LAB_OK' not in str(body['messages'][-1]['content'])):
                self.send_error(400, 'wrong route, model, tools or prompt')
                return
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            self.wfile.write(b'data: {"choices":[{"index":0,"delta":{"role":"assistant","content":"LAB_OK"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n')

    with tempfile.TemporaryDirectory() as tmp, HTTPServer(('127.0.0.1', 0), Fixture) as server:
        root = Path(tmp)
        catalog = root / 'models.json'
        catalog.write_text(json.dumps({'providers': {'loopback': {
            'api': 'openai-completions', 'baseUrl': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'gateway',
            'models': [{'id': 'fixture', 'name': 'Fixture', 'reasoning': False, 'input': ['text'],
                        'contextWindow': 8192, 'maxTokens': 128}]}}}))
        thread = Thread(target=server.serve_forever)
        thread.start()
        try:
            for arm, binary in vars(args).items():
                for submit in (False, True):
                    before = len(requests)
                    out = root / f'{arm}-{submit}'
                    cmd = [sys.executable, Path(__file__).with_name('trial.py'), arm, '--binary', binary,
                           '--model', 'loopback/fixture', '--catalog', catalog, '--out', out]
                    rc = subprocess.run(cmd + (['--live'] if submit else [])).returncode
                    if rc or len(requests) - before != int(submit):
                        stderr = out / 'stderr'
                        raise SystemExit(f'{arm} submit={submit} failed: {stderr.read_text() if stderr.exists() else rc}')
                    if not submit:
                        continue
                    if arm == 'pi':
                        events = map(json.loads, (out / 'stdout').read_text().splitlines())
                        replies = [e['message'] for e in events if e['type'] == 'message_end' and e['message']['role'] == 'assistant']
                        answer = ''.join(c['text'] for c in replies[-1]['content'] if c['type'] == 'text')
                    else:
                        result = json.loads((out / 'result.json').read_text())
                        answer = result['text'] if result['status'] == 'ok' else None
                    if answer != 'LAB_OK':
                        raise SystemExit(f'{arm}: wrong final answer')
        finally:
            server.shutdown()
            thread.join()
    print('PASS: both preflights sent zero requests; both clients returned the scripted answer.')


if __name__ == '__main__':
    main()
