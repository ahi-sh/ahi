"""Regression cases for the two-merge answer grader (no gateway or model)."""
import pytest

from merge_eval import merge_answer_supported


FACT = 'abc123def456'
ANSWER = 'relay-def456 limits retries to 3; beacon-abc123 sets the interval to 30.'
LOG = ('6e9af04 Refresh release calendar\n'
       'c6ae94a Merge relay-def456: retries=3\n'
       '523195d Merge beacon-abc123: interval=30\n')


def records(tool='git_inspect', exit_code=0, content=LOG):
    return [{'result': {'name': tool, 'content': content}, 'exit_code': exit_code}]


@pytest.mark.parametrize(('answer', 'passed'), [
    (ANSWER + ' The newer release calendar commit is not a merge.', True),
    ('`beacon-abc123`: 30-second interval\n`relay-def456`: three retries', True),
    ('relay-def456 limits retries to 99; beacon-abc123 sets the interval to 900.', False),
    ('relay-def456: interval=30; beacon-abc123: retries=3', False),
    ('relay-def456: retries=3.5; beacon-abc123: interval=30', False),
    ('beacon-abc123: interval=30; the other merge refreshed the release calendar.', False),
])
def test_answer_identifies_each_topic_and_its_change(answer, passed):
    assert merge_answer_supported(answer, FACT, records()) is passed


@pytest.mark.parametrize('evidence', [[], records(tool='read_file'), records(exit_code=126),
                                      records(content='523195d Merge beacon-abc123: interval=30')])
def test_correct_prose_alone_cannot_replace_git_evidence(evidence):
    assert not merge_answer_supported(ANSWER, FACT, evidence)
