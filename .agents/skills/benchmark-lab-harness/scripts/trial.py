"""Stage one fresh Pi or ahi headless arm. Default: validate setup without inference."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('arm', choices=['pi', 'ahi'])
    p.add_argument('--binary', required=True, type=Path, help='exact executable for this arm')
    p.add_argument('--catalog', required=True, type=Path, help='reviewed lab gateway models.json export')
    p.add_argument('--model', required=True, help='exact provider/model-id in the export')
    p.add_argument('--out', required=True, type=Path, help='new private trial directory outside the fixture')
    p.add_argument('--case', type=Path, help='reviewed case directory (default: no tools, reply LAB_OK)')
    p.add_argument('--pi-extension', type=Path, help='reviewed local extension file (Pi live arm only)')
    p.add_argument('--ahi-skill', help='explicit repo/name from the case workspace (ahi only)')
    p.add_argument('--timeout', type=int, default=60, help='client wall time limit in seconds')
    p.add_argument('--live', action='store_true', help='submit the prompt instead of checking setup')
    a = p.parse_args()
    binary, out = a.binary.resolve(), a.out.resolve()
    provider, _, model = a.model.partition('/')
    config = json.loads(a.catalog.read_bytes())['providers'].get(provider)
    if not config or not any(m['id'] == model for m in config['models']):
        p.error('select an exact provider/model from the reviewed export')
    if a.timeout < 1 or not binary.is_file() or not os.access(binary, os.X_OK):
        p.error('need an executable binary and a positive timeout')
    if a.case and (out == a.case.resolve() or a.case.resolve() in out.parents):
        p.error('--out must be outside --case')

    if a.pi_extension and (a.arm != 'pi' or not a.case or not a.pi_extension.is_file()):
        p.error('--pi-extension requires the pi arm, --case and a reviewed local file')
    if a.ahi_skill and (a.arm != 'ahi' or not a.case or not a.ahi_skill.startswith('repo/')):
        p.error('--ahi-skill requires the ahi arm, --case and a repo-scoped skill ID')

    os.umask(0o077)
    out.mkdir(exist_ok=False)
    work, state = out / 'work', out / 'state'
    state.mkdir()
    (state / 'models.json').write_text(json.dumps({'providers': {provider: config}}))
    prompt, policy = out / 'prompt.txt', out / 'policy.yaml'
    if a.case:
        if any(f.is_symlink() for f in a.case.rglob('*')):
            p.error('case must contain only local fixture files, not symlinks')
        shutil.copytree(a.case / 'work', work)
        shutil.copyfile(a.case / 'prompt.txt', prompt)
        shutil.copyfile(a.case / 'ahi.tools.yaml', policy)
        tools = (a.case / 'pi-tools.txt').read_text().strip()
        if not tools:
            p.error('pi-tools.txt must contain none or an explicit comma-separated tool list')
    else:
        work.mkdir()
        prompt.write_text('Reply with exactly LAB_OK.\n')
        policy.write_text('version: 1\ndefault: deny\nrules: []\n')
        tools = 'none'

    env = {'PATH': os.environ['PATH'], 'HOME': str(out), 'PI_CODING_AGENT_DIR': str(state), 'PI_OFFLINE': '1'}
    if a.arm == 'ahi':
        cmd = [binary, '--state-dir', state, 'exec', '--standalone', '--headless', '--model', a.model,
               '--thinking', 'off', '--timeout', str(a.timeout), '--prompt-file', prompt, '--tools', policy, '--workdir', work,
               '--result-file', out / 'result.json']
        if a.ahi_skill:
            cmd += ['--skill', a.ahi_skill]
        if not a.live:
            cmd.append('--check')
    else:
        cmd = [binary, '--offline', '--no-approve', '--no-mcp', '--no-extensions', '--no-context-files',
               '--no-skills', '--no-prompt-templates', '--no-session']
        if a.live and a.pi_extension:
            cmd += ['--extension', a.pi_extension.resolve()]
        cmd += ['--no-tools'] if tools == 'none' else ['--tools', tools]
        cmd += (['--provider', provider, '--model', model, '--thinking', 'off', '--mode', 'json', '-p'] if a.live
                else ['--list-models', model])

    version = subprocess.check_output([binary, '--version'], env=env, cwd=state, text=True, timeout=10).strip()
    prompt_hash = hashlib.sha256(prompt.read_bytes()).hexdigest()
    catalog_hash = hashlib.sha256((state / 'models.json').read_bytes()).hexdigest()
    start = time.monotonic()
    with prompt.open('rb') as inp, (out / 'stdout').open('wb') as stdout, (out / 'stderr').open('wb') as stderr:
        try:
            rc = subprocess.run(cmd, stdin=inp if a.live and a.arm == 'pi' else subprocess.DEVNULL,
                                stdout=stdout, stderr=stderr, env=env, cwd=work, timeout=a.timeout + 2).returncode
        except subprocess.TimeoutExpired:
            rc = 124
    elapsed = time.monotonic() - start
    if not a.live and a.arm == 'pi' and rc == 0:
        rows = [s.split()[:2] for s in (out / 'stdout').read_text().splitlines()]
        if [provider, model] not in rows:
            print('Pi did not list the requested provider/model; inspect stdout and stderr.', file=sys.stderr)
            rc = 1
    record = dict(arm=a.arm, model=a.model, binary=str(binary), live=a.live, client_wall_s=elapsed, exit=rc,
                  case=str(a.case.resolve()) if a.case else 'base', pi_tools=tools if a.arm == 'pi' else None,
                  pi_extension=str(a.pi_extension.resolve()) if a.pi_extension else None, ahi_skill=a.ahi_skill,
                  binary_version=version, prompt_sha256=prompt_hash, provider_snapshot_sha256=catalog_hash)
    (out / 'trial.json').write_text(json.dumps(record, indent=2) + '\n')
    print(out / 'trial.json')
    return rc


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc)) from exc
