#!/usr/bin/env node
// Two AzMeet processes meet through the local meet Worker mock, hear and see each other, and one leaves.
//
//   1. starts the meet dev server (azul-apps cf-workers/meet/dev-server.mjs, in memory);
//   2. starts AzMeet "Ada" headless with AZMEET_AUTOCREATE=1 and reads the link it prints;
//   3. starts AzMeet "Ben" headless with AZMEET_JOIN=<link>;
//   4. waits until the dev server lists both in the room, and each app's UI (read through its
//      debug server, op get_node_hierarchy) shows the other one as connected;
//   5. audio: both run with AZMEET_TEST_TONE=1, so a 440 Hz tone replaces the microphone (a
//      headless run never opens an audio device: no capture, and received audio is counted, not
//      played). Each window's "Audio from <other>: N packets, M played, ..." line must count at
//      least a second of packets taken into its jitter buffer and half a second played;
//   6. video: both run with AZMEET_TEST_PATTERN=1, so moving colour bars replace the camera (a
//      headless run never opens a camera or a screen). Each window's codec line says H.264 (a
//      working encoder, VideoToolbox on macOS) or JPEG (no encoder); each window's "Video from
//      <other> (camera): <codec>, decoded N, keyframes K, ..." line must count at least 30 decoded
//      frames (2 s at 15 fps) and a keyframe;
//   7. loss: Ben clicks "Drop a video packet" (shown with AZMEET_TEST_PATTERN=1): Ada's window
//      counts a gap. With H.264 she asks Ben for a keyframe (her "keyframe requests" count rises,
//      Ben's "on request" count rises) and decodes again (15 more frames, one more keyframe); if
//      the packet after the dropped one happened to be a keyframe no request is needed, and the
//      drop is repeated (up to 3 times). With JPEG every frame stands alone: no request, decoding
//      goes on. --require-h264 fails a run that fell back to JPEG;
//   8. Ben clicks "Mute": Ada's window shows "Ben · connected · muted" (the control message);
//   9. Ben clicks "Leave": his window returns to the start screen, the dev server stops listing
//      him at once (DELETE, not the 120 s TTL), and Ada's window stops listing him.
//
// Usage (from the azul repository, after building AzMeet and libazul with the debug server):
//   node examples/azul-meet/scripts/two-clients.mjs
//     [--bin target/release/AzMeet] [--worker-dir ../azul-apps/cf-workers/meet]
//     [--port 8787] [--debug-a 8765] [--debug-b 8766] [--timeout 90] [--require-h264]
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
    'require-h264': { type: 'boolean', default: false },
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

async function getJson(url, init) {
  const res = await fetch(url, { ...init, signal: AbortSignal.timeout(5000) });
  return { status: res.status, json: await res.json() };
}

/** One op on an app's debug server (AZ_DEBUG); it answers once the app has processed it. */
async function debugOp(debugPort, op) {
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
async function texts(debugPort) {
  return strings(await debugOp(debugPort, 'get_node_hierarchy'));
}

/** The people-list row of `peerName` ("Ben · connected", "Ben · connected · muted"), if listed
 *  (not the "Ben · waiting for video" tile). */
async function personRow(debugPort, peerName) {
  return (await texts(debugPort)).find((t) => t.startsWith(`${peerName} · `) && !t.includes('waiting for video'));
}

/** Whether the app's window lists `peerName` as connected, whatever its audio state. */
async function showsConnected(debugPort, peerName) {
  const row = await personRow(debugPort, peerName);
  return row !== undefined && /^[^·]+ · connected( · .*)?$/.test(row.trim());
}

/** The counts of the window's "Audio from <peer>: N packets, M played, ..." line, if shown. */
async function audioFrom(debugPort, peerName) {
  const line = (await texts(debugPort)).find((t) => t.startsWith(`Audio from ${peerName}: `));
  const m = line?.match(/: (\d+) packets, (\d+) played, (\d+) silent, (\d+) late, (\d+) buffered/);
  if (!m) return null;
  const [packets, played, silent, late, buffered] = m.slice(1).map(Number);
  return { line, packets, played, silent, late, buffered };
}

/** The window's codec line: "Video: H.264 (VideoToolbox)" or "Video: JPEG (no encoder)". */
async function codecLine(debugPort) {
  return (await texts(debugPort)).find((t) => t.startsWith('Video: '));
}

/** The counts of the window's "Video from <peer> (<source>): <codec>, decoded N, keyframes K,
 *  gaps G, dropped D, keyframe requests R" line, if shown. */
async function videoFrom(debugPort, peerName, source = 'camera') {
  const prefix = `Video from ${peerName} (${source}): `;
  const line = (await texts(debugPort)).find((t) => t.startsWith(prefix));
  const m = line?.match(
    /: (H\.264|JPEG), decoded (\d+), keyframes (\d+), gaps (\d+), dropped (\d+), keyframe requests (\d+)$/,
  );
  if (!m) return null;
  const [decoded, keyframes, gaps, dropped, requests] = m.slice(2).map(Number);
  return { line, codec: m[1], decoded, keyframes, gaps, dropped, requests };
}

/** The counts of the window's "Sending <source>: N H.264 packets, M JPEG frames, K keyframes,
 *  R on request, ..." line, if shown. */
async function sending(debugPort, source = 'camera') {
  const line = (await texts(debugPort)).find((t) => t.startsWith(`Sending ${source}: `));
  const m = line?.match(
    /: (\d+) H\.264 packets, (\d+) JPEG frames, (\d+) keyframes, (\d+) on request, (\d+) periodic, (\d+) reopens, (\d+) dropped on purpose$/,
  );
  if (!m) return null;
  const [h264, jpeg, keyframes, onRequest, periodic, reopens, dropped] = m.slice(1).map(Number);
  return { line, h264, jpeg, keyframes, onRequest, periodic, reopens, dropped };
}

/** Clicks the first node whose text contains `text` (the debug server's click op). */
async function click(debugPort, text) {
  const answer = await debugOp(debugPort, { op: 'click', text });
  log(`click "${text}" on :${debugPort}: ${JSON.stringify(answer).slice(0, 120)}`);
}

function appEnv(name, debugPort, extra) {
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

  // Audio: a second of packets in each jitter buffer, half a second played (to nothing: headless).
  for (const [listener, port, speaker] of [['Ada', debugA, 'Ben'], ['Ben', debugB, 'Ada']]) {
    const heard = await until(`${listener}'s window to count a second of audio from ${speaker}`, async () => {
      const a = await audioFrom(port, speaker);
      return a && a.packets >= 50 && a.played >= 25 ? a : null;
    });
    log(`${listener}: ${heard.line}`);
  }
  for (const [name, child] of [['ada', ada], ['ben', children.find((c) => c.name === 'ben')]]) {
    const err = readFileSync(child.err, 'utf8');
    if (!err.includes('no audio device is opened')) {
      throw new Error(`${name} did not say it runs without audio devices (see its stderr)`);
    }
  }
  log('both apps run without audio devices: the tone replaces the mic, playback is counted');

  // Video: the test pattern goes out as H.264 where an encoder works, else as JPEG.
  const codecs = [];
  for (const [name, port] of [['Ada', debugA], ['Ben', debugB]]) {
    const line = await until(`${name}'s window to show its video codec`, () => codecLine(port));
    log(`${name}: ${line}`);
    codecs.push(line);
  }
  const h264 = codecs.every((line) => line.startsWith('Video: H.264'));
  if (!h264 && opts['require-h264']) {
    throw new Error(`--require-h264, but the video is not H.264 on both sides: ${codecs.join(' / ')}`);
  }
  const codec = h264 ? 'H.264' : 'JPEG';
  for (const [listener, port, sender] of [['Ada', debugA, 'Ben'], ['Ben', debugB, 'Ada']]) {
    const seen = await until(`${listener}'s window to decode 2 s of ${codec} video from ${sender}`, async () => {
      const v = await videoFrom(port, sender);
      return v && v.codec === codec && v.decoded >= 30 && v.keyframes >= 1 ? v : null;
    });
    log(`${listener}: ${seen.line}`);
  }
  for (const [name, child] of [['ada', ada], ['ben', children.find((c) => c.name === 'ben')]]) {
    if (!readFileSync(child.err, 'utf8').includes('no camera or screen is opened')) {
      throw new Error(`${name} did not say it runs without a camera or screen (see its stderr)`);
    }
  }
  log('both apps send the test pattern: no camera or screen is opened');

  // Loss: Ben drops one video packet before it leaves.
  let recovered = null;
  for (let attempt = 1; attempt <= 3 && !recovered; attempt++) {
    const before = await until("Ada's video line before the drop", () => videoFrom(debugA, 'Ben'));
    const sentBefore = await until("Ben's sending line before the drop", () => sending(debugB));
    await click(debugB, 'Drop a video packet');
    const gap = await within(15000, async () => {
      const v = await videoFrom(debugA, 'Ben');
      return v && v.gaps > before.gaps ? v : null;
    });
    if (!gap) throw new Error("Ada's window never counted Ben's dropped packet as a gap");
    log(`Ada after the drop: ${gap.line}`);
    const sentAfter = await until("Ben's window to count the dropped packet", async () => {
      const s = await sending(debugB);
      return s && s.dropped > sentBefore.dropped ? s : null;
    });
    if (!h264) {
      if (gap.requests !== before.requests) throw new Error('a lost JPEG frame asked for a keyframe');
      recovered = await until('JPEG decoding to go on after the lost frame', async () => {
        const v = await videoFrom(debugA, 'Ben');
        return v && v.decoded >= gap.decoded + 15 ? v : null;
      });
      log(`JPEG: the lost frame cost nothing, no keyframe request: ${recovered.line}`);
      break;
    }
    if (gap.requests === before.requests) {
      log(`attempt ${attempt}: the packet after the dropped one was a keyframe, nothing to ask for; again`);
      continue;
    }
    if (gap.dropped <= before.dropped) {
      throw new Error(`Ada asked for a keyframe but decoded the P-frames after the gap: ${gap.line}`);
    }
    recovered = await until('H.264 decoding to resume at the requested keyframe', async () => {
      const v = await videoFrom(debugA, 'Ben');
      return v && v.keyframes > before.keyframes && v.decoded >= gap.decoded + 15 ? v : null;
    });
    log(`Ada decodes again: ${recovered.line}`);
    const forced = await until("Ben's window to count a keyframe forced on request", async () => {
      const s = await sending(debugB);
      return s && s.onRequest > sentBefore.onRequest ? s : null;
    });
    log(`Ben: ${forced.line} (was: ${sentAfter.line})`);
    const benErr = readFileSync(children.find((c) => c.name === 'ben').err, 'utf8');
    if (!benErr.includes('Ada asked for a keyframe')) {
      throw new Error("Ben's stderr does not show Ada's keyframe request");
    }
  }
  if (!recovered) {
    throw new Error('each of 3 drops was followed by a keyframe, so no keyframe request was ever tested');
  }

  // Mute: Ben's state reaches Ada as a control message.
  await click(debugB, 'Mute');
  await until("Ada's window to show Ben as muted", async () => {
    const row = await personRow(debugA, 'Ben');
    return row?.trim() === 'Ben · connected · muted';
  });
  log("Ada's window shows Ben · connected · muted");

  // Leave: Ben is back on the start screen and gone from the room at once.
  await click(debugB, 'Leave');
  await until("Ben's window to return to the start screen", async () => {
    const shown = await texts(debugB);
    return shown.some((t) => t.includes('New meeting')) && shown.some((t) => t.includes('You left the meeting'));
  });
  log("Ben's window is back on the start screen");
  const leftAt = Date.now();
  await until('the dev server to stop listing Ben', async () => {
    const { json } = await getJson(`${worker}/rooms/${room}/peers`);
    return json.peers.map((p) => p.name).join(',') === 'Ada';
  });
  const waited = (Date.now() - leftAt) / 1000;
  if (waited > 30) throw new Error(`Ben left the list after ${waited} s: the TTL, not the leave request`);
  log(`the dev server lists only Ada (${waited.toFixed(1)} s after the click)`);
  await until("Ada's window to stop listing Ben", async () => (await personRow(debugA, 'Ben')) === undefined);
  log("Ada's window no longer lists Ben");

  passed = true;
  log(
    `PASS: two AzMeet clients met through the meet Worker mock, heard and saw each other over iroh ` +
      `(${codec}), recovered from a lost video packet, and one left`,
  );
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
