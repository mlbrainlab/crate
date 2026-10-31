// Crate Pro keys, shared by the Worker and sign.mjs. The format is checked by
// app/src/license.rs: "CRATE-PRO-" + base64url(payload + 64-byte Ed25519
// signature), payload "1|<id>|<issued, YYYY-MM-DD>".

export function payload(id, issued = new Date().toISOString().slice(0, 10)) {
  if (id.includes("|")) throw new Error("the id can't contain |");
  return new TextEncoder().encode(`1|${id}|${issued}`);
}

// `signature`: a Buffer (node:crypto) or an ArrayBuffer (WebCrypto).
export function encode(payloadBytes, signature) {
  const sig = new Uint8Array(signature);
  const all = new Uint8Array(payloadBytes.length + sig.length);
  all.set(payloadBytes);
  all.set(sig, payloadBytes.length);
  let bin = "";
  for (const b of all) bin += String.fromCharCode(b);
  return "CRATE-PRO-" + btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
