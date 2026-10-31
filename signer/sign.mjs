// Signs a Crate Pro key by hand (GitHub sponsors, support cases):
//   node signer/sign.mjs gh:<sponsor> [~/crate-license/license-ed25519.pem]
// Prints the key to give to the person.

import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { sign } from "node:crypto";
import { payload, encode } from "./key.mjs";

const [id, pem = `${homedir()}/crate-license/license-ed25519.pem`] = process.argv.slice(2);
if (!id || !/^[a-z]+:[^|\s]+$/.test(id)) {
  console.error("usage: node signer/sign.mjs <kind>:<id> [private-key.pem]   e.g. gh:octocat");
  process.exit(2);
}
const bytes = payload(id);
console.log(encode(bytes, sign(null, bytes, readFileSync(pem))));
