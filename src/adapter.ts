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
  launch: (id, mode, workingDirectory) => invoke('launch_account', { id, mode, workingDirectory }),
  beginLogin: (input) => invoke('begin_login', { ...input }),
  finishLogin: (loginId) => invoke('finish_login', { loginId }),
  cancelLogin: (loginId) => invoke('cancel_login', { loginId }),
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
  'Choose an existing project directory.',
  'Close the existing session before changing its home.',
  'Select another account before removing this one.',
  'Select this account before launching managed mode.',
  'Captured Codex login has no identity token. Sign in again or use managed mode.',
  'Account saved; isolated login cleanup needs attention.',
  'Finish or close sign-in in Terminal before cancelling.',
]);
const coreErrors: Record<string, string> = {
  'Codex does not support setup tokens': 'Choose an API key, imported OAuth JSON, or official sign-in for Codex.',
  'Credential format is invalid': 'Check the credential format and enter it again.',
  'Credential input is empty or too large': 'Enter a credential within the supported size limit.',
  'OAuth JSON is invalid': 'Enter valid credential JSON for the selected provider.',
  'Unsupported OAuth JSON schema': 'This OAuth JSON format is not supported. Use the official sign-in flow instead.',
  'OAuth access token is missing': 'The imported JSON has no access token. Sign in again or import a complete credential.',
  'OAuth field type is invalid': 'The OAuth JSON contains an invalid field. Use a complete provider credential export.',
  'OAuth expiration is invalid': 'The OAuth expiration is invalid. Sign in again or import a complete credential.',
  'Label or pool is invalid': 'Enter a label and a pool using lowercase letters, numbers, hyphens, or underscores.',
  'Label is invalid': 'Enter a nonempty account label within the supported length limit.',
  'Account limit reached': 'The account limit has been reached. Remove an unused account before adding another.',
  'Account credential already exists in this pool': 'This credential already exists in this provider and pool. Use the existing account.',
  'Select another account or disable this account before removing it': 'Select another account in this pool, or disable this account, before removing it.',
  'Account is disabled': 'This account is disabled. Enable it before continuing.',
  'Credential expired; reauthenticate this account': 'This credential has expired. Add an account through official sign-in again.',
  'Credential storage unavailable; reauthenticate this account': 'The stored credential is unavailable. Check Keychain access or sign in again.',
  'No account selected for this provider and pool': 'Select an account in this provider and pool before launching managed mode.',
  'Account is unavailable in this provider and pool': 'This account is unavailable in the chosen provider and pool. Refresh the account list.',
  'Selected account unavailable': 'The selected account is unavailable. Refresh and select an enabled account.',
  'Account not found': 'This account no longer exists. Refresh the account list.',
  'Invalid usage observation': 'The provider returned an invalid usage observation. The last observation is unchanged.',
  'Usage observation is older than the stored observation': 'The provider returned an older observation. The last observation is unchanged.',
  'Another Switchboard instance owns this account storage': 'Another Switchboard window owns the account storage. Close that instance, then retry.',
  'Account metadata is corrupt; restore a known-good backup': 'Account metadata could not be read. Restore a known-good backup before continuing.',
  'Unsupported account metadata version': 'This account metadata version is not supported. Open it with a compatible Switchboard version.',
  'Account metadata could not be saved': 'Account metadata could not be saved. Check storage access and retry.',
  'Storage failure; credential cleanup requires recovery': 'The account could not be saved and credential cleanup needs recovery. Check storage before retrying.',
  'Native vault is not implemented on this platform': 'Native credential storage is not available on this platform. Use the macOS app.',
  'Secure metadata storage is not implemented on this platform': 'Secure account storage is not available on this platform. Use the macOS app.',
};
for (const error of ['Vault unavailable', 'Credential unavailable', 'Native credential storage unavailable', 'Credential storage unavailable', 'Private account storage unavailable', 'Account store unavailable']) coreErrors[error] = 'Storage unavailable. Check Keychain access and retry.';
for (const error of ['Unsafe account storage file', 'Unsafe account storage directory', 'Unsafe account metadata file', 'Invalid account metadata', 'Invalid route metadata', 'Invalid event metadata', 'Invalid metadata bounds', 'Account metadata exceeds size limit']) coreErrors[error] = 'Account storage failed validation. Restore a known-good backup or check the app’s storage permissions before retrying.';
export function safeError(error: unknown): string {
  const candidate = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
  return safeErrors.has(candidate) ? candidate : Object.hasOwn(coreErrors, candidate) ? coreErrors[candidate] : 'The operation could not be completed. Check your input and Keychain access, then retry.';
}
