// The stable name of a message in a plain (not encrypted) Azlin drive, as AzMail writes and reads
// it (examples/azul-mail/AZLIN_MAIL.md, section 2):
//
//   mail/<Folder>/<stamp>-<hash>.eml     20261010T080000Z-3f2a9c1e5b7d4a60.eml
//
// <stamp> is when the message arrived (UTC, the SigV4 date format), <hash> the first 16 hex
// digits of the SHA-256 of its exact bytes: the same message stored twice (a retry) lands on the
// same name.

import { bytesToHex } from "./azd1.js";

/** `20261010T080000Z` for 2026-10-10 08:00:00 UTC. */
export function amzDate(seconds) {
  const iso = new Date(seconds * 1000).toISOString(); // 2026-10-10T08:00:00.000Z
  return iso.slice(0, 19).replace(/[-:]/g, "") + "Z";
}

/** `<stamp>-<hash16>.eml` of `raw` arriving at `seconds`. */
export async function objectName(raw, seconds) {
  const hash = bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", raw)));
  return `${amzDate(seconds)}-${hash.slice(0, 16)}.eml`;
}

/** `mail/<folder>/<name>`. */
export function messageKey(folder, name) {
  return `mail/${folder}/${name}`;
}
