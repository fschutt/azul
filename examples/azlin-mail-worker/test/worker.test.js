import assert from "node:assert/strict";
import { test } from "node:test";

import { bytesToHex, openDrop } from "../src/azd1.js";
import { amzDate, objectName } from "../src/mailname.js";
import worker, { deliver, dropPublicKey, folderOf, MAX_MESSAGE_BYTES } from "../src/worker.js";

const encoder = new TextEncoder();
const NOW = Date.UTC(2026, 9, 10, 8, 0, 0);
const RAW = encoder.encode(
  "From: Alice Example <alice@example.com>\r\nTo: bob@example.org\r\nSubject: hi\r\n\r\nhello\r\n",
);
const S3 = {
  S3_ENDPOINT: "https://s3.example.test",
  S3_BUCKET: "drive-bucket",
  S3_ACCESS_KEY_ID: "AKIDTEST",
  S3_SECRET_ACCESS_KEY: "secret",
};
// A bucket that is not an Azlin drive, declared plain in so many words.
const PLAIN = { ...S3, AZLIN_PLAIN_DRIVE: "yes" };

function recorder() {
  const puts = [];
  const fetchImpl = async (url, init) => {
    puts.push({ url, body: init.body, contentType: init.headers["content-type"] });
    return new Response(null, { status: 200 });
  };
  return { puts, fetchImpl };
}

test("the stable name is AzMail's: the arrival stamp and 16 hex digits of the SHA-256", async () => {
  assert.equal(amzDate(1791619200), "20261010T080000Z");
  const hash = bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", RAW)));
  assert.equal(await objectName(RAW, 1791619200), `20261010T080000Z-${hash.slice(0, 16)}.eml`);
});

test("a plain drive gets the message's exact bytes under its stable name", async () => {
  const { puts, fetchImpl } = recorder();
  const result = await deliver({ raw: RAW, headers: new Headers(), env: PLAIN, now: NOW, fetchImpl });
  assert.equal(result.encrypted, false);
  assert.equal(result.folder, "Inbox");
  assert.equal(result.key, `mail/Inbox/${await objectName(RAW, NOW / 1000)}`);
  assert.equal(puts.length, 1);
  assert.equal(puts[0].url, `https://s3.example.test/drive-bucket/${result.key}`);
  assert.deepEqual(puts[0].body, RAW);
  assert.equal(puts[0].contentType, "message/rfc822");
  // A retry of the same message lands on the same name: no duplicate.
  const again = await deliver({ raw: RAW, headers: new Headers(), env: PLAIN, now: NOW, fetchImpl });
  assert.equal(again.key, result.key);
});

test("an upstream spam verdict files the message into Spam", async () => {
  assert.equal(folderOf(new Headers({ "X-Spam-Flag": "YES" })), "Spam");
  assert.equal(folderOf(new Headers({ "X-Spam-Status": "Yes, score=9.1" })), "Spam");
  assert.equal(folderOf(new Headers({ "X-Spam-Status": "No, score=0.2" })), "Inbox");
  const { puts, fetchImpl } = recorder();
  const result = await deliver({
    raw: RAW,
    headers: new Headers({ "X-Spam-Flag": "YES" }),
    env: PLAIN,
    now: NOW,
    fetchImpl,
  });
  assert.ok(result.key.startsWith("mail/Spam/"));
  assert.equal(puts.length, 1);
});

test("an encrypted drive gets only a sealed drop under a random name", async () => {
  const pair = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
  const publicHex = bytesToHex(new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey)));
  const env = { ...S3, AZLIN_DROP_PUBLIC_KEY: publicHex, AZLIN_DRIVE_ID: "d_1" };
  const { puts, fetchImpl } = recorder();
  const result = await deliver({ raw: RAW, headers: new Headers(), env, now: NOW, fetchImpl });
  assert.equal(result.encrypted, true);
  assert.match(result.key, /^\.azlin\/drop\/[0-9a-f]{32}$/);
  const body = puts[0].body;
  assert.equal(puts[0].contentType, "application/octet-stream");
  const text = new TextDecoder("latin1").decode(body);
  for (const secret of ["alice@example.com", "bob@example.org", "hello", "Subject"]) {
    assert.ok(!text.includes(secret), `${secret} is not in the bucket`);
  }
  const opened = await openDrop({
    bytes: body,
    dropSecret: pair.privateKey,
    driveId: "d_1",
    objectKey: result.key,
  });
  assert.deepEqual(opened.raw, RAW);
  assert.equal(opened.received, NOW / 1000);
  assert.equal(opened.folder, "Inbox");
});

test("a damaged drop key or a missing drive id stops the delivery before anything is sent", async () => {
  const { puts, fetchImpl } = recorder();
  await assert.rejects(
    deliver({
      raw: RAW,
      headers: new Headers(),
      env: { ...S3, AZLIN_DROP_PUBLIC_KEY: "abcd", AZLIN_DRIVE_ID: "d_1" },
      fetchImpl,
    }),
    /64 hex digits/,
  );
  await assert.rejects(
    deliver({
      raw: RAW,
      headers: new Headers(),
      env: { ...S3, AZLIN_DROP_PUBLIC_KEY: "11".repeat(32) },
      fetchImpl,
    }),
    /AZLIN_DRIVE_ID/,
  );
  assert.equal(puts.length, 0);
});

test("the email handler rejects what is too big and files the rest", async () => {
  let rejected = null;
  const big = {
    rawSize: MAX_MESSAGE_BYTES + 1,
    setReject: (why) => {
      rejected = why;
    },
  };
  await worker.email(big, S3);
  assert.equal(rejected, "Message too large");

  const originalFetch = globalThis.fetch;
  const puts = [];
  globalThis.fetch = async (url) => {
    puts.push(url);
    return new Response(null, { status: 200 });
  };
  try {
    const message = {
      rawSize: RAW.length,
      raw: new Blob([RAW]).stream(),
      headers: new Headers(),
      setReject: () => assert.fail("not rejected"),
    };
    await worker.email(message, PLAIN);
  } finally {
    globalThis.fetch = originalFetch;
  }
  assert.equal(puts.length, 1);
  assert.match(puts[0], /\/drive-bucket\/mail\/Inbox\/\d{8}T\d{6}Z-[0-9a-f]{16}\.eml$/);
});

// X-DELTA-AZDRIVE-KEYS-17: the Worker fails CLOSED. Every Azlin drive is encrypted; a Worker whose
// drop key was not set yet (the routing rules came first), or whose secret was removed, wrote the
// mail of the domain as plain .eml objects into the bucket - readable by the node's operator and
// invisible to the owner's index. Without the key and without the word "yes" nothing is written
// and the delivery fails, so the sending server tries again.
test("x_delta_azdrive_keys_17 a Worker without a drop key files nothing unless the drive is declared plain", async () => {
  for (const env of [
    S3,
    { ...S3, AZLIN_DROP_PUBLIC_KEY: "" },
    { ...S3, AZLIN_DROP_PUBLIC_KEY: "   ", AZLIN_PLAIN_DRIVE: "" },
    { ...S3, AZLIN_PLAIN_DRIVE: "no" },
    { ...S3, AZLIN_PLAIN_DRIVE: "true" },
  ]) {
    const { puts, fetchImpl } = recorder();
    await assert.rejects(
      deliver({ raw: RAW, headers: new Headers(), env, now: NOW, fetchImpl }),
      /AZLIN_DROP_PUBLIC_KEY is not set/,
    );
    assert.equal(puts.length, 0, "nothing reaches the bucket");
  }
});

test("x_delta_azdrive_keys_17 the email handler fails the delivery of a Worker without a key, and sends nothing", async () => {
  const originalFetch = globalThis.fetch;
  const puts = [];
  globalThis.fetch = async (url) => {
    puts.push(url);
    return new Response(null, { status: 200 });
  };
  try {
    const message = {
      rawSize: RAW.length,
      raw: new Blob([RAW]).stream(),
      headers: new Headers(),
      setReject: () => assert.fail("a closed Worker fails the delivery; it does not reject the mail"),
    };
    await assert.rejects(worker.email(message, S3), /AZLIN_DROP_PUBLIC_KEY is not set/);
  } finally {
    globalThis.fetch = originalFetch;
  }
  assert.equal(puts.length, 0);
});

test("x_delta_azdrive_keys_17 the plain-drive word is read as the word it is, and a drop key wins over it", async () => {
  assert.equal(dropPublicKey({ AZLIN_PLAIN_DRIVE: "yes" }), null);
  assert.equal(dropPublicKey({ AZLIN_PLAIN_DRIVE: " YES " }), null);
  assert.throws(() => dropPublicKey({}), /not set/);
  // With a key the drive is encrypted, whatever the switch says.
  const key = dropPublicKey({
    AZLIN_PLAIN_DRIVE: "yes",
    AZLIN_DROP_PUBLIC_KEY: "11".repeat(32),
    AZLIN_DRIVE_ID: "d_1",
  });
  assert.equal(key.length, 32);
  const { puts, fetchImpl } = recorder();
  const result = await deliver({
    raw: RAW,
    headers: new Headers(),
    env: { ...PLAIN, AZLIN_DROP_PUBLIC_KEY: "11".repeat(32), AZLIN_DRIVE_ID: "d_1" },
    now: NOW,
    fetchImpl,
  });
  assert.equal(result.encrypted, true);
  assert.match(puts[0].url, /\/\.azlin\/drop\/[0-9a-f]{32}$/);
});
