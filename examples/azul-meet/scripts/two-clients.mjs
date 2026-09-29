#!/usr/bin/env node
// Two AzMeet processes meet through the local meet Worker mock.
//
//   1. starts the meet dev server (azul-apps cf-workers/meet/dev-server.mjs, in memory);
//   2. starts AzMeet "Ada" headless with AZMEET_AUTOCREATE=1 and reads the link it prints;
//   3. starts AzMeet "Ben" headless with AZMEET_JOIN=<link>;
//   4. passes once the dev server lists both in the room, and each app's UI (read through its
//      debug server, op get_node_hierarchy) shows the other one as connected.
//
// Usage (from the azul repository, after building AzMeet and libazul with the debug server):
//   node examples/azul-meet/scripts/two-clients.mjs
//     [--bin target/release/AzMeet] [--worker-dir ../azul-apps/cf-workers/meet]
//     [--port 8787] [--debug-a 8765] [--debug-b 8766] [--timeout 90]
//
// Also read from the environment: AZMEET_BIN, AZMEET_WORKER_DIR. Logs go to a temporary
// directory that is printed at the end (kept on failure, or always with --keep-logs).
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, openSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const { values: opts } = parseArgs({
  options: {
    bin: { type: 'string' },
    'worker-dir': { type: 'string' },
    port: { type: 'string', default: '8787' },
    'debug-a': { type: 'string', default: '8765' },
    'debug-b': { type: 'string', default: '8766' },
    timeout: { type: 'string', default: '90' },
    'keep-logs': { type: 'boolean', default: false },
  },
});

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..', '..');
// The main checkout when this runs from a git worktree (for its target/ and sibling repos).
let mainRepo = repo;
try {
  const common = execFileSync('git', ['-C', repo, 'rev-parse', '--path-format=absolute', '--git-common-dir'], {
    encoding: 'utf8',
  }).trim();
  mainRepo = dirname(common);
} catch {
  // not a git checkout: use this one
}

function firstExisting(what, candidates) {
  for (const c of candidates.filter(Boolean)) {
    if (existsSync(c)) return resolve(c);
  }
  throw new Error(`${what} not found; tried:\n  ${candidates.filter(Boolean).join('\n  ')}`);
}

const bin = firstExisting('the AzMeet binary (pass --bin)', [
  opts.bin,
  process.env.AZMEET_BIN,
  join(repo, 'target', 'release', 'AzMeet'),
  join(repo, 'target', 'debug', 'AzMeet'),
  join(mainRepo, 'target', 'release', 'AzMeet'),
  join(mainRepo, 'target', 'debug', 'AzMeet'),
]);
const workerDir = firstExisting('the meet Worker (pass --worker-dir)', [
  opts['worker-dir'],
  process.env.AZMEET_WORKER_DIR,
  join(mainRepo, '..', 'azul-apps-m1', 'cf-workers', 'meet'),
  join(mainRepo, '..', 'azul-apps', 'cf-workers', 'meet'),
  join(repo, '..', 'azul-apps', 'cf-workers', 'meet'),
]);

const port = Number(opts.port);
const worker = `http://127.0.0.1:${port}`;
const debugA = Number(opts['debug-a']);
const debugB = Number(opts['debug-b']);
const deadline = Date.now() + Number(opts.timeout) * 1000;
const logs = mkdtempSync(join(tmpdir(), 'azmeet-two-clients-'));
const children = [];

function log(line) {
  console.log(`[two-clients] ${line}`);
}

function start(name, command, args, env) {
  const out = join(logs, `${name}.out`);
  const err = join(logs, `${name}.err`);
  const child = spawn(command, args, {
    env: { ...process.env, ...env },
    stdio: ['ignore', openSync(out, 'w'), openSync(err, 'w')],
  });
  child.on('exit', (code, signal) => {
    child.exited = true;
    if (!child.stopping) log(`${name} exited (${code ?? signal})`);
  });
  children.push({ name, child, out, err });
  return { out, err, child };
}

function stopAll() {
  for (const { child } of children) {
    child.stopping = true;
    if (!child.exited) child.kill('SIGTERM');
  }
  setTimeout(() => {
    for (const { child } of children) if (!child.exited) child.kill('SIGKILL');
  }, 2000).unref();
}

function tail(file, lines = 25) {
  try {
    return readFileSync(file, 'utf8').split('\n').slice(-lines).join('\n');
  } catch {
    return '(no output)';
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function until(what, check) {
  let last = null;
  while (Date.now() < deadline) {
    try {
      const value = await check();
      if (value) return value;
    } catch (e) {
      last = e;
    }
    await sleep(500);
  }
  throw new Error(`timed out waiting for ${what}${last ? ` (last error: ${last.message})` : ''}`);
}

async function getJson(url, init) {
  const res = await fetch(url, { ...init, signal: AbortSignal.timeout(5000) });
  return { status: res.status, json: await res.json() };
}

/** One op on an app's debug server (AZ_DEBUG); it answers once the app has processed it. */
async function debugOp(debugPort, op) {
  const res = await fetch(`http://127.0.0.1:${debugPort}/`, {
    method: 'POST',
    body: JSON.stringify({ op }),
    signal: AbortSignal.timeout(10000),
  });
  return res.json();
}

/** Every string in a JSON value (the hierarchy's `text` fields among them). */
function strings(value, out = []) {
  if (typeof value === 'string') out.push(value);
  else if (Array.isArray(value)) value.forEach((v) => strings(v, out));
  else if (value && typeof value === 'object') Object.values(value).forEach((v) => strings(v, out));
  return out;
}

/** Whether the app's window lists `peerName` as connected ("Ben · connected"). */
async function showsConnected(debugPort, peerName) {
  const hierarchy = await debugOp(debugPort, 'get_node_hierarchy');
  return strings(hierarchy).some((t) => t.startsWith(peerName) && t.trimEnd().endsWith('connected'));
}

function appEnv(name, debugPort, extra) {
  return {
    AZ_BACKEND: 'headless',
    AZ_DEBUG: String(debugPort),
    AZMEET_WORKER: worker,
    AZMEET_NAME: name,
    AZMEET_RELAY: 'off',
    ...extra,
  };
}

let passed = false;
try {
  log(`AzMeet: ${bin}`);
  log(`meet Worker: ${workerDir}`);
  log(`logs: ${logs}`);

  start('worker', process.execPath, [join(workerDir, 'dev-server.mjs'), '--memory', '--port', String(port)], {});
  await until('the dev server', async () => (await getJson(`${worker}/health`)).json.ok === true);
  log(`dev server up on ${worker}`);

  const ada = start('ada', bin, [], appEnv('Ada', debugA, { AZMEET_AUTOCREATE: '1' }));
  const link = await until('Ada to create a meeting (AZMEET_LINK on stdout)', async () => {
    const m = readFileSync(ada.out, 'utf8').match(/^AZMEET_LINK (\S+)$/m);
    return m?.[1];
  });
  const room = readFileSync(ada.out, 'utf8').match(/^AZMEET_ROOM (\S+)$/m)?.[1];
  log(`Ada created ${link}`);

  start('ben', bin, [], appEnv('Ben', debugB, { AZMEET_JOIN: link }));

  await until('the dev server to list Ada and Ben in the room', async () => {
    const { status, json } = await getJson(`${worker}/rooms/${room}/peers`);
    const names = status === 200 ? json.peers.map((p) => p.name) : [];
    return names.includes('Ada') && names.includes('Ben');
  });
  log('the dev server lists Ada and Ben');

  await until("Ada's window to show Ben as connected", () => showsConnected(debugA, 'Ben'));
  log("Ada's window shows Ben as connected");
  await until("Ben's window to show Ada as connected", () => showsConnected(debugB, 'Ada'));
  log("Ben's window shows Ada as connected");

  const state = await debugOp(debugA, 'get_state');
  log(`Ada's debug server answers get_state: ${state.status ?? JSON.stringify(state).slice(0, 80)}`);
  passed = true;
  log('PASS: two AzMeet clients met through the meet Worker mock and connected over iroh');
} catch (e) {
  log(`FAIL: ${e.message}`);
  for (const { name, out, err } of children) {
    console.log(`\n----- ${name} stdout (tail) -----\n${tail(out)}`);
    console.log(`----- ${name} stderr (tail) -----\n${tail(err)}`);
  }
} finally {
  stopAll();
  if (passed && !opts['keep-logs']) rmSync(logs, { recursive: true, force: true });
  else log(`logs kept in ${logs}`);
  process.exitCode = passed ? 0 : 1;
}
