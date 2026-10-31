// One S3 PutObject, signed with AWS Signature Version 4 through Web Crypto (HMAC-SHA256,
// SHA-256), path style (`<endpoint>/<bucket>/<key>`) as the Azlin storage nodes speak it.
// Checked against the S3 API reference's worked PUT example (test/s3.test.js).

import { bytesToHex } from "./azd1.js";

const encoder = new TextEncoder();

async function sha256Hex(bytes) {
  return bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)));
}

async function hmac(key, text) {
  const k = await crypto.subtle.importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, [
    "sign",
  ]);
  return new Uint8Array(await crypto.subtle.sign("HMAC", k, encoder.encode(text)));
}

/** RFC 3986 percent-encoding of every byte but the unreserved ones (and `/` in paths). */
export function uriEncode(text, keepSlash) {
  let out = "";
  for (const byte of encoder.encode(text)) {
    const c = String.fromCharCode(byte);
    if (/[A-Za-z0-9\-._~]/.test(c) || (keepSlash && c === "/")) {
      out += c;
    } else {
      out += "%" + byte.toString(16).toUpperCase().padStart(2, "0");
    }
  }
  return out;
}

/** `20130524T000000Z` of a time in milliseconds. */
export function amzTimestamp(ms) {
  return new Date(ms).toISOString().slice(0, 19).replace(/[-:]/g, "") + "Z";
}

/**
 * The SigV4 signature of one request without a query: `headers` (name -> value) are all signed.
 * Returns `{ signature, authorization }`.
 */
export async function sign({
  method,
  canonicalUri,
  headers,
  payloadHash,
  accessKeyId,
  secretAccessKey,
  region,
  service,
  amzDate,
}) {
  const names = Object.keys(headers)
    .map((name) => name.toLowerCase())
    .sort();
  const byName = Object.fromEntries(
    Object.entries(headers).map(([name, value]) => [
      name.toLowerCase(),
      String(value).trim().replace(/\s+/g, " "),
    ]),
  );
  const canonicalHeaders = names.map((name) => `${name}:${byName[name]}\n`).join("");
  const signedHeaders = names.join(";");
  const canonicalRequest = [
    method,
    canonicalUri,
    "",
    canonicalHeaders,
    signedHeaders,
    payloadHash,
  ].join("\n");
  const day = amzDate.slice(0, 8);
  const scope = `${day}/${region}/${service}/aws4_request`;
  const stringToSign = [
    "AWS4-HMAC-SHA256",
    amzDate,
    scope,
    await sha256Hex(encoder.encode(canonicalRequest)),
  ].join("\n");
  let key = await hmac(encoder.encode("AWS4" + secretAccessKey), day);
  key = await hmac(key, region);
  key = await hmac(key, service);
  key = await hmac(key, "aws4_request");
  const signature = bytesToHex(await hmac(key, stringToSign));
  return {
    signature,
    authorization:
      `AWS4-HMAC-SHA256 Credential=${accessKeyId}/${scope}, ` +
      `SignedHeaders=${signedHeaders}, Signature=${signature}`,
  };
}

/** The S3 settings of the Worker's environment; throws naming what is missing. */
export function s3Settings(env) {
  const missing = ["S3_ENDPOINT", "S3_BUCKET", "S3_ACCESS_KEY_ID", "S3_SECRET_ACCESS_KEY"].filter(
    (name) => !env[name],
  );
  if (missing.length) {
    throw new Error(`the Worker is not configured: ${missing.join(", ")} missing`);
  }
  return {
    endpoint: String(env.S3_ENDPOINT).replace(/\/+$/, ""),
    bucket: String(env.S3_BUCKET),
    region: env.S3_REGION ? String(env.S3_REGION) : "us-east-1",
    accessKeyId: String(env.S3_ACCESS_KEY_ID),
    secretAccessKey: String(env.S3_SECRET_ACCESS_KEY),
    sessionToken: env.S3_SESSION_TOKEN ? String(env.S3_SESSION_TOKEN) : null,
  };
}

/**
 * PUTs `body` at `key` of the configured bucket. Throws on any answer but 2xx: the Email Worker
 * then fails the delivery and the sending server tries again later (no mail is lost).
 */
export async function putObject({ env, key, body, contentType, now = Date.now(), fetchImpl }) {
  const s3 = s3Settings(env);
  const url = new URL(`${s3.endpoint}/${uriEncode(s3.bucket, false)}/${uriEncode(key, true)}`);
  const amzDate = amzTimestamp(now);
  const payloadHash = await sha256Hex(body);
  const headers = {
    host: url.host,
    "content-type": contentType,
    "x-amz-content-sha256": payloadHash,
    "x-amz-date": amzDate,
  };
  if (s3.sessionToken) {
    headers["x-amz-security-token"] = s3.sessionToken;
  }
  const { authorization } = await sign({
    method: "PUT",
    canonicalUri: url.pathname,
    headers,
    payloadHash,
    accessKeyId: s3.accessKeyId,
    secretAccessKey: s3.secretAccessKey,
    region: s3.region,
    service: "s3",
    amzDate,
  });
  const sent = { ...headers, authorization };
  delete sent.host; // fetch sets it
  const response = await (fetchImpl ?? fetch)(url.toString(), {
    method: "PUT",
    headers: sent,
    body,
  });
  if (!response.ok) {
    throw new Error(`the bucket answered ${response.status} to the PUT`);
  }
  return url.toString();
}
