import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  bytesToHex,
  dropKeyId,
  DROP_PREFIX,
  hexToBytes,
  importX25519Secret,
  newDropObjectKey,
  openDrop,
  plaintextOf,
  sealDrop,
  x25519PublicOf,
} from "../src/azd1.js";

const vector = JSON.parse(readFileSync(new URL("./azd1-vector.json", import.meta.url), "utf8"));
const encoder = new TextEncoder();

test("the AZD1 vector seals to its bytes and opens back", async () => {
  const dropSecret = await importX25519Secret(hexToBytes(vector.drop_secret));
  const dropPublic = await x25519PublicOf(dropSecret);
  assert.equal(bytesToHex(dropPublic), vector.drop_public);
  assert.equal(bytesToHex(await dropKeyId(dropPublic)), vector.drop_key_id);
  const raw = encoder.encode(vector.raw);
  assert.equal(bytesToHex(plaintextOf(vector.received, vector.folder, raw)), vector.plaintext);
  const sealed = await sealDrop({
    dropPublicKey: dropPublic,
    driveId: vector.drive_id,
    objectKey: vector.object_key,
    received: vector.received,
    folder: vector.folder,
    raw,
    ephemeralSecret: hexToBytes(vector.ephemeral_secret),
    nonce: hexToBytes(vector.nonce),
  });
  assert.equal(bytesToHex(sealed), vector.sealed);
  const opened = await openDrop({
    bytes: hexToBytes(vector.sealed),
    dropSecret,
    driveId: vector.drive_id,
    objectKey: vector.object_key,
  });
  assert.equal(opened.received, vector.received);
  assert.equal(opened.folder, vector.folder);
  assert.equal(new TextDecoder().decode(opened.raw), vector.raw);
});

test("a real drop has a fresh ephemeral key and nonce and opens only for its drive and name", async () => {
  const pair = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
  const dropPublicKey = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  const objectKey = newDropObjectKey();
  assert.match(objectKey, /^\.azlin\/drop\/[0-9a-f]{32}$/);
  assert.ok(objectKey.startsWith(DROP_PREFIX));
  const raw = encoder.encode("Subject: hi\r\n\r\nhello\r\n");
  const seal = () =>
    sealDrop({ dropPublicKey, driveId: "d_1", objectKey, received: 7, folder: "Spam", raw });
  const [a, b] = [await seal(), await seal()];
  assert.notEqual(bytesToHex(a.slice(21, 65)), bytesToHex(b.slice(21, 65)), "E and nonce are new");
  const opened = await openDrop({ bytes: a, dropSecret: pair.privateKey, driveId: "d_1", objectKey });
  assert.equal(opened.folder, "Spam");
  assert.deepEqual(opened.raw, raw);
  await assert.rejects(
    openDrop({ bytes: a, dropSecret: pair.privateKey, driveId: "d_2", objectKey }),
  );
  await assert.rejects(
    openDrop({ bytes: a, dropSecret: pair.privateKey, driveId: "d_1", objectKey: newDropObjectKey() }),
  );
  const changed = a.slice();
  changed[changed.length - 1] ^= 1;
  await assert.rejects(
    openDrop({ bytes: changed, dropSecret: pair.privateKey, driveId: "d_1", objectKey }),
  );
});

test("a header the format does not have is refused before anything is sealed", async () => {
  const raw = encoder.encode("x");
  assert.throws(() => plaintextOf(1, "Drafts", raw), /folder/);
  assert.throws(() => plaintextOf(1.5, "Inbox", raw), /seconds/);
  await assert.rejects(
    sealDrop({
      dropPublicKey: new Uint8Array(31),
      driveId: "d",
      objectKey: "k",
      received: 1,
      folder: "Inbox",
      raw,
    }),
    /32 bytes/,
  );
});
