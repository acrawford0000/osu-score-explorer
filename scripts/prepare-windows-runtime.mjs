import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

if (process.platform !== 'win32' || process.arch !== 'x64')
  throw Error('Build this Windows package on Windows x64.');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const source = path.join(root, 'runtime');
const stage = path.resolve(root, 'src-tauri', 'release-runtime');
// Only replace this generated staging folder, never source libraries or user data.
if (
  path.dirname(stage) !== path.resolve(root, 'src-tauri') ||
  path.basename(stage) !== 'release-runtime'
)
  throw Error('Unexpected staging path');
await fs.access(path.join(source, 'node_modules/realm/prebuilds/node/realm.node'));
const node = path.join(source, 'node.exe');
await fs.access(node);
const nodeVersion = execFileSync(node, ['--version'], { encoding: 'utf8' }).trim();
if (
  execFileSync(node, ['-p', "process.platform+'-'+process.arch"], { encoding: 'utf8' }).trim() !==
  'win32-x64'
)
  throw Error('The bundled Node runtime must be Windows x64.');
const license = await fetch(`https://raw.githubusercontent.com/nodejs/node/${nodeVersion}/LICENSE`);
if (!license.ok) throw Error(`Cannot obtain Node.js ${nodeVersion} license: ${license.status}`);
const nodeLicense = await license.text();
await fs.rm(stage, { recursive: true, force: true });
await fs.mkdir(stage, { recursive: true });
for (const name of [
  'node.exe',
  'import-lazer.cjs',
  'statistics.cjs',
  'package.json',
  'package-lock.json',
])
  await fs.copyFile(path.join(source, name), path.join(stage, name));
await fs.cp(path.join(source, 'node_modules'), path.join(stage, 'node_modules'), {
  recursive: true,
  filter: (file) => {
    const relative = path.relative(path.join(source, 'node_modules'), file).replaceAll('\\', '/');
    if (relative === '.bin' || relative.startsWith('.bin/')) return false;
    if (relative.startsWith('realm/prebuilds/'))
      return relative === 'realm/prebuilds/node' || relative.startsWith('realm/prebuilds/node/');
    return true;
  },
});
await fs.writeFile(path.join(stage, 'NODE-LICENSE.txt'), nodeLicense);
for (const name of ['LICENSE', 'THIRD_PARTY_NOTICES.md'])
  await fs.copyFile(path.join(root, name), path.join(stage, name));
// Exclude development Node installations and NODE_PATH from module resolution.
const env = { ...process.env, NODE_PATH: '', NODE_OPTIONS: '' };
execFileSync(
  path.join(stage, 'node.exe'),
  [
    '-e',
    "const Realm=require('realm');if(typeof Realm.schemaVersion!=='function')throw Error('Missing Realm native binding');console.log('Packaged lazer runtime ready');process.exit(0)",
  ],
  { cwd: stage, env, stdio: 'inherit', timeout: 30000 },
);
const realm = JSON.parse(
  await fs.readFile(path.join(stage, 'node_modules/realm/package.json'), 'utf8'),
);
await fs.writeFile(
  path.join(stage, 'RUNTIME.json'),
  JSON.stringify(
    {
      node: nodeVersion,
      realm: realm.version,
      platform: 'windows-x64',
      nodeSha256: createHash('sha256')
        .update(await fs.readFile(node))
        .digest('hex'),
    },
    null,
    2,
  ) + '\n',
);
console.log(`Prepared Windows runtime in ${stage}`);
