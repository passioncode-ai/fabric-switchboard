#!/usr/bin/env python3
"""Run a failed release's failed jobs once more when every failure was the infrastructure's
(SB-563, M4-06). Started by `.github/workflows/release-rerun.yml` when a `release` run on a tag
completes with failure; never on a push or a pull request, and never a full suite.

  python3 scripts/release_rerun.py --repo OWNER/NAME --run-id ID [--dry-run]

The rule is narrow on purpose:
  - only a run's first attempt of a tag push is retried, so a release is retried at most once;
  - in every failed job, the failed step's part of the log must show an infrastructure failure
    (a package mirror, a registry or the runner itself, the patterns in INFRASTRUCTURE) and none
    of ours (a test, a compile error, a traceback, a refusal of the guard or of a check, the
    patterns in OURS); what earlier steps printed and recovered from does not count;
  - a job whose log cannot be read counts as ours: nothing is retried on a guess.
Then `gh run rerun ID --failed` starts the failed jobs and the jobs that need them. Otherwise the
run stays failed and the summary names the job and the line that decided it.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

# A transient failure of something outside this repository: worth one more attempt.
INFRASTRUCTURE = [
    ('package mirror', re.compile(r'^[EW]: (Failed to fetch|Unable to fetch some archives|Some index files failed to download|'
                                  r'Could not (connect|resolve)|Temporary failure resolving|.*Hash Sum mismatch)', re.MULTILINE)),
    # The retry loop around apt-get in nightly.yml and release.yml, after its last attempt.
    ('package mirror', re.compile(r'the package mirror failed \d+ times')),
    ('npm registry', re.compile(r'npm (ERR!|error) (code )?(ECONNRESET|ETIMEDOUT|EAI_AGAIN|ECONNREFUSED|E50[0-4])')),
    ('crates.io', re.compile(r'failed to download from `https://(static\.)?crates\.io|'
                             r'failed to get `[^`]+` as a dependency of package')),
    ('rustup', re.compile(r'error: (could not download file|component download failed)')),
    ('GitHub', re.compile(r'Failed to download action|Response status code does not indicate success: 5\d\d|'
                          r'Request failed with status code 5\d\d|HTTP 50[0-4]|HTTP Error 50[0-4]|502 Bad Gateway|'
                          r'503 Service Unavailable|504 Gateway Time-?out')),
    ('runner', re.compile(r'The runner has received a shutdown signal|lost communication with the server|'
                          r'The hosted runner encountered an error')),
    ('PowerShell Gallery', re.compile(r"Package '[^']+' failed to download|Unable to find repository 'PSGallery'")),
]

# A failure of this repository's own code, whatever else the log says: never retried.
OURS = [
    ('test', re.compile(r'test result: FAILED|^failures:$|panicked at ', re.MULTILINE)),
    ('compile', re.compile(r'^error(\[E\d+\])?: (could not compile|aborting due to)|^error\[E\d+\]:', re.MULTILINE)),
    ('script', re.compile(r'^Traceback \(most recent call last\):', re.MULTILINE)),
]
# How GitHub writes an error annotation into a raw job log, and the one every failed step adds.
ANNOTATION = re.compile(r'^##\[error\](.*)$', re.MULTILINE)
EXIT_CODE = re.compile(r'^Process completed with exit code \d+\.?$')
TIMESTAMP = re.compile(r'^\d{4}-\d\d-\d\dT[\d:.]+Z ', re.MULTILINE)
COLOUR = re.compile(r'\x1b\[[0-9;]*m')


def infrastructure(text: str) -> str | None:
    for name, pattern in INFRASTRUCTURE:
        found = pattern.search(text)
        if found:
            return f'{name}: {found.group(0).strip()[:160]}'
    return None


STEP_START = re.compile(r'^##\[group\]Run ', re.MULTILINE)


def failed_section(text: str) -> str:
    """The part of a raw job log that belongs to the step that failed the job.

    A raw log marks each `run:` and `uses:` step with a `##[group]Run ...` line and the failure of
    a step with an error annotation, so the failed step is the section from the last `##[group]Run`
    before the job's last error annotation to that annotation. Earlier steps are left out: an apt
    mirror that failed and recovered in the dependency step says nothing about a smoke check that
    failed later. A failure before any step (setting up the job, downloading an action) is the
    log up to the error; a log without an error annotation is read whole."""
    errors = list(ANNOTATION.finditer(text))
    if not errors:
        return text
    last = errors[-1]
    starts = [m.start() for m in STEP_START.finditer(text, 0, last.start())]
    return text[starts[-1] if starts else 0:last.end()]


def classify(log: str | None) -> tuple[str, str]:
    """('infrastructure', what) when the failed step shows only a transient outside failure, else
    ('ours', why): a failure of our own, or nothing recognised (no guess is retried).

    Only the failed step's section counts (`failed_section`). In it, a failed test, a compile error
    or a Python traceback decides `ours`; so does every error annotation (`##[error]`, which
    `::error::` writes) that is neither the step's exit code nor an outside failure: a refusal of
    the guard, the preflight, a signature check or the feed check is ours."""
    if log is None:
        return 'ours', 'the log could not be read'
    text = failed_section(TIMESTAMP.sub('', COLOUR.sub('', log)))
    for name, pattern in OURS:
        found = pattern.search(text)
        if found:
            return 'ours', f'{name}: {found.group(0).strip()[:160]}'
    for found in ANNOTATION.finditer(text):
        message = found.group(1).strip()
        if not EXIT_CODE.match(message) and not infrastructure(message):
            return 'ours', f'error: {message[:160]}'
    what = infrastructure(text)
    if what:
        return 'infrastructure', what
    return 'ours', 'no infrastructure failure recognised in the failed step'


def decide(run: dict, jobs: list[dict], logs: dict) -> tuple[bool, list[str]]:
    """Whether to rerun the failed jobs, and one line per failed job saying why."""
    if run.get('run_attempt', 1) != 1:
        return False, [f'attempt {run.get("run_attempt")}: a release is retried once only']
    if run.get('conclusion') != 'failure':
        return False, [f'the run concluded {run.get("conclusion")}, not failure']
    failed = [job for job in jobs if job.get('conclusion') in ('failure', 'timed_out')]
    if not failed:
        return False, ['no failed job']
    lines, rerun = [], True
    for job in failed:
        if job.get('conclusion') == 'timed_out':
            kind, why = 'ours', 'the job ran out of time'
        else:
            kind, why = classify(logs.get(job['id']))
        lines.append(f'{job.get("name")}: {kind} ({why})')
        rerun = rerun and kind == 'infrastructure'
    return rerun, lines


def gh(*args: str) -> str:
    done = subprocess.run(['gh', *args], capture_output=True, encoding='utf-8', errors='replace',
                          stdin=subprocess.DEVNULL)
    if done.returncode:
        raise RuntimeError(f'gh {" ".join(args[:3])}: {(done.stderr or "").strip()[:300]}')
    return done.stdout


def log_of(repo: str, job_id: int) -> str | None:
    try:
        # Raw logs carry the runner's colour codes, which `gh` refuses to print without this flag.
        return gh('api', '--allow-escape-sequences', f'repos/{repo}/actions/jobs/{job_id}/logs')
    except RuntimeError:
        return None


def summary(line: str) -> None:
    print(line)
    path = os.environ.get('GITHUB_STEP_SUMMARY')
    if path:
        with open(path, 'a', encoding='utf-8') as handle:
            handle.write(line + '\n')


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--run-id', required=True, type=int)
    parser.add_argument('--dry-run', action='store_true', help='decide and say, rerun nothing')
    args = parser.parse_args(argv)
    run = json.loads(gh('api', f'repos/{args.repo}/actions/runs/{args.run_id}'))
    attempt = run.get('run_attempt', 1)
    jobs = json.loads(gh('api', f'repos/{args.repo}/actions/runs/{args.run_id}/attempts/{attempt}/jobs?per_page=100'))['jobs']
    failed = [job for job in jobs if job.get('conclusion') == 'failure']
    logs = {job['id']: log_of(args.repo, job['id']) for job in failed}
    rerun, lines = decide(run, jobs, logs)
    summary(f'release run {args.run_id} ({run.get("head_branch")}, attempt {attempt}):')
    for line in lines:
        summary(f'- {line}')
    if not rerun:
        summary('Not retried: a failure of our own, or one this rule does not recognise, needs a person or an agent.')
        print(f'::warning title=release not retried::{run.get("head_branch")}: {lines[0] if lines else "nothing to do"}')
        return 0
    if args.dry_run:
        summary('Would rerun the failed jobs once (dry run).')
        return 0
    gh('run', 'rerun', str(args.run_id), '--failed', '-R', args.repo)
    summary('Every failure was the infrastructure\'s: the failed jobs run once more (attempt 2).')
    print(f'::notice title=release retried::{run.get("head_branch")}: failed jobs started again')
    return 0


if __name__ == '__main__':
    sys.exit(main())
