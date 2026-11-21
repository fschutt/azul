import assert from "node:assert/strict";
import { test } from "node:test";

import { bytesToHex } from "../src/azd1.js";
import { amzTimestamp, putObject, s3Settings, sign, uriEncode } from "../src/s3.js";

const encoder = new TextEncoder();

test("the S3 reference's worked PUT example signs to its published signature", async () => {
  // "Signature Calculations for the Authorization Header: Transferring Payload in a Single
  // Chunk", the PUT Object example (azul-storage's sigv4 tests check the same).
  const payload = encoder.encode("Welcome to Amazon S3.");
  const payloadHash = bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", payload)));
  assert.equal(payloadHash, "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072");
  const { signature, authorization } = await sign({
    method: "PUT",
    canonicalUri: uriEncode("/test$file.text", true),
    headers: {
      Date: "Fri, 24 May 2013 00:00:00 GMT",
      Host: "examplebucket.s3.amazonaws.com",
      "x-amz-content-sha256": payloadHash,
      "x-amz-date": "20130524T000000Z",
      "x-amz-storage-class": "REDUCED_REDUNDANCY",
    },
    payloadHash,
    accessKeyId: "AKIAIOSFODNN7EXAMPLE",
    secretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    region: "us-east-1",
    service: "s3",
    amzDate: "20130524T000000Z",
  });
  assert.equal(signature, "98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd");
  assert.equal(
    authorization,
    "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, " +
      "SignedHeaders=date;host;x-amz-content-sha256;x-amz-date;x-amz-storage-class, " +
      "Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd",
  );
});

test("a PUT goes path style to the bucket with its body's hash, signed", async () => {
  const sent = [];
  const fetchImpl = async (url, init) => {
    sent.push({ url, init });
    return new Response(null, { status: 200 });
  };
  const env = {
    S3_ENDPOINT: "https://s3.example.test/",
    S3_BUCKET: "drive-bucket",
    S3_ACCESS_KEY_ID: "AKIDTEST",
    S3_SECRET_ACCESS_KEY: "secret",
    S3_SESSION_TOKEN: "session",
  };
  const body = encoder.encode("bytes");
  const now = Date.UTC(2026, 9, 10, 8, 0, 0);
  const url = await putObject({
    env,
    key: "mail/Inbox/20261010T080000Z-0011223344556677.eml",
    body,
    contentType: "message/rfc822",
    now,
    fetchImpl,
  });
  assert.equal(url, "https://s3.example.test/drive-bucket/mail/Inbox/20261010T080000Z-0011223344556677.eml");
  assert.equal(sent.length, 1);
  const { init } = sent[0];
  assert.equal(init.method, "PUT");
  assert.equal(init.headers["x-amz-date"], amzTimestamp(now));
  assert.equal(init.headers["x-amz-date"], "20261010T080000Z");
  assert.equal(init.headers["x-amz-security-token"], "session");
  assert.match(
    init.headers.authorization,
    /^AWS4-HMAC-SHA256 Credential=AKIDTEST\/20261010\/us-east-1\/s3\/aws4_request, SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date;x-amz-security-token, Signature=[0-9a-f]{64}$/,
  );
  assert.equal(init.body, body);
});

test("a refused PUT fails the delivery, and a missing setting is named", async () => {
  const env = {
    S3_ENDPOINT: "https://s3.example.test",
    S3_BUCKET: "b",
    S3_ACCESS_KEY_ID: "a",
    S3_SECRET_ACCESS_KEY: "s",
  };
  await assert.rejects(
    putObject({
      env,
      key: "k",
      body: new Uint8Array(1),
      contentType: "x/y",
      fetchImpl: async () => new Response(null, { status: 503 }),
    }),
    /503/,
  );
  assert.throws(() => s3Settings({ S3_ENDPOINT: "x" }), /S3_BUCKET, S3_ACCESS_KEY_ID, S3_SECRET_ACCESS_KEY/);
  assert.equal(uriEncode("a b/ü$", true), "a%20b/%C3%BC%24");
});
