# Identity And Devices

Zim separates a person's account identity from the keys held by their devices.
Vault authorization ultimately belongs to device keys.

## DID Forms

- `did:key` is self-describing and identifies one concrete key. Native peers
  use this form for direct identity.
- `did:web` names a key roster published by a web host. Zim uses it to represent
  an account whose current devices are managed through a hub.

Sharing to an account DID resolves its current roster and grants access to each
resolved device key. The account name itself is not a decryption key.

## Enrollment And Sharing

Hub enrollment proves that a device belongs to an authenticated account. Vault
sharing is a separate act. Enrolling a new device does not retroactively add it
to existing vaults, because no service without an existing vault secret can
mint the required encrypted grant.

Account sharing is therefore a snapshot of the device roster at share time.
An existing shareholder must explicitly grant a newly enrolled device access.

Removing a device from a hub prevents future hub authentication and removes it
from future DID resolution. It does not remove the key from existing vault
histories or erase data already received.

## Browser Key Custody

The browser generates and unlocks its key client-side. The hub stores a copy
encrypted under a passphrase-derived key so the browser identity can be
recovered in another session.

Passive theft of the hub database does not immediately reveal the browser key,
but permits offline passphrase guessing. A live malicious hub is more powerful:
it controls the application delivered to the browser and could capture a
passphrase or unlocked key. Browser users therefore trust the code served by
their hub.

The unlocked browser seed is retained in tab-scoped session storage for the
active session; it is not confined exclusively to WASM memory.

## Hub And Google Identity

Google authentication establishes the account session used by the hosted
service. Hub administrators decide which accounts may use that service.

An account is bound to the sign-in method that created it, and to that
provider's stable account id (Google's `sub`). Accounts are never linked: the
same email arriving through a different sign-in method, or through a different
Google account, is refused rather than merged into the existing account.
Neither Google nor the hub account session replaces vault signatures or sealed
device grants.

A `did:web` identity trusts its named hub to publish the correct key roster.
The roster is intentionally minimal and does not claim the complete semantics
of W3C DID Core.

## Mobile Sign-In Handoff

The iOS app is a hosted device like the browser: the same account session, the
same escrowed browser key, the same passphrase unlock. It has no device key of
its own. Google refuses sign-in inside embedded webviews, so the app runs the
sign-in in the system web-auth sheet and hands the result back:

1. The app keeps a random verifier and sends its SHA-256 (`mobile_challenge`)
   when it starts the provider's sign-in in the sheet.
2. After the normal handshake the hub issues a one-time token (random, stored
   only as a hash, valid about 60 seconds) and redirects the sheet to
   `zim-auth://callback?token=…`. No cookie is set in the system browser.
3. The app's webview loads `/auth/mobile/complete?token=…&verifier=…`. The hub
   deletes the token, checks the verifier against the challenge, and only then
   sets the ordinary session cookie in the webview and lands on the SPA, which
   continues to the passphrase unlock.

Another app can claim the `zim-auth` scheme and read the token. It cannot
redeem it without the verifier, which never leaves the app, and a wrong attempt
still burns the token. Failures hand `zim-auth://callback?error=failed` back so
the sheet closes and the app returns to the sign-in page.
