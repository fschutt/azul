// Shared pieces of the AzMeet end-to-end scripts (two-clients.mjs, three-clients.mjs): finding the
// binary and the meet Worker, starting and stopping processes with their logs, waiting, the debug
// server's ops, and readers for the lines AzMeet's window shows.
//
// Every reader takes the port of an app's debug server (AZ_DEBUG) and reads the window's texts
// through the `get_node_hierarchy` op, so it sees what a user would see.
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, openSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

/** The repository, the main checkout when this runs from a git worktree (for its target/ and
 *  sibling repos), the AzMeet binary and the meet Worker's directory. */
export function findPaths(opts) {
  const repo = resolve(here, '..', '..', '..');
  let mainRepo = repo;
  try {
    const common = execFileSync('git', ['-C', repo, 'rev-parse', '--path-format=absolute', '--git-common-dir'], {
      encoding: 'utf8',
    }).trim();
    mainRepo = dirname(common);
  } catch {
    // not a git checkout: use this one
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
  return { repo, mainRepo, bin, workerDir };
}

function firstExisting(what, candidates) {
  for (const c of candidates.filter(Boolean)) {
    if (existsSync(c)) return resolve(c);
  }
  throw new Error(`${what} not found; tried:\n  ${candidates.filter(Boolean).join('\n  ')}`);
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** One test run: its log prefix, its deadline, the processes it started and their logs (in a
 *  temporary directory). */
export function createRun({ name, timeoutSecs }) {
  const deadline = Date.now() + timeoutSecs * 1000;
  const logs = mkdtempSync(join(tmpdir(), `azmeet-${name}-`));
  const children = [];

  function log(line) {
    console.log(`[${name}] ${line}`);
  }

  function start(procName, command, args, env) {
    const out = join(logs, `${procName}.out`);
    const err = join(logs, `${procName}.err`);
    const child = spawn(command, args, {
      env: { ...process.env, ...env },
      stdio: ['ignore', openSync(out, 'w'), openSync(err, 'w')],
    });
    child.on('exit', (code, signal) => {
      child.exited = true;
      if (!child.stopping) log(`${procName} exited (${code ?? signal})`);
    });
    const entry = { name: procName, child, out, err };
    children.push(entry);
    return entry;
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

  /** Waits for `check` to return something truthy, polling every half second until the deadline. */
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

  /** Like `until`, but gives up after `ms` (or at the deadline) and returns null. */
  async function within(ms, check) {
    const stop = Math.min(deadline, Date.now() + ms);
    while (Date.now() < stop) {
      try {
        const value = await check();
        if (value) return value;
      } catch {
        // the next poll may answer
      }
      await sleep(500);
    }
    return null;
  }

  /** Prints every process's log tails (on failure). */
  function dumpLogs() {
    for (const { name: procName, out, err } of children) {
      console.log(`\n----- ${procName} stdout (tail) -----\n${tail(out)}`);
      console.log(`----- ${procName} stderr (tail) -----\n${tail(err)}`);
    }
  }

  /** Ends the run: stops every process, keeps the logs on failure (or with `keep`). */
  function finish(passed, keep) {
    stopAll();
    if (passed && !keep) rmSync(logs, { recursive: true, force: true });
    else log(`logs kept in ${logs}`);
    process.exitCode = passed ? 0 : 1;
  }

  return { log, start, stopAll, until, within, dumpLogs, finish, logs, children };
}

export function tail(file, lines = 25) {
  try {
    return readFileSync(file, 'utf8').split('\n').slice(-lines).join('\n');
  } catch {
    return '(no output)';
  }
}

/** A process's stderr so far. */
export function stderrOf(entry) {
  return readFileSync(entry.err, 'utf8');
}

export async function getJson(url, init) {
  const res = await fetch(url, { ...init, signal: AbortSignal.timeout(5000) });
  return { status: res.status, json: await res.json() };
}

/** Starts the meet dev server (in memory) and waits until it answers. */
export async function startWorker(run, workerDir, port) {
  const worker = `http://127.0.0.1:${port}`;
  run.start('worker', process.execPath, [join(workerDir, 'dev-server.mjs'), '--memory', '--port', String(port)], {});
  await run.until('the dev server', async () => (await getJson(`${worker}/health`)).json.ok === true);
  run.log(`dev server up on ${worker}`);
  return worker;
}

/** The names the dev server lists in `room`. */
export async function listedNames(worker, room) {
  const { status, json } = await getJson(`${worker}/rooms/${room}/peers`);
  return status === 200 ? json.peers.map((p) => p.name) : [];
}

/** The environment of a headless AzMeet: tone and test pattern instead of devices, no relays. */
export function appEnv(worker, name, debugPort, extra) {
  return {
    AZ_BACKEND: 'headless',
    AZ_DEBUG: String(debugPort),
    AZMEET_WORKER: worker,
    AZMEET_NAME: name,
    AZMEET_RELAY: 'off',
    AZMEET_TEST_TONE: '1',
    AZMEET_TEST_PATTERN: '1',
    ...extra,
  };
}

/** Waits for the `AZMEET_LINK` and `AZMEET_ROOM` lines of a process that created a meeting. */
export async function meetingOf(run, entry) {
  const link = await run.until(`${entry.name} to create a meeting (AZMEET_LINK on stdout)`, async () => {
    const m = readFileSync(entry.out, 'utf8').match(/^AZMEET_LINK (\S+)$/m);
    return m?.[1];
  });
  const room = readFileSync(entry.out, 'utf8').match(/^AZMEET_ROOM (\S+)$/m)?.[1];
  return { link, room };
}

/** One op on an app's debug server (AZ_DEBUG); it answers once the app has processed it. */
export async function debugOp(debugPort, op) {
  const res = await fetch(`http://127.0.0.1:${debugPort}/`, {
    method: 'POST',
    body: JSON.stringify(typeof op === 'string' ? { op } : op),
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

/** Every text in the app's window. */
export async function texts(debugPort) {
  return strings(await debugOp(debugPort, 'get_node_hierarchy'));
}

/** The first text of the window starting with `prefix`. */
export async function lineStarting(debugPort, prefix) {
  return (await texts(debugPort)).find((t) => t.startsWith(prefix));
}

/** The people-list row of `peerName` ("Ben · connected", "Ben · connected · muted"), if listed
 *  (not the "Ben · waiting for video" tile, not the network panel's line). */
export async function personRow(debugPort, peerName) {
  return (await texts(debugPort)).find(
    (t) => t.startsWith(`${peerName} · `) && !t.includes('waiting for video') && !t.includes(` · to ${peerName}: `),
  );
}

/** Whether the app's window lists `peerName` as connected, whatever its audio state. */
export async function showsConnected(debugPort, peerName) {
  const row = await personRow(debugPort, peerName);
  return row !== undefined && /^[^·]+ · connected( · .*)?$/.test(row.trim());
}

/** The counts of the window's "Audio from <peer>: N packets, M played, ..." line, if shown. */
export async function audioFrom(debugPort, peerName) {
  const line = await lineStarting(debugPort, `Audio from ${peerName}: `);
  const m = line?.match(/: (\d+) packets, (\d+) played, (\d+) silent, (\d+) late, (\d+) buffered/);
  if (!m) return null;
  const [packets, played, silent, late, buffered] = m.slice(1).map(Number);
  return { line, packets, played, silent, late, buffered };
}

/** The window's codec line: "Video: H.264 (VideoToolbox)" or "Video: JPEG (no encoder)". */
export async function codecLine(debugPort) {
  return lineStarting(debugPort, 'Video: ');
}

const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** The window's "Video from <peer> (<source> <height>p[ via <forwarder>]): <codec>, decoded N,
 *  keyframes K, gaps G, dropped D, keyframe requests R" lines for `peerName`, parsed. */
export async function videoLines(debugPort, peerName, source = 'camera') {
  const re = new RegExp(
    `^Video from ${escape(peerName)} \\(${escape(source)}(?: (\\d+)p)?(?: via ([^)]+))?\\): ` +
      '(H\\.264|JPEG), decoded (\\d+), keyframes (\\d+), gaps (\\d+), dropped (\\d+), keyframe requests (\\d+)$',
  );
  const out = [];
  for (const line of await texts(debugPort)) {
    const m = line.match(re);
    if (!m) continue;
    const [decoded, keyframes, gaps, dropped, requests] = m.slice(4).map(Number);
    out.push({
      line,
      height: m[1] ? Number(m[1]) : null,
      via: m[2] ?? null,
      codec: m[3],
      decoded,
      keyframes,
      gaps,
      dropped,
      requests,
    });
  }
  return out;
}

/** One "Video from <peer> ..." line: the one of rendition `height` when given, else the one that
 *  decoded most. */
export async function videoFrom(debugPort, peerName, source = 'camera', height = null) {
  const lines = await videoLines(debugPort, peerName, source);
  const matching = height === null ? lines : lines.filter((l) => l.height === height);
  return matching.sort((a, b) => b.decoded - a.decoded)[0] ?? null;
}

/** The window's "Sending <source> <height>p: N H.264 packets, M JPEG frames, K keyframes, R on
 *  request, P periodic, X reopens, D dropped on purpose" lines, parsed. */
export async function sendingLines(debugPort, source = 'camera') {
  const re = new RegExp(
    `^Sending ${escape(source)}(?: (\\d+)p)?: (\\d+) H\\.264 packets, (\\d+) JPEG frames, (\\d+) keyframes, ` +
      '(\\d+) on request, (\\d+) periodic, (\\d+) reopens, (\\d+) dropped on purpose$',
  );
  const out = [];
  for (const line of await texts(debugPort)) {
    const m = line.match(re);
    if (!m) continue;
    const [h264, jpeg, keyframes, onRequest, periodic, reopens, dropped] = m.slice(2).map(Number);
    out.push({ line, height: m[1] ? Number(m[1]) : null, h264, jpeg, keyframes, onRequest, periodic, reopens, dropped });
  }
  return out;
}

/** One "Sending <source> ..." line: rendition `height` when given, else the one that sent most. */
export async function sending(debugPort, source = 'camera', height = null) {
  const lines = await sendingLines(debugPort, source);
  const matching = height === null ? lines : lines.filter((l) => l.height === height);
  return matching.sort((a, b) => b.h264 + b.jpeg - (a.h264 + a.jpeg))[0] ?? null;
}

/** The network panel's line about `peerName`: "Ben · direct 0.4 ms · backbone · up 50 Mbps ·
 *  to Ben: ... · from Ben: ...", split into its parts. */
export async function peerLine(debugPort, peerName) {
  const line = (await texts(debugPort)).find(
    (t) => t.startsWith(`${peerName} · `) && t.includes(` · to ${peerName}: `),
  );
  if (!line) return null;
  const to = line.match(new RegExp(` · to ${escape(peerName)}: (.*?) · from ${escape(peerName)}: `))?.[1] ?? '';
  const from = line.match(new RegExp(` · from ${escape(peerName)}: (.*)$`))?.[1] ?? '';
  const list = (s) => (s === 'nothing' ? [] : s.split(', '));
  return { line, to: list(to), from: list(from), backbone: line.includes(' · backbone · ') };
}

/** The counts of the network panel's "Forwarded: N packets, M frames, R keyframe requests passed
 *  on" line, if shown. */
export async function forwarded(debugPort) {
  const line = await lineStarting(debugPort, 'Forwarded: ');
  const m = line?.match(/^Forwarded: (\d+) packets, (\d+) frames, (\d+) keyframe requests passed on$/);
  if (!m) return null;
  const [packets, frames, requests] = m.slice(1).map(Number);
  return { line, packets, frames, requests };
}

/** Clicks the first node whose text contains `text` (the debug server's click op). */
export async function click(run, debugPort, text) {
  const answer = await debugOp(debugPort, { op: 'click', text });
  run.log(`click "${text}" on :${debugPort}: ${JSON.stringify(answer).slice(0, 120)}`);
}
