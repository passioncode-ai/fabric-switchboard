import { t } from './i18n';
// Runtime platform comes from the native backend, never the browser user agent.
// This is a syntax hint only; the backend authoritatively checks existence and
// canonicalizes the path on the machine where it will launch the session.
export function platformLabel(platform?: string): string {
  if (platform === 'macos') return 'macOS';
  if (platform === 'windows') return 'Windows';
  if (platform === 'linux') return 'Linux';
  return t('Platform unavailable');
}

export function projectPathExample(platform?: string): string {
  return platform === 'windows' ? 'C:\\Users\\you\\Projects\\my-project' : '/Users/you/Projects/my-project';
}

export function isAbsoluteProjectPath(path: string, platform?: string): boolean {
  if (!path || /[\u0000-\u001f\u007f]/.test(path)) return false;
  const posix = path.startsWith('/');
  const drive = /^[a-z]:[\\/]/i.test(path);
  // UNC requires both a server and a share. Device/extended namespaces are not
  // offered by this form; a normal drive or UNC path keeps the hint unambiguous.
  const unc = /^\\\\[^\\/?.][^\\/]*[\\/][^\\/]+(?:[\\/]|$)/.test(path);
  if (platform === 'windows') return drive || unc;
  if (platform === 'macos' || platform === 'linux') return posix;
  // Runtime status can temporarily fail while isolated launch is still usable.
  // Avoid rejecting a valid native path solely because status is unavailable.
  return posix || drive || unc;
}
