// Only use for reads: a timed-out mutation may still complete and must not be retried.
export const READ_TIMEOUT = 'The native app did not respond. Close and reopen Switchboard, then retry.';
export function readWithDeadline<T>(read: Promise<T>, milliseconds = 12_000): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(READ_TIMEOUT)), milliseconds);
    read.then(value => { clearTimeout(timer); resolve(value); }, error => { clearTimeout(timer); reject(error); });
  });
}
