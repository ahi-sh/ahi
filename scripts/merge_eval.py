"""Disposable Git history and observable acceptance criteria for the merges eval."""
import os
import re
import subprocess


def merge_topics(fact):
    return (f'beacon-{fact[:6]}', f'relay-{fact[6:]}')


def seed_merges(work, fact):
    env = dict(PATH=os.environ['PATH'], GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull,
               GIT_AUTHOR_DATE='2025-01-01T00:00:00Z', GIT_COMMITTER_DATE='2025-01-01T00:00:00Z')

    def git(*args):
        subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=test@example.invalid',
                        *args], cwd=work, env=env, check=True, capture_output=True)

    git('init', '--template=', '-b', 'main')
    git('commit', '--allow-empty', '-m', 'Initialize workspace')
    for topic, setting in zip(merge_topics(fact), ('interval=30', 'retries=3')):
        git('switch', '-c', topic)
        (work / f'{topic}.conf').write_text(setting + '\n')
        git('add', f'{topic}.conf')
        git('commit', '-m', f'Configure {topic}: {setting}')
        git('switch', 'main')
        git('merge', '--no-ff', '-m', f'Merge {topic}: {setting}', topic)
    git('commit', '--allow-empty', '-m', 'Refresh release calendar')


def merge_answer_supported(answer, fact, records):
    """Match topic → setting/value, not a bag of names and numbers from the whole reply."""
    beacon, relay = merge_topics(fact)
    subjects = {
        f'Merge {beacon}: interval=30',
        f'Merge {relay}: retries=3',
    }
    observed = {
        line.partition(' ')[2]
        for entry in records
        if entry['result']['name'] == 'git_inspect' and entry['exit_code'] == 0
        for line in entry['result']['content'].splitlines()
    }
    if not subjects <= observed:
        return False

    # Require each claim in the description of its own topic. Accept either
    # merge order, configuration syntax or prose. A decoy mentioned elsewhere
    # (e.g. to explain why it was excluded) doesn't invalidate either claim.
    parts = re.split(rf'\b({beacon}|{relay})\b', answer.lower().replace('`', '').replace('**', ''))
    descriptions = list(zip(parts[1::2], parts[2::2]))
    for topic, key, number in ((beacon, 'interval', '(?:30|thirty)'),
                               (relay, 'retr(?:y|ies)', '(?:3|three)')):
        claim = rf'\b{key}\b\s*(?:(?:=|:|of|to|at|is|set to)\s*)?{number}\b(?!\.\d)'
        reversed_claim = rf'(?<![\w.+-]){number}(?:[- ]seconds?)?\s+{key}\b'
        if not any(t == topic and re.search(f'{claim}|{reversed_claim}', detail)
                   for t, detail in descriptions):
            return False
    return True
