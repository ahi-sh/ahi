#!/usr/bin/env python3
"""Opt-in, paid live acceptance probe. Uses only the selected model and synthetic inputs.
Run from checkout: python3 scripts/probe-live-tools.py --live --model home/GLM-5.3-Flash-EXL3
Each group starts a fresh conversation; failed checks are not automatically retried.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--live', action='store_true', required=True)
    parser.add_argument('--model', required=True)
    parser.add_argument('--config', type=Path, default=Path.home() / '.config/ahi/models.json')
    parser.add_argument('--group', action='append', choices=['discover', 'search', 'page', 'empty', 'oversize', 'create', 'edit', 'refuse', 'command', 'skill'])
    args = parser.parse_args()
    gateway, model = args.model.split('/', 1)
    provider = json.loads(args.config.read_text())['providers'][gateway]
    provider['models'] = [m for m in provider['models'] if (m if isinstance(m, str) else m['id']) == model]
    if not provider['models']:
        parser.error('requested model is not curated')
    checks = []
    with tempfile.TemporaryDirectory(prefix='ahi-tool-probe-') as tmp:
        base = Path(tmp).resolve()
        work, state = base / 'work', base / 'state'
        work.mkdir()
        state.mkdir(mode=0o700)
        config = state / 'models.json'
        config.write_text(json.dumps({'providers': {gateway: provider}}))
        config.chmod(0o600)
        marker, skill = secrets.token_hex(8), secrets.token_hex(8)
        fixtures = {'docs/sub/notes α.txt': f'header\nRow {marker}\nend\n', 'docs/dupes.txt': 'before\nred\nred\nafter\n',
                    'docs/topic.md': 'Title\nToken.* one\ntoken.* two\ntokenXX decoy\n', '.gitignore': 'ignored/\n',
                    'ignored/decoy.txt': 'Token.* ignored\n', 'empty': '', 'long': 'w' * 65537 + '\nnext\n',
                    'exists': 'original\n', '.agents/skills/probe/SKILL.md': '---\nname: probe\ndescription: Return a disposable test marker.\n---\nMarker: ' + skill + '\n'}
        for name, data in fixtures.items():
            path = work / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(data)
        rules = []
        for action, match in [('tool_call', {'tool': t}) for t in ('read_file', 'find_files', 'list_dir', 'search_files')] + [
                ('file_write', {}), ('skill', {'skill': 'repo/probe'}), ('command', {'argv': ['pwd']}), ('command', {'argv': ['printf']})]:
            rules.append({'id': 'allow-' + str(len(rules)), 'action': action, 'match': match, 'effect': 'approve'})
        policy = base / 'policy.yaml'
        policy.write_text(json.dumps({'version': 1, 'default': 'deny', 'rules': rules}))
        groups = {
            'discover': 'Use list_dir with path docs (not the root). Separately, use find_files from the workspace root for *.txt, including nested files. Read docs/sub/notes α.txt and return its Row marker.',
            'search': 'Use search_files three times under docs: literal Token.* ignoring case in *.md, then regex ^red$ in *.txt with limit 1, then literal NO_SUCH_NEEDLE. Report matches.',
            'page': 'Use read_file on docs/sub/notes α.txt twice: first only line 2 (offset 2, limit 1), then offset 99. Do not retry; report both results.',
            'empty': 'Use read_file on empty with offset 1 and limit 1. Then use read_file with offset 2 on the same empty file. Do not retry.',
            'oversize': 'Use read_file on long with offset 1, limit 1 exactly once, then on line 2 only. Do not repeat a request. Describe whether line 1 could fit.',
            'create': 'Use write_file to create docs/new.txt containing exactly alpha and a newline; then use edit_file to replace alpha with beta. Read it back with read_file.',
            'edit': 'Use edit_file on docs/dupes.txt: first try replacing red with blue without replace_all; then replace all red occurrences with blue. Report how to recover from the first error.',
            'refuse': 'Use write_file on exists with content other but no overwrite. Then try writing missing.txt with overwrite true. Both should fail; do not retry.',
            'command': 'Use run_command for pwd in docs/sub, then printf with format <%s> and a single argument two words. No shell. Return both outputs.',
            'skill': 'Use load_skill to read repo/probe. Return its marker; do not use read_file.',
        }
        for group, prompt in groups.items():
            if args.group and group not in args.group:
                continue
            proc = subprocess.run(['cargo', 'run', '--quiet', '--locked', '--', '--state-dir', str(state), 'exec', '-p', prompt,
                                   '--model', args.model, '--workdir', str(work), '--tools', str(policy), '--standalone',
                                   '--timeout', '60', '--thinking', 'off', '--format', 'json'], capture_output=True, text=True, timeout=110)
            if not proc.stdout.strip():
                raise RuntimeError(f'{group}: no result (exit {proc.returncode}); {proc.stderr[-500:]}')
            result = json.loads(proc.stdout)
            session = result['session']
            entries = [json.loads(s) for s in (state / 'sessions' / (session + '.jsonl')).read_text().splitlines()]
            calls = {c['id']: c for e in entries for c in e.get('calls', [])}
            records = [(calls[e['result']['call_id']], e) for e in entries if e.get('result', {}).get('call_id')]
            def check(label, value):
                passed = result['status'] == 'ok' and bool(value)
                checks.append(passed)
                print(f'{"PASS" if passed else "FAIL"} {group}: {label}', flush=True)
            def observed(name, contains=None, code=None, arguments=None, detail=None, exact=None):
                return any(c['name'] == name and all(c['arguments'].get(k) == v for k, v in (arguments or {}).items())
                           and (code is None or (e['exit_code'] == 0) == (code == 0))
                           and (contains is None or contains in e['result']['content'])
                           and (exact is None or exact == e['result']['content'])
                           and (detail is None or detail == e['presentation']['detail']['lines']) for c, e in records)
            print(f'{group}: {result["status"]}, {session}, {len(records)} tool results', flush=True)
            if group == 'discover':
                check('sorted directory including subdirectory suffix', observed('list_dir', exact='dupes.txt\nsub/\ntopic.md'))
                check('nested glob and ignore rule', observed('find_files', exact='docs/dupes.txt\ndocs/sub/notes α.txt'))
                check('Unicode path, exact file bytes', observed('read_file', exact=fixtures['docs/sub/notes α.txt']))
                check('hidden marker in answer', marker in result.get('text', ''))
            elif group == 'search':
                check('case-insensitive literal with glob', observed('search_files', exact='docs/topic.md:2:Token.* one\ndocs/topic.md:3:token.* two'))
                check('regex with one matching row', observed('search_files', contains='docs/dupes.txt:2:red', arguments={'limit': 1}) and all('docs/dupes.txt:3:red' not in e['result']['content'] for _, e in records))
                check('no-match sentinel', observed('search_files', exact='[no matches]'))
                check('no-match presentation counts zero', any(e['result']['content'] == '[no matches]' and e['presentation']['detail']['matches'] == 0 for _, e in records))
                check('limited presentation excludes the continuation note', any('1 more match not shown' in e['result']['content'] and e['presentation']['detail']['matches'] == 1 for _, e in records))
            elif group == 'page':
                check('window excludes other lines', observed('read_file', exact=f'[lines 2-2 of 3]\nRow {marker}\n[1 more line; continue with offset=3]'))
                check('presentation counts file lines, not framing', observed('read_file', arguments={'offset': 2, 'limit': 1}, detail=1))
                check('past EOF explicitly rejected', observed('read_file', contains='past the end', code=1))
            elif group == 'empty':
                check('empty first page is not an inverted range', observed('read_file', exact='[empty file]\n', code=0))
                check('past EOF on empty file is rejected', observed('read_file', contains='past the end', code=1))
            elif group == 'oversize':
                check('oversized first line has actionable error, not self-continuation', observed('read_file', contains='requested line exceeds', code=1))
                check('subsequent small line readable', observed('read_file', contains='next\n', arguments={'offset': 2}))
            elif group == 'create':
                check('create succeeds', observed('write_file', code=0, exact='created new.txt: 1 line'))
                check('exact replacement succeeds', observed('edit_file', code=0))
                check('read-back matches disk bytes', observed('read_file', exact='beta\n') and (work / 'docs/new.txt').read_bytes() == b'beta\n')
                check('mutation declared in result', result['effects'] == 'some')
            elif group == 'edit':
                check('ambiguous match rejected', observed('edit_file', contains='more than once', code=1))
                check('ambiguous edit gives count and recovery options', observed('edit_file', contains='2 occurrences); include more context or set replace_all', code=1))
                check('replace_all preserves surrounding bytes', observed('edit_file', code=0, arguments={'replace_all': True}) and (work / 'docs/dupes.txt').read_text() == 'before\nblue\nblue\nafter\n')
            elif group == 'refuse':
                check('create cannot clobber existing file', observed('write_file', contains='file exists', code=1) and (work / 'exists').read_text() == 'original\n')
                check('overwrite cannot create missing file', observed('write_file', contains='does not exist', code=1) and not (work / 'missing.txt').exists())
                check('no file effects for failed writes', result['effects'] == 'none')
            elif group == 'command':
                check('cwd honored', observed('run_command', contains=str(work / 'docs/sub'), code=0))
                check('argv preserves a space', observed('run_command', contains='<two words>', code=0))
            elif group == 'skill':
                check('scoped skill actually loaded', observed('load_skill', contains=skill, code=0))
                check('skill marker reached final answer', skill in result.get('text', ''))
            if proc.returncode or not checks[-1] or os.getenv('AHI_PROBE_DETAILS'):
                for c, e in records:
                    print(' ', c['name'], c['arguments'], e['exit_code'], repr(e['result']['content'][:220]), flush=True)
    print(f'{sum(checks)}/{len(checks)} checks passed', flush=True)
    return int(not all(checks))


if __name__ == '__main__':
    raise SystemExit(main())
