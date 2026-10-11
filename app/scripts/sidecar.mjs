// Builds howdah-tourney (the engine test daemon) in release mode and puts
// it where Tauri looks for the sidecar: src-tauri/binaries/howdah-tourney-
// <target triple>[.exe]. Release builds bundle it with
// `--config src-tauri/tauri.sidecar.conf.json` (see release.yml); plain
// builds and `tauri dev` don't need it.
//
//   npm run sidecar [-- --target <triple>]
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
const root = join(app, '..');
const args = process.argv.slice(2);
const at = args.indexOf('--target');
const target =
  at >= 0
    ? args[at + 1]
    : execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
        .split('\n')
        .find((l) => l.startsWith('host: '))
        .slice('host: '.length)
        .trim();

const cargo = ['build', '--release', '-p', 'howdah-tourney', '--bin', 'howdah-tourney'];
if (at >= 0) cargo.push('--target', target);
execFileSync('cargo', cargo, { cwd: root, stdio: 'inherit' });

const exe = target.includes('windows') ? '.exe' : '';
const built = join(root, 'target', ...(at >= 0 ? [target] : []), 'release', `howdah-tourney${exe}`);
const dest = join(app, 'src-tauri', 'binaries', `howdah-tourney-${target}${exe}`);
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(built, dest);
console.log(`sidecar: ${dest}`);
