// Deterministic desktop assets from the vendored canonical Switchboard mark.
import { mkdtemp, copyFile, rm, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const scratch = await mkdtemp(join(tmpdir(), 'switchboard-icons-'));
try {
  execFileSync('node_modules/.bin/tauri', ['icon', 'brand/passioncode/switchboard-mark.svg', '--output', scratch], { stdio: 'inherit' });
  // macOS: the Dock icon follows Apple's grid (an 824-px tile on a 1024 canvas, soft shadow in
  // the margin), drawn by the same script as Project Observatory's, then packed by iconutil.
  if (process.platform !== 'darwin') throw new Error('Generate the icons on macOS: icon.icns needs make-icon.swift and iconutil.');
  const iconset = join(scratch, 'icon.iconset');
  execFileSync('swift', ['scripts/make-icon.swift', 'brand/passioncode/switchboard-mark.svg', iconset], { stdio: 'inherit' });
  execFileSync('iconutil', ['-c', 'icns', iconset, '-o', join(scratch, 'icon.icns')], { stdio: 'inherit' });
  // Tauri's ICNS encoder emits a map of chunks in nondeterministic order.
  // ICNS chunks are self-contained; sort their four-byte types for stable bytes.
  const icnsPath = join(scratch, 'icon.icns');
  const icns = await readFile(icnsPath);
  const chunks = [];
  for (let offset = 8; offset < icns.length;) {
    const size = icns.readUInt32BE(offset + 4);
    if (size < 8 || offset + size > icns.length) throw new Error('Invalid generated ICNS chunk');
    chunks.push(icns.subarray(offset, offset + size));
    offset += size;
  }
  chunks.sort((a, b) => Buffer.compare(a.subarray(0, 4), b.subarray(0, 4)));
  await writeFile(icnsPath, Buffer.concat([icns.subarray(0, 8), ...chunks]));
  const hashes = {};
  for (const name of ['icon.png', 'icon.icns', 'icon.ico']) {
    const target = `src-tauri/icons/${name}`;
    await copyFile(join(scratch, name), target);
    hashes[target] = createHash('sha256').update(await readFile(target)).digest('hex');
  }
  await writeFile('brand/passioncode/native-icons.json', JSON.stringify({
    source: 'brand/passioncode/switchboard-mark.svg',
    generator: `@tauri-apps/cli ${JSON.parse(await readFile('node_modules/@tauri-apps/cli/package.json', 'utf8')).version} (png, ico); scripts/make-icon.swift + iconutil (icns)`,
    command: 'node scripts/generate-icons.mjs',
    sha256: hashes,
  }, null, 2) + '\n');
} finally { await rm(scratch, { recursive: true, force: true }); }
