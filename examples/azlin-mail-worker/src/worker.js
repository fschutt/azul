// The Cloudflare Email Worker that files a domain's incoming mail into an Azlin drive. It runs on
// the CUSTOMER's own Cloudflare account (Email Routing -> this Worker); Azlin never runs a mail
// server and never sees the mail.
//
// - An encrypted drive (every Azlin drive; the variable AZLIN_DROP_PUBLIC_KEY is set - the app
//   sets it with the customer's own Cloudflare API token): the message is sealed to the drive's
//   drop key (AZD1, src/azd1.js) and put at `.azlin/drop/<random>`; a device of the drive opens
//   it and files it into the encrypted mail folders. Neither the bucket nor this Worker keeps it
//   in the clear.
// - A plain bucket (NOT an Azlin drive), only when the configuration says so in so many words
//   (AZLIN_PLAIN_DRIVE = "yes"): the message's exact bytes go to `mail/Inbox/<stamp>-<hash>.eml`
//   (or `mail/Spam/`), the names AzMail reads (examples/azul-mail/AZLIN_MAIL.md).
// - Neither: the Worker FAILS CLOSED. A drive whose key was not set yet (the routing rules come
//   before it in a hurried install), or whose secret was removed, gets no plain message: the
//   node's operator would read it, and the drive's owner would not see it in the index.
//
// The Worker logs nothing of a message (no address, no subject). A failed PUT, and a delivery
// that fails closed, throw: the delivery fails and the sending server tries again later.

import { hexToBytes, newDropObjectKey, sealDrop } from "./azd1.js";
import { messageKey, objectName } from "./mailname.js";
import { putObject } from "./s3.js";

/** Cloudflare Email Routing takes messages up to 25 MiB. */
export const MAX_MESSAGE_BYTES = 25 * 1024 * 1024;

/** The folder of a message: Spam when an upstream filter flagged it, else Inbox. */
export function folderOf(headers) {
  const flag = (headers.get("x-spam-flag") || "").trim().toUpperCase();
  const status = (headers.get("x-spam-status") || "").trim().toLowerCase();
  return flag === "YES" || status.startsWith("yes") ? "Spam" : "Inbox";
}

/** Whether the configuration says, in so many words, that the bucket is a plain one. */
export function plainDriveSwitch(env) {
  return String(env.AZLIN_PLAIN_DRIVE || "").trim().toLowerCase() === "yes";
}

/**
 * The drop public key of an encrypted drive's configuration; `null` only for a bucket that the
 * configuration declares plain (AZLIN_PLAIN_DRIVE = "yes"). Without a key and without that word
 * it throws: no message is written, the sender tries again once the key is set.
 */
export function dropPublicKey(env) {
  const hex = (env.AZLIN_DROP_PUBLIC_KEY || "").trim();
  if (!hex) {
    if (plainDriveSwitch(env)) {
      return null;
    }
    throw new Error(
      "AZLIN_DROP_PUBLIC_KEY is not set: this Worker writes no plain mail into a drive unless " +
        'AZLIN_PLAIN_DRIVE is "yes" (a bucket that is not an Azlin drive)',
    );
  }
  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw new Error("AZLIN_DROP_PUBLIC_KEY is not 64 hex digits");
  }
  if (!env.AZLIN_DRIVE_ID) {
    throw new Error("AZLIN_DRIVE_ID is missing (an encrypted drive's drops name their drive)");
  }
  return hexToBytes(hex);
}

/**
 * Files `raw` (the RFC 5322 bytes) into the drive. Returns `{ key, folder, encrypted }`.
 * `now` (milliseconds) and `fetchImpl` are for the tests.
 */
export async function deliver({ raw, headers, env, now = Date.now(), fetchImpl }) {
  const received = Math.floor(now / 1000);
  const folder = folderOf(headers);
  const publicKey = dropPublicKey(env);
  let key;
  let body;
  let contentType;
  if (publicKey) {
    key = newDropObjectKey();
    body = await sealDrop({
      dropPublicKey: publicKey,
      driveId: String(env.AZLIN_DRIVE_ID),
      objectKey: key,
      received,
      folder,
      raw,
    });
    contentType = "application/octet-stream";
  } else {
    key = messageKey(folder, await objectName(raw, received));
    body = raw;
    contentType = "message/rfc822";
  }
  await putObject({ env, key, body, contentType, now, fetchImpl });
  return { key, folder, encrypted: publicKey !== null };
}

export default {
  async email(message, env) {
    if (message.rawSize > MAX_MESSAGE_BYTES) {
      message.setReject("Message too large");
      return;
    }
    const raw = new Uint8Array(await new Response(message.raw).arrayBuffer());
    await deliver({ raw, headers: message.headers, env });
  },
};
