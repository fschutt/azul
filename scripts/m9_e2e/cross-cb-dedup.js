// M10-D cross-cb dedup verification.
//
// Walks the manifest's boundaries list and confirms each canonical
// name appears at most once. Sharded mode's main win comes from
// every boundary shipping ONCE per page regardless of how many cbs
// reference it; a dup would defeat that.
//
// Also verifies that the manifest's transitive_boundaries form a
// closed set (no cb references a boundary canonical_addr that
// isn't itself in the manifest).
//
// Usage: `node cross-cb-dedup.js` (no args). Returns 0 on PASS.
const http = require('http');
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b));
        x.on('end',()=>r({ status: x.statusCode, body: Buffer.concat(c) }));
        x.on('error', j);
    }));
}

function failSync(msg) { console.error('FAIL: ' + msg); process.exit(1); }

(async () => {
    const resp = await fetch_('/az/manifest.json');
    if (resp.status !== 200) failSync('manifest fetch status=' + resp.status);
    const manifest = JSON.parse(resp.body.toString());

    if (!manifest.boundaries) failSync('manifest missing boundaries[]');

    // Dedup check: each canonical_addr should appear exactly once.
    const byAddr = new Map();
    const byName = new Map();
    for (const b of manifest.boundaries) {
        if (byAddr.has(b.canonical_addr)) {
            failSync('duplicate canonical_addr ' + b.canonical_addr +
                     ' (names: ' + byAddr.get(b.canonical_addr) + ' + ' + b.name + ')');
        }
        byAddr.set(b.canonical_addr, b.name);
        if (byName.has(b.name)) {
            failSync('duplicate name ' + b.name);
        }
        byName.set(b.name, b);
    }
    console.log('OK: ' + manifest.boundaries.length +
                ' boundaries, all canonical_addrs + names unique');

    // Closure check: every transitive_boundary addr must itself be
    // in the manifest.
    const knownAddrs = new Set(byAddr.keys());
    let missing = 0;
    for (const b of manifest.boundaries) {
        for (const t of (b.transitive_boundaries || [])) {
            if (!knownAddrs.has(t)) {
                console.warn('  WARN: ' + b.name + ' references missing addr ' + t);
                missing++;
            }
        }
    }
    if (missing > 0) {
        failSync(missing + ' transitive boundary refs not in manifest');
    }
    console.log('OK: transitive_boundaries closed under manifest');

    // URL uniqueness: each shard's URL should be unique (sharing a
    // URL would mean two shards happened to hash identically, which
    // is fine for true dups but suspicious).
    const urls = new Set();
    for (const b of manifest.boundaries) {
        if (urls.has(b.url)) {
            failSync('duplicate URL ' + b.url + ' across boundaries');
        }
        urls.add(b.url);
    }
    console.log('OK: all boundary URLs unique');

    console.log('\nPASS: cross-cb dedup invariants hold for ' +
                manifest.boundaries.length + ' boundary shards');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
