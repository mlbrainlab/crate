// The Crate Pro signer (Cloudflare Worker). Crate posts a Lemon Squeezy
// license key; if Lemon Squeezy says it's an active key for Crate Pro, the
// Worker answers with a key signed with Crate's private key, which the app
// then checks offline. Nothing is stored.
//
// Secrets and variables (see README.md):
//   LICENSE_PRIVATE_KEY  secret: ~/crate-license/license-ed25519.pem (PKCS#8 PEM)
//   LS_STORE_ID          the Lemon Squeezy store's id
//   LS_PRODUCT_ID        Crate Pro's product id

import { payload, encode } from "./key.mjs";

const json = (status, body) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

async function privateKey(pem) {
  const b64 = pem.replace(/-----[^-]+-----/g, "").replace(/\s+/g, "");
  const der = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  return crypto.subtle.importKey("pkcs8", der, { name: "Ed25519" }, false, ["sign"]);
}

export default {
  async fetch(request, env) {
    if (request.method !== "POST") return json(405, { error: "POST only" });
    let licenseKey;
    try {
      licenseKey = String((await request.json()).license_key ?? "").trim();
    } catch {
      return json(400, { error: "expected JSON" });
    }
    if (!licenseKey || licenseKey.length > 200) return json(400, { error: "no license key" });

    const ls = await fetch("https://api.lemonsqueezy.com/v1/licenses/validate", {
      method: "POST",
      headers: { accept: "application/json", "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ license_key: licenseKey }),
    });
    if (ls.status >= 500) return json(502, { error: "Lemon Squeezy is unavailable" });
    const answer = await ls.json().catch(() => ({}));
    const meta = answer.meta ?? {};
    const ours =
      String(meta.store_id) === String(env.LS_STORE_ID) &&
      String(meta.product_id) === String(env.LS_PRODUCT_ID);
    if (!answer.valid || !ours || answer.license_key?.status === "disabled") {
      return json(404, { error: "not a Crate Pro key" });
    }

    const bytes = payload(`ls:${meta.order_id}`);
    const signature = await crypto.subtle.sign("Ed25519", await privateKey(env.LICENSE_PRIVATE_KEY), bytes);
    return json(200, { key: encode(bytes, signature) });
  },
};
