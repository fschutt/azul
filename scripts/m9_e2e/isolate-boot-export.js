// Which bootstrap export traps? Instantiate azul-mini standalone and call the
// boot sequence one export at a time.
//
// The browser console only shows that bootstrap failed somewhere between
// AzStartup_init and the first call that logs on success, because
// registerCbNodeKind / setLayoutCbTableIdx / setRefAny log nothing. Narrowing it
// in the loader would need a rebuild and a ~40 minute relift; instantiating the
// saved mini here answers it in seconds.
//
// Usage: node isolate-boot-export.js <azul-mini.wasm>
const fs = require('fs');

const path = process.argv[2];
if (!path) {
  console.error('usage: isolate-boot-export.js <azul-mini.wasm>');
  process.exit(2);
}

let memory = null;
const dv = () => new DataView(memory.buffer);

const env = {
  __remill_read_memory_8: (m, a) => dv().getUint8(Number(a)),
  __remill_read_memory_16: (m, a) => dv().getUint16(Number(a), true),
  __remill_read_memory_32: (m, a) => dv().getUint32(Number(a), true),
  __remill_read_memory_64: (m, a) => dv().getBigUint64(Number(a), true),
  __remill_write_memory_8: (m, a, v) => { dv().setUint8(Number(a), Number(v) & 0xff); return m; },
  __remill_write_memory_16: (m, a, v) => { dv().setUint16(Number(a), Number(v) & 0xffff, true); return m; },
  __remill_write_memory_32: (m, a, v) => { dv().setUint32(Number(a), Number(v) >>> 0, true); return m; },
  __remill_write_memory_64: (m, a, v) => {
    dv().setBigUint64(Number(a), BigInt.asUintN(64, BigInt(v)), true); return m;
  },
  __remill_atomic_begin: m => m,
  __remill_atomic_end: m => m,
  memset: (d, c, n) => {
    new Uint8Array(memory.buffer).fill(Number(c) & 0xff, Number(d), Number(d) + Number(n));
    return d;
  },
  memcpy: (d, s, n) => {
    new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n));
    return d;
  },
  memmove: (d, s, n) => {
    new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n));
    return d;
  },
  // 128-bit helpers: zero stubs make every float/u128 path trap as a lab
  // artefact rather than a real finding, so provide something total.
  __multi3: () => 0n,
  __udivti3: () => 0n,
};

// Anything else the module imports resolves to a counted no-op rather than
// failing instantiation - a missing import would abort before any export runs
// and tell us nothing about which one traps.
const missing = new Map();
const handler = {
  get(_t, name) {
    if (name in env) return env[name];
    if (name === '__indirect_function_table' || name === 'memory') return env[name];
    missing.set(name, (missing.get(name) || 0) + 1);
    return () => 0;
  },
  has: () => true,
};

async function main() {
  const bytes = fs.readFileSync(path);
  env.__indirect_function_table = new WebAssembly.Table({ initial: 1024, element: 'anyfunc' });
  const { instance } = await WebAssembly.instantiate(bytes, { env: new Proxy({}, handler) });
  const ex = instance.exports;
  memory = ex.memory || env.memory;
  if (!memory) {
    console.error('module exports no memory');
    process.exit(3);
  }
  console.log('instantiated. exports: ' + Object.keys(ex).length);

  const call = (name, ...args) => {
    if (typeof ex[name] !== 'function') return { name, status: 'absent' };
    try {
      const r = ex[name](...args);
      return { name, status: 'ok', result: String(r) };
    } catch (e) {
      return { name, status: 'TRAP', err: (e && e.message) || String(e) };
    }
  };

  // The bootstrap order the loader uses.
  const seq = [];
  seq.push(call('AzStartup_resetBumpHeap', 160 * 1024 * 1024));
  const init = call('AzStartup_init', 0, 0);
  seq.push(init);
  const state = init.status === 'ok' ? Number(init.result) : 0;
  console.log('state = ' + state);
  // the three unlogged calls, which is where the browser trace goes dark
  seq.push(call('AzStartup_registerCbNodeKind', state, 8, 0));
  seq.push(call('AzStartup_setLayoutCbTableIdx', state, 432));
  seq.push(call('AzStartup_setRefAny', state, 0));
  seq.push(call('AzStartup_initLayoutCache', state, 1280, 800, 0));

  console.log('');
  for (const r of seq) {
    if (r.status === 'ok') console.log('  ok    ' + r.name + ' -> ' + r.result);
    else if (r.status === 'absent') console.log('  --    ' + r.name + ' (not exported)');
    else console.log('  TRAP  ' + r.name + ' : ' + r.err);
  }
  if (missing.size) {
    console.log('');
    console.log('imports stubbed (would be real in the browser): ' +
                [...missing.keys()].slice(0, 12).join(', '));
  }
}

main().catch(e => {
  console.error(e && e.stack ? e.stack : String(e));
  process.exit(1);
});
