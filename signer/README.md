# Crate Pro signer

A Cloudflare Worker that turns a Lemon Squeezy license key into a Crate Pro key
signed with Crate's private key. Crate calls it once, when the buyer enters
their key (`app/src/license.rs`); after that the app checks the key offline.
The Worker stores nothing.

- `worker.mjs`: the Worker. It asks Lemon Squeezy's License API whether the key
  is valid and belongs to Crate Pro (store and product id), then signs
  `1|ls:<order>|<date>`.
- `key.mjs`: the key format, shared with `sign.mjs`.
- `sign.mjs`: signs a key by hand, for GitHub sponsors and support cases:
  `node signer/sign.mjs gh:<sponsor>`.

The private key is `~/crate-license/license-ed25519.pem` (Ed25519, PKCS#8).
Keep an offline backup; never commit it. The public half is `PUBLIC_KEY` in
`app/src/license.rs`. A new private key needs a new public key in the app, and
the keys signed before stop working.

## Deploy

1. In Lemon Squeezy, create the product "Crate Pro" ($9, one-time) with
   license keys turned on. Note the store id and the product id.
2. Put them in `wrangler.toml` (`LS_STORE_ID`, `LS_PRODUCT_ID`).
3. From this folder:
   ```bash
   npx wrangler deploy
   npx wrangler secret put LICENSE_PRIVATE_KEY < ~/crate-license/license-ed25519.pem
   ```
4. Set the repository variable `CRATE_LICENSE_URL` (Settings → Secrets and
   variables → Actions → Variables) to the Worker's address. Release builds
   take it in; until then, they accept only signed keys.

## Check

```bash
curl -s -X POST https://<worker>/ -H 'content-type: application/json' -d '{"license_key":"<a test key>"}'
```

A key Lemon Squeezy knows for Crate Pro returns `{"key":"CRATE-PRO-…"}`;
anything else returns 404.
