"""Tests for scripts/release_rerun.py: a failed release is retried once, and only when every
failure was the infrastructure's (SB-563, M4-06). The logs are shaped like GitHub's raw job logs
(a timestamp on every line), modelled on real failures: v0.7.122 attempt 1 (an Ubuntu mirror
answered 503), v0.7.121 attempt 1 (a test failed) and v0.7.123 (the guard refused)."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from release_rerun import classify, decide  # noqa: E402


def log(*lines):
    return ''.join(f'2026-11-19T12:36:58.{i:07d}Z {line}\n' for i, line in enumerate(lines))


MIRROR = log(
    '##[group]Run sudo apt-get update',
    'Err:3 https://security.ubuntu.com/ubuntu noble-updates InRelease',
    'E: Failed to fetch http://security.ubuntu.com/ubuntu/pool/main/b/bubblewrap/bubblewrap.deb  503  Service Unavailable',
    'E: Unable to fetch some archives, maybe run apt-get update or try with --fix-missing?',
    '##[error]Process completed with exit code 100.')
TEST = log(
    'Err:3 https://security.ubuntu.com/ubuntu noble-updates InRelease',
    'W: Failed to fetch https://security.ubuntu.com/ubuntu/dists/noble-updates/InRelease',
    "thread 'a_server_whose_program_was_replaced_answers' panicked at crates/switchboard-cli/tests/mcp.rs:1697:37:",
    'failures:',
    'test result: FAILED. 12 passed; 1 failed; 0 ignored',
    '##[error]Process completed with exit code 101.')
GUARD = log(
    '##[error]nightly.yml concluded failure on c997483 (https://example/runs/1); fix it',
    'release guard: refused, nightly.yml concluded failure on c997483',
    '##[error]Process completed with exit code 1.')
GUARD_503 = log(
    '##[error]GitHub could not answer: gh api repos/x/compare/a...main: HTTP 503',
    '##[error]Process completed with exit code 1.')
COMPILE = log('error[E0425]: cannot find function `parse_elapsed` in this scope',
              'error: could not compile `switchboard-runtime` (lib) due to 1 previous error')
SCRIPT_TEXT = log(
    '##[group]Run $changed = git status --porcelain',
    'Write-Host "::error::the build changed the source tree"; exit 1',
    'E: Failed to fetch http://mirror/x.deb  503  Service Unavailable',
    '##[error]Process completed with exit code 100.')
OWN_CHECK = log('E: Failed to fetch http://mirror/x.deb  503',
                '##[error]the build changed the source tree',
                '##[error]Process completed with exit code 1.')
APT_LOOP = log('##[warning]apt-get failed (attempt 3)',
               '##[error]the package mirror failed 3 times; see the apt-get output above',
               '##[error]Process completed with exit code 1.')
RUNNER = log('##[error]The runner has received a shutdown signal. This can happen when the runner service is stopped.')
NPM = log('npm error code ECONNRESET', 'npm error network aborted', '##[error]Process completed with exit code 1.')
# The apt step failed once and recovered; the smoke check failed later with a traceback of ours.
RECOVERED_THEN_OURS = log(
    '##[group]Run for attempt in 1 2 3; do',
    'E: Failed to fetch http://security.ubuntu.com/ubuntu/pool/main/b/bubblewrap.deb  503  Service Unavailable',
    '##[warning]apt-get failed (attempt 1); again in 30 s',
    'Setting up xvfb (2:21.1.12-1ubuntu1.5) ...',
    '##[endgroup]',
    '##[group]Run version=$(node -p "require(\'./package.json\').version")',
    'Traceback (most recent call last):',
    '  File "scripts/smoke_native.py", line 120, in <module>',
    'RuntimeError: the window did not show',
    '##[error]Process completed with exit code 1.')
# The same recovered apt error, then a packaging refusal of ours (an error annotation).
RECOVERED_THEN_REFUSAL = log(
    '##[group]Run for attempt in 1 2 3; do',
    'E: Failed to fetch http://mirror/x.deb  503  Service Unavailable',
    '##[endgroup]',
    '##[group]Run python3 scripts/package_linux.py --arch x64',
    '##[error]Expected exactly one amd64 .deb for 0.7.124, found 0.',
    '##[error]Process completed with exit code 1.')
# A signing step with continue-on-error failed on the gallery, its retry passed; the job then
# failed in a later step on the mirror: the failed step decides.
EARLIER_ERROR_THEN_MIRROR = log(
    '##[group]Run Azure/artifact-signing-action@c7ab',
    '##[error]the build changed the source tree',
    '##[endgroup]',
    '##[group]Run sudo apt-get update',
    'E: Failed to fetch http://mirror/x.deb  503  Service Unavailable',
    '##[error]Process completed with exit code 100.')
NOTHING = log('Something else went wrong', '##[error]Process completed with exit code 1.')


class Classify(unittest.TestCase):
    def test_outside_failures_are_infrastructure(self):
        for name, text in (('mirror', MIRROR), ('guard 503', GUARD_503), ('apt loop', APT_LOOP),
                           ('runner', RUNNER), ('npm', NPM), ('script text', SCRIPT_TEXT)):
            self.assertEqual(classify(text)[0], 'infrastructure', name)

    def test_our_own_failures_win_over_any_outside_noise(self):
        for name, text in (('test', TEST), ('guard', GUARD), ('compile', COMPILE), ('own check', OWN_CHECK),
                           ('unknown', NOTHING), ('recovered apt, then a traceback', RECOVERED_THEN_OURS),
                           ('recovered apt, then a refusal', RECOVERED_THEN_REFUSAL)):
            self.assertEqual(classify(text)[0], 'ours', name)

    def test_only_the_failed_step_decides(self):
        self.assertEqual(classify(EARLIER_ERROR_THEN_MIRROR)[0], 'infrastructure')
        self.assertIn('Traceback', classify(RECOVERED_THEN_OURS)[1])

    def test_an_unreadable_log_is_never_a_guess(self):
        self.assertEqual(classify(None), ('ours', 'the log could not be read'))


def job(job_id, conclusion, name='linux (x64, ubuntu-24.04)'):
    return {'id': job_id, 'conclusion': conclusion, 'name': name}


FAILED = {'run_attempt': 1, 'conclusion': 'failure'}


class Decide(unittest.TestCase):
    def test_a_first_attempt_failed_only_by_the_infrastructure_is_rerun(self):
        rerun, lines = decide(FAILED, [job(1, 'failure'), job(2, 'success'), job(3, 'skipped')], {1: MIRROR})
        self.assertTrue(rerun)
        self.assertEqual(len(lines), 1)
        self.assertIn('infrastructure', lines[0])

    def test_one_failure_of_our_own_keeps_the_whole_run_failed(self):
        rerun, lines = decide(FAILED, [job(1, 'failure'), job(2, 'failure', 'windows')], {1: MIRROR, 2: TEST})
        self.assertFalse(rerun)
        self.assertTrue(any(line.startswith('windows: ours') for line in lines))

    def test_a_release_is_retried_once_only(self):
        self.assertFalse(decide(dict(FAILED, run_attempt=2), [job(1, 'failure')], {1: MIRROR})[0])

    def test_only_a_failed_run_with_a_failed_job_is_considered(self):
        self.assertFalse(decide(dict(FAILED, conclusion='cancelled'), [job(1, 'failure')], {1: MIRROR})[0])
        self.assertFalse(decide(FAILED, [job(1, 'success')], {})[0])

    def test_a_timed_out_job_is_not_retried(self):
        self.assertFalse(decide(FAILED, [job(1, 'timed_out')], {1: MIRROR})[0])


if __name__ == '__main__':
    unittest.main()
