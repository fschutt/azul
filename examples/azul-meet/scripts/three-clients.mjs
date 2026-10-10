#!/usr/bin/env node
// Three AzMeet processes in one room above the mesh cap: the backbone forwards, each viewer gets the
// rendition its tile needs, and a keyframe request from a far viewer reaches the origin through the
// forwarder.
//
// All three run headless with the 440 Hz tone and the test pattern, relays off, and
// --mesh-cap 2, so a room of three routes over a backbone. Each pins the uplink it reports
// (--uplink-kbps), so the plan is known in advance:
//
//   Ada   1 Mbps    grid view
//   Ben  50 Mbps    grid view
//   Cleo 10 Mbps    speaker view with Ben on the stage (--layout speaker --stage Ben)
//
// The planner (IrohLoadBalancer with a mesh cap of 2, then the trees of routes.rs) picks
// max(ceil(sqrt 3), ceil(3 / 8)) = 2 forwarders by score (reported uplink times stability): Ben and
// Cleo (their 51 Mbps of usable uplink carry 1.5 times the room's ~17 Mbps of fan-out, so the
// backbone does not grow). The uplinks lie far apart, so a dip in someone's stability changes no
// rank. Ada is the one leaf and attaches to the best forwarder, Ben. So:
//
//   Ada's media:  Ada>Ben, Ben>Cleo      (a leaf uploads once)
//   Ben's media:  Ben>Ada, Ben>Cleo
//   Cleo's media: Cleo>Ben, Ben>Ada      (a leaf receives everything through its parent)
//
// Tiles (scale 1, the headless default): a grid tile is 200 px tall and asks for 360p, the stage
// (360 px) for 360p, a speaker-view thumbnail (90 px) for 90p. So Ada encodes two renditions of her
// camera, 90p for Cleo and 360p for Ben, uploads both to Ben once, and Ben passes only the 90p on
// to Cleo.
//
// Checks:
//   1. the dev server lists all three, each window shows the other two as connected;
//   2. every window's network panel shows the plan and the routes above (the same line
//      everywhere), and its role: Ada a leaf uploading once to Ben, Ben and Cleo backbone;
//      Ada's line about Cleo says she sends Cleo nothing directly;
//   3. audio: every window counts a second of packets from both others (Cleo hears Ada via Ben);
//   4. video: every window decodes 2 s from both others, at the renditions above and via the
//      forwarder where the routes say so; Ada sends 90p and 360p; Ben's panel says he passes
//      Ada's 90p to Cleo and counts forwarded packets;
//   5. a far keyframe request (H.264): Ada clicks "Drop a video packet"; Cleo's 90p stream of Ada
//      counts a gap and asks for a keyframe, Ben's stderr shows him passing Cleo's request on to
//      Ada, Ada's stderr shows Cleo's request arriving via Ben, and Cleo decodes again at a new
//      keyframe. With JPEG: a gap and no request. --require-h264 fails a JPEG run;
//   6. Cleo leaves: Ada and Ben are back in the full mesh and Ada still gets Ben's video.
//
// Usage (from the azul repository, after building AzMeet and libazul with the debug server):
//   node examples/azul-meet/scripts/three-clients.mjs
//     [--bin target/release/AzMeet] [--worker-dir ../azul-apps/cf-workers/meet]
//     [--port 8787] [--debug-a 8765] [--debug-b 8766] [--debug-c 8767] [--timeout 120]
//     [--require-h264] [--keep-logs]
import { parseArgs } from 'node:util';

import {
  appArgs,
  appEnv,
  audioFrom,
  click,
  codecLine,
  createRun,
  findPaths,
  forwarded,
  lineStarting,
  signedPeers,
  meetingOf,
  peerLine,
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
    'debug-c': { type: 'string', default: '8767' },
    timeout: { type: 'string', default: '120' },
    'keep-logs': { type: 'boolean', default: false },
    'require-h264': { type: 'boolean', default: false },
  },
});

const MESH_CAP = '2';
const people = [
  { name: 'Ada', port: Number(opts['debug-a']), args: ['--uplink-kbps', '1000'] },
  { name: 'Ben', port: Number(opts['debug-b']), args: ['--uplink-kbps', '50000'] },
  {
    name: 'Cleo',
    port: Number(opts['debug-c']),
    args: ['--uplink-kbps', '10000', '--layout', 'speaker', '--stage', 'Ben'],
  },
];
const [ada, ben, cleo] = people;

// What the planner says for these reports (see the header).
const PLAN = 'Network: 3 people, mesh cap 2: backbone Ben, Cleo; Ada uploads to Ben';
const ROUTES = 'Routes: Ada: Ada>Ben, Ben>Cleo | Ben: Ben>Ada, Ben>Cleo | Cleo: Ben>Ada, Cleo>Ben';
const ROLES = {
  Ada: 'You: leaf, uploading once to Ben',
  Ben: 'You: backbone, forwarding for others',
  Cleo: 'You: backbone, forwarding for others',
};
// Who sees whom at which rendition, and through whom (null: directly from the origin).
const VIDEO = [
  { viewer: 'Ada', origin: 'Ben', height: 360, via: null },
  { viewer: 'Ada', origin: 'Cleo', height: 360, via: 'Ben' },
  { viewer: 'Ben', origin: 'Ada', height: 360, via: null },
  { viewer: 'Ben', origin: 'Cleo', height: 360, via: null },
  { viewer: 'Cleo', origin: 'Ben', height: 360, via: null },
  { viewer: 'Cleo', origin: 'Ada', height: 90, via: 'Ben' },
];

const run = createRun({ name: 'three-clients', timeoutSecs: Number(opts.timeout) });
const { log, until, within } = run;
const byName = Object.fromEntries(people.map((p) => [p.name, p]));

let passed = false;
try {
  const { bin, workerDir } = findPaths(opts);
  log(`AzMeet: ${bin}`);
  log(`meet Worker: ${workerDir}`);
  log(`logs: ${run.logs}`);

  const worker = await startWorker(run, workerDir, Number(opts.port));
  const args = (p, extra) => appArgs(worker, p.name, ['--mesh-cap', MESH_CAP, ...p.args, ...extra]);

  ada.proc = run.start('ada', bin, args(ada, ['--autocreate']), appEnv(ada.port));
  const { link, room } = await meetingOf(run, ada.proc);
  log(`Ada created ${link}`);
  ben.proc = run.start('ben', bin, args(ben, ['--join', link]), appEnv(ben.port));
  cleo.proc = run.start('cleo', bin, args(cleo, ['--join', link]), appEnv(cleo.port));

  // 1. Everyone meets everyone.
  await until("the dev server to list Ada's, Ben's and Cleo's signed announcements", () =>
    signedPeers(worker, room, 3, ['Ada', 'Ben', 'Cleo']),
  );
  log('the dev server lists three signed announcements, none with a name');
  for (const p of people) {
    for (const other of people.filter((o) => o !== p)) {
      await until(`${p.name}'s window to show ${other.name} as connected`, () => showsConnected(p.port, other.name));
    }
  }
  log('every window shows the other two as connected');

  // 2. The plan: the same everywhere, and what the planner says.
  for (const p of people) {
    const lines = await until(`${p.name}'s network panel to show the planned routes`, async () => {
      const [plan, routes, role] = await Promise.all([
        lineStarting(p.port, 'Network: '),
        lineStarting(p.port, 'Routes: '),
        lineStarting(p.port, 'You: '),
      ]);
      return plan === PLAN && routes === ROUTES && role === ROLES[p.name] ? { plan, routes, role } : null;
    }).catch(async (e) => {
      const shown = await Promise.all(['Network: ', 'Routes: ', 'You: '].map((pre) => lineStarting(p.port, pre)));
      throw new Error(`${e.message}; ${p.name} shows: ${shown.join(' / ')}`);
    });
    log(`${p.name}: ${lines.plan} / ${lines.role}`);
  }
  log(`every window shows ${ROUTES}`);
  const adaToCleo = await until("Ada's network panel line about Cleo", () => peerLine(ada.port, 'Cleo'));
  if (adaToCleo.to.length !== 0) {
    throw new Error(`Ada is a leaf but sends Cleo something directly: ${adaToCleo.line}`);
  }
  log(`Ada, a leaf, sends Cleo nothing directly: ${adaToCleo.line}`);

  // 3. Audio from both others everywhere (through Ben where the routes say so).
  for (const p of people) {
    for (const other of people.filter((o) => o !== p)) {
      const heard = await until(`${p.name}'s window to count a second of audio from ${other.name}`, async () => {
        const a = await audioFrom(p.port, other.name);
        return a && a.packets >= 50 && a.played >= 25 ? a : null;
      });
      log(`${p.name}: ${heard.line}`);
    }
  }

  // 4. Video at the planned renditions, through the planned forwarder.
  const codecs = [];
  for (const p of people) {
    const line = await until(`${p.name}'s window to show its video codec`, () => codecLine(p.port));
    codecs.push(line);
  }
  const h264 = codecs.every((line) => line.startsWith('Video: H.264'));
  if (!h264 && opts['require-h264']) {
    throw new Error(`--require-h264, but the video is not H.264 everywhere: ${codecs.join(' / ')}`);
  }
  const codec = h264 ? 'H.264' : 'JPEG';
  for (const want of VIDEO) {
    const port = byName[want.viewer].port;
    const seen = await until(
      `${want.viewer}'s window to decode 2 s of ${want.origin}'s ${want.height}p ${codec} video` +
        (want.via ? ` via ${want.via}` : ''),
      async () => {
        const v = await videoFrom(port, want.origin, 'camera', want.height);
        return v && v.codec === codec && v.via === want.via && v.decoded >= 30 && v.keyframes >= 1 ? v : null;
      },
    );
    log(`${want.viewer}: ${seen.line}`);
  }
  for (const height of [90, 360]) {
    const s = await until(`Ada's window to send her ${height}p rendition`, async () => {
      const line = await sending(ada.port, 'camera', height);
      return line && line.h264 + line.jpeg > 0 ? line : null;
    });
    log(`Ada: ${s.line}`);
  }
  const benToCleo = await until("Ben's panel to say he passes Ada's 90p on to Cleo", async () => {
    const line = await peerLine(ben.port, 'Cleo');
    return line?.to.includes(`Ada camera 90p ${codec}`) ? line : null;
  });
  log(`Ben: ${benToCleo.line}`);
  const relayed = await until("Ben's panel to count forwarded media", async () => {
    const f = await forwarded(ben.port);
    return f && f.packets + f.frames > 0 ? f : null;
  });
  log(`Ben: ${relayed.line}`);
  for (const p of people) {
    if (!stderrOf(p.proc).includes('no camera or screen is opened')) {
      throw new Error(`${p.name} did not say it runs without a camera or screen (see its stderr)`);
    }
  }

  // 5. A keyframe request from the far viewer reaches the origin through the forwarder.
  let recovered = null;
  for (let attempt = 1; attempt <= 3 && !recovered; attempt++) {
    const before = await until("Cleo's line about Ada's 90p before the drop", () =>
      videoFrom(cleo.port, 'Ada', 'camera', 90),
    );
    const sentBefore = await until("Ada's 90p sending line before the drop", () => sending(ada.port, 'camera', 90));
    await click(run, ada.port, 'Drop a video packet');
    const gap = await within(15000, async () => {
      const v = await videoFrom(cleo.port, 'Ada', 'camera', 90);
      return v && v.gaps > before.gaps ? v : null;
    });
    if (!gap) throw new Error("Cleo's window never counted Ada's dropped packet as a gap");
    log(`Cleo after the drop: ${gap.line}`);
    await until("Ada's window to count the dropped packet", async () => {
      const s = await sending(ada.port, 'camera', 90);
      return s && s.dropped > sentBefore.dropped ? s : null;
    });
    if (!h264) {
      if (gap.requests !== before.requests) throw new Error('a lost JPEG frame asked for a keyframe');
      recovered = await until('JPEG decoding to go on after the lost frame', async () => {
        const v = await videoFrom(cleo.port, 'Ada', 'camera', 90);
        return v && v.decoded >= gap.decoded + 15 ? v : null;
      });
      log(`JPEG: the lost frame cost nothing, no keyframe request: ${recovered.line}`);
      break;
    }
    if (gap.requests === before.requests) {
      log(`attempt ${attempt}: the packet after the dropped one was a keyframe, nothing to ask for; again`);
      continue;
    }
    const passedOn = await until("Ben's stderr to show him passing Cleo's request on to Ada", () =>
      stderrOf(ben.proc).includes("passing Cleo's keyframe request for Ada's camera 90p on to Ada"),
    );
    const arrived = await until("Ada's stderr to show Cleo's request arriving via Ben", () =>
      stderrOf(ada.proc).includes('Cleo asked for a keyframe (camera 90p, via Ben)'),
    );
    log(`Ben passed Cleo's keyframe request on (${passedOn}); Ada got it via Ben (${arrived})`);
    recovered = await until("Cleo's 90p of Ada to decode again at a new keyframe", async () => {
      const v = await videoFrom(cleo.port, 'Ada', 'camera', 90);
      return v && v.keyframes > before.keyframes && v.decoded >= gap.decoded + 15 ? v : null;
    });
    log(`Cleo decodes again: ${recovered.line}`);
    const forced = await until("Ada's window to count a keyframe forced on request", async () => {
      const s = await sending(ada.port, 'camera', 90);
      return s && s.onRequest > sentBefore.onRequest ? s : null;
    });
    log(`Ada: ${forced.line}`);
  }
  if (!recovered) {
    throw new Error('each of 3 drops was followed by a keyframe, so no keyframe request was ever tested');
  }

  // 6. Cleo leaves: two people are within the mesh cap again.
  await click(run, cleo.port, 'Leave');
  await until("Cleo's window to return to the start screen", async () =>
    (await texts(cleo.port)).some((t) => t.includes('You left the meeting')),
  );
  for (const p of [ada, ben]) {
    const plan = await until(`${p.name}'s network panel to show the full mesh after Cleo left`, async () => {
      const line = await lineStarting(p.port, 'Network: ');
      return line === 'Network: 2 people, full mesh (mesh cap 2)' ? line : null;
    });
    log(`${p.name}: ${plan}`);
  }
  const benBefore = await until("Ada's line about Ben's video", () => videoFrom(ada.port, 'Ben', 'camera', 360));
  const benAfter = await until("Ada's window to keep decoding Ben's video in the mesh", async () => {
    const v = await videoFrom(ada.port, 'Ben', 'camera', 360);
    return v && v.via === null && v.decoded >= benBefore.decoded + 15 ? v : null;
  });
  log(`Ada: ${benAfter.line}`);

  passed = true;
  log(
    `PASS: three AzMeet clients routed over a backbone (${ROUTES.slice('Routes: '.length)}), got ` +
      `their renditions (${codec}) through the forwarder, a far keyframe request reached its origin, ` +
      'and two went back to the full mesh',
  );
} catch (e) {
  log(`FAIL: ${e.message}`);
  run.dumpLogs();
} finally {
  run.finish(passed, opts['keep-logs']);
}
