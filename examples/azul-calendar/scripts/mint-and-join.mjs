#!/usr/bin/env node
// AzCalendar mints an AzMeet link for a new event, and AzMeet joins the meeting with it.
//
//   1. starts the meet dev server (azul-apps cf-workers/meet/dev-server.mjs, in memory);
//   2. starts AzCalendar headless with an empty temporary data folder (AZCAL_DATA) and the dev
//      server as its meeting server (AZMEET_WORKER);
//   3. through AzCalendar's debug server: clicks "New event", focuses the title field
//      (#event-title) and types a title, clicks "Add AzMeet link", clicks "Save event";
//   4. asserts the event is ONE file, <data>/events/<uuid>.json, in format "azcalendar.event"
//      version 1, with the title and an azlin://meet/<room id> link minted by the dev server,
//      that the dev server knows that room, and that the week view shows the event with a
//      "Join meeting" button;
//   5. starts AzMeet "Ben" headless with AZMEET_JOIN=<that link> and waits until the dev server
//      lists Ben in the room;
//   6. clicks "Join meeting" in AzCalendar: AzCalendar starts the AzMeet next to it (or
//      AZMEET_BIN) with AZMEET_JOIN and AZMEET_WORKER, as "Cal" (AZMEET_NAME, inherited), and
//      the dev server lists Cal in the room too.
//
// Usage (from the azul repository, after building AzCalendar, AzMeet and libazul with the debug
// server, AZ_DEBUG / e2e-server):
//   node examples/azul-calendar/scripts/mint-and-join.mjs
//     [--bin target/release/AzCalendar] [--meet-bin target/release/AzMeet]
//     [--worker-dir ../azul-apps-m1/cf-workers/meet] [--port 8797]
//     [--debug-cal 8767] [--debug-meet 8768] [--timeout 90] [--keep-logs]
//
// Also read from the environment: AZCAL_BIN, AZMEET_BIN, AZMEET_WORKER_DIR. Logs and the data
// folder go to a temporary directory that is printed at the end (kept on failure, or always with
// --keep-logs).
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, openSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const { values: opts } = parseArgs({
  options: {
    bin: { type: 'string' },
    'meet-bin': { type: 'string' },
    'worker-dir': { type: 'string' },
    port: { type: 'string', default: '8797' },
    'debug-cal': { type: 'string', default: '8767' },
    'debug-meet': { type: 'string', default: '8768' },
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

/** Where cargo puts a binary: target/<profile>, or target/consumer/<profile> when built from the
 *  example's own folder (its .cargo/config.toml). */
function builtBinaries(name) {
  const exe = process.platform === 'win32' ? `${name}.exe` : name;
  const out = [];
  for (const root of [repo, mainRepo]) {
    for (const dir of [['release'], ['debug'], ['consumer', 'release'], ['consumer', 'debug']]) {
      out.push(join(root, 'target', ...dir, exe));
    }
  }
  return out;
}

const calBin = firstExisting('the AzCalendar binary (pass --bin)', [
  opts.bin,
  process.env.AZCAL_BIN,
  ...builtBinaries('AzCalendar'),
]);
const meetBin = firstExisting('the AzMeet binary (pass --meet-bin)', [
  opts['meet-bin'],
  process.env.AZMEET_BIN,
  join(dirname(calBin), process.platform === 'win32' ? 'AzMeet.exe' : 'AzMeet'),
  ...builtBinaries('AzMeet'),
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
const debugCal = Number(opts['debug-cal']);
const debugMeet = Number(opts['debug-meet']);
const deadline = Date.now() + Number(opts.timeout) * 1000;
const logs = mkdtempSync(join(tmpdir(), 'azcalendar-mint-and-join-'));
const data = join(logs, 'data');
mkdirSync(data);
const children = [];
/** AzMeet processes AzCalendar started ("Join meeting"): not our children, stopped by pid. */
const launched = [];

const TITLE = 'Team sync';
const ROOM_LINK = /^azlin:\/\/meet\/([0-9a-z]{26})$/;
const EVENT_FILE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.json$/;
/** How AzCalendar's form says an event was not saved (lib.rs: form_error, mint_failure, ...). */
const FORM_ERRORS = [
  'Give the event a title',
  'The event must end after',
  'This event cannot be saved',
  'The meeting server',
  'Too many new meetings',
  'Could not write',
  'No meeting server',
];

function log(line) {
  console.log(`[mint-and-join] ${line}`);
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
  for (const pid of launched) {
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      // already gone
    }
  }
  for (const { child } of children) {
    child.stopping = true;
    if (!child.exited) child.kill('SIGTERM');
  }
  setTimeout(() => {
    for (const { child } of children) if (!child.exited) child.kill('SIGKILL');
    for (const pid of launched) {
      try {
        process.kill(pid, 'SIGKILL');
      } catch {
        // already gone
      }
    }
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

async function shows(debugPort, text) {
  return (await texts(debugPort)).some((t) => t.includes(text));
}

/** An op that must succeed; the debug server answers `status: "error"` otherwise. */
async function mustOp(debugPort, op) {
  const answer = await debugOp(debugPort, op);
  const shown = JSON.stringify(answer).slice(0, 160);
  if (answer?.status === 'error') throw new Error(`${JSON.stringify(op)} failed: ${shown}`);
  const target = op.text ?? op.selector;
  log(`${op.op ?? op}${target ? ` "${target}"` : ''} on :${debugPort}: ${shown}`);
  return answer;
}

/** Clicks the first node whose text contains `text` (the debug server's click op). */
async function click(debugPort, text) {
  return mustOp(debugPort, { op: 'click', text });
}

/** The `<uuid>.json` files in the data folder's events folder. */
function eventFiles() {
  try {
    return readdirSync(join(data, 'events')).filter((name) => EVENT_FILE.test(name));
  } catch {
    return [];
  }
}

/** The first `<KEY> <value>` line an app printed on stdout. */
function printed(file, key) {
  const m = readFileSync(file, 'utf8').match(new RegExp(`^${key} (\\S+)$`, 'm'));
  return m?.[1];
}

async function peerNames(room) {
  const { status, json } = await getJson(`${worker}/rooms/${room}/peers`);
  return status === 200 ? json.peers.map((p) => p.name) : [];
}

let passed = false;
try {
  log(`AzCalendar: ${calBin}`);
  log(`AzMeet: ${meetBin}`);
  log(`meet Worker: ${workerDir}`);
  log(`logs and data: ${logs}`);

  start('worker', process.execPath, [join(workerDir, 'dev-server.mjs'), '--memory', '--port', String(port)], {});
  await until('the dev server', async () => (await getJson(`${worker}/health`)).json.ok === true);
  log(`dev server up on ${worker}`);

  // AZMEET_NAME / AZMEET_RELAY are for the AzMeet that "Join meeting" starts (it inherits them).
  const cal = start('azcalendar', calBin, [], {
    AZ_BACKEND: 'headless',
    AZ_DEBUG: String(debugCal),
    AZCAL_DATA: data,
    AZMEET_WORKER: worker,
    AZMEET_BIN: meetBin,
    AZMEET_NAME: 'Cal',
    AZMEET_RELAY: 'off',
  });
  await until("AzCalendar's week view", () => shows(debugCal, 'New event'));
  if (eventFiles().length !== 0) throw new Error('the data folder is not empty at the start');

  // New event: title, AzMeet link, save.
  await click(debugCal, 'New event');
  await until('the new-event form', () => shows(debugCal, 'Add AzMeet link'));
  await mustOp(debugCal, { op: 'focus_node', selector: '#event-title' });
  await sleep(200);
  await mustOp(debugCal, { op: 'text_input', text: TITLE });
  await sleep(300);
  await click(debugCal, 'Add AzMeet link');
  await until('the form to say a link will be minted', () => shows(debugCal, 'A new AzMeet link is made'));
  await click(debugCal, 'Save event');

  const link = await until('AzCalendar to save the event (AZCAL_LINK on stdout)', async () => {
    const minted = printed(cal.out, 'AZCAL_LINK');
    if (minted) return minted;
    // Asking the window also wakes the app's loop; a form error says why nothing was saved.
    const problem = (await texts(debugCal)).find((t) => FORM_ERRORS.some((e) => t.startsWith(e)));
    if (problem) throw new Error(`the form says: ${problem}`);
    return null;
  });
  const saved = printed(cal.out, 'AZCAL_SAVED');
  log(`AzCalendar saved ${saved} with ${link}`);

  // The event is one file, in the documented format, holding the minted link.
  const files = eventFiles();
  if (files.length !== 1) throw new Error(`expected one event file, found ${JSON.stringify(files)}`);
  const path = join(data, 'events', files[0]);
  if (saved && resolve(saved) !== resolve(path)) throw new Error(`AZCAL_SAVED ${saved} is not ${path}`);
  const event = JSON.parse(readFileSync(path, 'utf8'));
  const expect = (ok, what) => {
    if (!ok) throw new Error(`${what}; the file holds ${JSON.stringify(event)}`);
  };
  expect(event.format === 'azcalendar.event' && event.version === 1, 'not an azcalendar.event version 1');
  expect(`${event.id}.json` === files[0], 'the file is not named by the event id');
  expect(event.title === TITLE, `the title is not "${TITLE}"`);
  expect(/^\d{4}-\d{2}-\d{2}$/.test(event.date), 'no date');
  expect(event.start === '09:00' && event.end === '10:00', 'not the default 09:00 - 10:00');
  expect(event.meeting?.link === link, 'the file does not hold the printed link');
  expect(event.meeting?.server === worker, 'the file does not name the meeting server');
  const room = link.match(ROOM_LINK)?.[1];
  expect(room, `the link ${link} is not azlin://meet/<room id>`);
  log(`${files[0]}: "${event.title}" on ${event.date} ${event.start} - ${event.end}, ${event.meeting.link}`);

  // The meeting server made that room.
  const known = await getJson(`${worker}/rooms/${room}?format=json`);
  if (known.status !== 200 || known.json.room !== room) {
    throw new Error(`the dev server does not know room ${room}: ${known.status} ${JSON.stringify(known.json)}`);
  }
  log(`the dev server knows room ${room} (code ${known.json.code})`);

  // The week view shows the event with its Join button.
  await until('the week view to show the event', () => shows(debugCal, TITLE));
  await until('the event to offer "Join meeting"', () => shows(debugCal, 'Join meeting'));
  log('the week view shows the event with "Join meeting"');

  // A second AzMeet joins with the shared link.
  start('ben', meetBin, [], {
    AZ_BACKEND: 'headless',
    AZ_DEBUG: String(debugMeet),
    AZMEET_WORKER: worker,
    AZMEET_NAME: 'Ben',
    AZMEET_RELAY: 'off',
    AZMEET_TEST_TONE: '1',
    AZMEET_JOIN: link,
  });
  await until('the dev server to list Ben in the room', async () => (await peerNames(room)).includes('Ben'));
  log('the dev server lists Ben in the room: AzMeet joined with the minted link');

  // "Join meeting" in AzCalendar starts AzMeet with the link.
  await click(debugCal, 'Join meeting');
  const pid = await until('AzCalendar to start AzMeet (AZCAL_JOIN_PID on stdout)', async () =>
    printed(cal.out, 'AZCAL_JOIN_PID'),
  );
  launched.push(Number(pid));
  log(`AzCalendar started AzMeet (pid ${pid})`);
  await until('the dev server to list Cal in the room', async () => (await peerNames(room)).includes('Cal'));
  log(`the dev server lists ${JSON.stringify(await peerNames(room))} in the room`);

  passed = true;
  log('PASS: AzCalendar minted an AzMeet link into its event file, and AzMeet joined the meeting with it');
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
