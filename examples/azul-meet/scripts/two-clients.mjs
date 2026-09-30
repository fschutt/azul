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
//      <other> (camera <height>p): <codec>, decoded N, keyframes K, ..." line must count at least
//      30 decoded frames (2 s at 15 fps) and a keyframe. Two people are within the mesh cap, so
//      each sends to the other directly (the network panel says "full mesh");
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
import { parseArgs } from 'node:util';

import {
  appEnv,
  audioFrom,
  click,
  codecLine,
  createRun,
  debugOp,
  findPaths,
  getJson,
  lineStarting,
  listedNames,
  meetingOf,
  personRow,
  sending,
  showsConnected,
  startWorker,
  stderrOf,
  texts,
  videoFrom,
} from './meet-e2e.mjs';

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

const debugA = Number(opts['debug-a']);
const debugB = Number(opts['debug-b']);
const run = createRun({ name: 'two-clients', timeoutSecs: Number(opts.timeout) });
const { log, until, within } = run;

let passed = false;
try {
  const { bin, workerDir } = findPaths(opts);
  log(`AzMeet: ${bin}`);
  log(`meet Worker: ${workerDir}`);
  log(`logs: ${run.logs}`);

  const worker = await startWorker(run, workerDir, Number(opts.port));

  const ada = run.start('ada', bin, [], appEnv(worker, 'Ada', debugA, { AZMEET_AUTOCREATE: '1' }));
  const { link, room } = await meetingOf(run, ada);
  log(`Ada created ${link}`);

  const ben = run.start('ben', bin, [], appEnv(worker, 'Ben', debugB, { AZMEET_JOIN: link }));

  await until('the dev server to list Ada and Ben in the room', async () => {
    const names = await listedNames(worker, room);
    return names.includes('Ada') && names.includes('Ben');
  });
  log('the dev server lists Ada and Ben');

  await until("Ada's window to show Ben as connected", () => showsConnected(debugA, 'Ben'));
  log("Ada's window shows Ben as connected");
  await until("Ben's window to show Ada as connected", () => showsConnected(debugB, 'Ada'));
  log("Ben's window shows Ada as connected");

  const state = await debugOp(debugA, 'get_state');
  log(`Ada's debug server answers get_state: ${state.status ?? JSON.stringify(state).slice(0, 80)}`);

  // Two people are within the mesh cap: each sends to the other directly.
  for (const [name, port] of [['Ada', debugA], ['Ben', debugB]]) {
    const plan = await until(`${name}'s network panel to show the full mesh`, async () => {
      const line = await lineStarting(port, 'Network: ');
      return line?.includes('full mesh') ? line : null;
    });
    log(`${name}: ${plan}`);
  }

  // Audio: a second of packets in each jitter buffer, half a second played (to nothing: headless).
  for (const [listener, port, speaker] of [['Ada', debugA, 'Ben'], ['Ben', debugB, 'Ada']]) {
    const heard = await until(`${listener}'s window to count a second of audio from ${speaker}`, async () => {
      const a = await audioFrom(port, speaker);
      return a && a.packets >= 50 && a.played >= 25 ? a : null;
    });
    log(`${listener}: ${heard.line}`);
  }
  for (const entry of [ada, ben]) {
    if (!stderrOf(entry).includes('no audio device is opened')) {
      throw new Error(`${entry.name} did not say it runs without audio devices (see its stderr)`);
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
    if (seen.via !== null) throw new Error(`in the full mesh ${listener} gets ${sender}'s video via ${seen.via}`);
    log(`${listener}: ${seen.line}`);
  }
  for (const entry of [ada, ben]) {
    if (!stderrOf(entry).includes('no camera or screen is opened')) {
      throw new Error(`${entry.name} did not say it runs without a camera or screen (see its stderr)`);
    }
  }
  log('both apps send the test pattern: no camera or screen is opened');

  // Loss: Ben drops one video packet before it leaves.
  let recovered = null;
  for (let attempt = 1; attempt <= 3 && !recovered; attempt++) {
    const before = await until("Ada's video line before the drop", () => videoFrom(debugA, 'Ben'));
    const sentBefore = await until("Ben's sending line before the drop", () => sending(debugB));
    await click(run, debugB, 'Drop a video packet');
    const gap = await within(15000, async () => {
      const v = await videoFrom(debugA, 'Ben', 'camera', before.height);
      return v && v.gaps > before.gaps ? v : null;
    });
    if (!gap) throw new Error("Ada's window never counted Ben's dropped packet as a gap");
    log(`Ada after the drop: ${gap.line}`);
    const sentAfter = await until("Ben's window to count the dropped packet", async () => {
      const s = await sending(debugB, 'camera', sentBefore.height);
      return s && s.dropped > sentBefore.dropped ? s : null;
    });
    if (!h264) {
      if (gap.requests !== before.requests) throw new Error('a lost JPEG frame asked for a keyframe');
      recovered = await until('JPEG decoding to go on after the lost frame', async () => {
        const v = await videoFrom(debugA, 'Ben', 'camera', before.height);
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
      const v = await videoFrom(debugA, 'Ben', 'camera', before.height);
      return v && v.keyframes > before.keyframes && v.decoded >= gap.decoded + 15 ? v : null;
    });
    log(`Ada decodes again: ${recovered.line}`);
    const forced = await until("Ben's window to count a keyframe forced on request", async () => {
      const s = await sending(debugB, 'camera', sentBefore.height);
      return s && s.onRequest > sentBefore.onRequest ? s : null;
    });
    log(`Ben: ${forced.line} (was: ${sentAfter.line})`);
    if (!stderrOf(ben).includes('Ada asked for a keyframe')) {
      throw new Error("Ben's stderr does not show Ada's keyframe request");
    }
  }
  if (!recovered) {
    throw new Error('each of 3 drops was followed by a keyframe, so no keyframe request was ever tested');
  }

  // Mute: Ben's state reaches Ada as a control message.
  await click(run, debugB, 'Mute');
  await until("Ada's window to show Ben as muted", async () => {
    const row = await personRow(debugA, 'Ben');
    return row?.trim() === 'Ben · connected · muted';
  });
  log("Ada's window shows Ben · connected · muted");

  // Leave: Ben is back on the start screen and gone from the room at once.
  await click(run, debugB, 'Leave');
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
  run.dumpLogs();
} finally {
  run.finish(passed, opts['keep-logs']);
}
