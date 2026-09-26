import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Adapter } from './types';

export const native = isTauri();
export const demo = !native && new URLSearchParams(location.search).get('demo') === '1';
export const nativeAdapter: Adapter = {
  snapshot: () => invoke('snapshot'),
  runtime: () => invoke('runtime_status'),
  add: (input) => invoke('add_account', { ...input }),
  update: (id, label, enabled) => invoke('update_account', { id, label, enabled }),
  remove: (id) => invoke('remove_account', { id }),
  select: ({ id, provider, pool }) => invoke('select_account', { id, provider, pool }),
  launch: (id, mode) => invoke('launch_account', { id, mode }),
  beginLogin: (input) => invoke('begin_login', { ...input }),
  finishLogin: (loginId) => invoke('finish_login', { loginId }),
  probe: (id) => invoke('probe_usage', { id }),
};

// Only exact, fixed backend vocabulary is surfaced. Unknown failures never print
// raw provider output, credentials, filesystem paths, or serialized error objects.
const safeErrors = new Set([
  'Provider CLI not found. Install the official CLI and retry.',
  'Sign-in is not complete. Finish in Terminal, then try again.',
  'Managed mode requires a selected account in this pool.',
  'Usage unavailable for this credential type.',
  'Provider rejected the credential. Sign in again.',
  'Storage unavailable. Check Keychain access and retry.',
  'Select another account in this pool, or disable this account, before removing it.',
  'This account is disabled. Enable it before continuing.',
  'Enter valid credential JSON for the selected provider.',
  'Enter a credential before adding the account.',
  'This credential type is not supported by this provider.',
]);
export function safeError(error: unknown): string {
  const candidate = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
  return safeErrors.has(candidate) ? candidate : 'The operation could not be completed. Check your input and Keychain access, then retry.';
}
