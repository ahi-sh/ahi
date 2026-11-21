#!/usr/bin/env python3
"""Run the scratch worker with a disposable Rex server, no GUI, and fake inference."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    ahi = str(p.parse_args().binary.resolve(strict=True))
    rex = shutil.which('rex')
    if not rex:
        p.error('install Rex first')
    requests = []

    class Gateway(BaseHTTPRequestHandler):
        def do_POST(self):
            requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            data = b'data: {"choices":[{"index":0,"delta":{"content":"headless worker answered"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    with tempfile.TemporaryDirectory(prefix='ahi-rx-',dir='/tmp') as tmp, HTTPServer(('127.0.0.1',0),Gateway) as gateway:
        root = Path(tmp).resolve()
        # No inherited REX_*, cloud credentials, user config, or live socket.
        env = {'HOME': str(root), 'PATH': str(Path(rex).parent) + os.pathsep + '/usr/bin:/bin', 'TMPDIR': str(root), 'XDG_CONFIG_HOME':str(root/'config'),'XDG_STATE_HOME':str(root/'state'),'XDG_CACHE_HOME':str(root/'cache'),'XDG_DATA_HOME':str(root/'data')}
        config = root/'empty.lua'; config.touch()
        endpoint='unix://'+str(root/'rex.sock')
        def call(*args):
            return subprocess.check_output(args, env=env, cwd=root, text=True, timeout=15)
        rx = [rex,'--autostart=false','--server',endpoint]
        state=root/'ahi'; state.mkdir(); prompt=root/'prompt'; prompt.write_text('Report a short fixture answer.')
        (state/'models.json').write_text(json.dumps({'providers':{'fixture':{'baseUrl':f'http://127.0.0.1:{gateway.server_port}/v1','api':'openai-completions','apiKey':'gateway','models':[{'id':'plain'}]}}}))
        threading.Thread(target=gateway.serve_forever,daemon=True).start()
        with (root/'rex.log').open('w+') as log:
            server=subprocess.Popen([rex,'server','run','--listen',endpoint,'--restore=false','--state-dir',str(root/'rex-state'),'--config',str(config)], env=env,cwd=root,stdin=subprocess.DEVNULL,stdout=log,stderr=log)
            try:
                for _ in range(100):
                    if (root/'rex.sock').is_socket(): break
                    if server.poll() is not None: raise RuntimeError((root/'rex.log').read_text()[-2000:])
                    time.sleep(.05)
                session=json.loads(call(*rx,'new','--json','--shell','none','--','/bin/sleep','30'))['session_id']
                prefix=[ahi,'--state-dir',str(state),'delegate']
                launched=json.loads(call(*prefix,'launch','--model','fixture/plain','--server',endpoint,'--host-session',session,'--prompt-file',str(prompt)))
                assignment=launched['assignment']['id']
                status=json.loads(call(*prefix,'status',assignment,'--wait','10'))
                assert status['result']['text']=='headless worker answered',status
                assert status['claim']['observed']['rex_block']==status['assignment']['phase']['terminal']['id'],status
                assert len(requests)==1 and requests[0]['model']=='plain' and 'tools' not in requests[0],requests
                print('PASS: one headless Rex worker, one tool-free inference, structured result')
            finally:
                server.terminate()
                try: server.wait(8)
                except subprocess.TimeoutExpired: server.kill(); server.wait()
                gateway.shutdown()


if __name__=='__main__': main()
