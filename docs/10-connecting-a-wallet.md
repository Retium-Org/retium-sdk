# Connecting a wallet from a web page

How a web page asks the Retium wallet to identify its user and to sign transactions. The page never sees a key. Every transaction gets its own approval screen in the wallet, and the user can say no.

## Load the provider

Users with the Retium browser extension already have `window.retium`. For everyone else, load the wallet's script. Pin a released version and its hash, so the browser refuses a file that was swapped on the server.

```html
<script src="https://wallet.retium.org/connect/sdk/retium-sdk-1.3.0.js"
        integrity="sha384-FTG6s31+cSLKvZXfREum814s/L2AnqVyikuFQvMKIvFfAz40ksTcaXVbRmkHGGTU"
        crossorigin="anonymous"></script>
```

The current versions and hashes are listed at [wallet.retium.org/connect/sdk/manifest.json](https://wallet.retium.org/connect/sdk/manifest.json).

The provider exists on `https://` pages and on `http://localhost`. It does not exist on other `http://` addresses (use `localhost` for development, not a LAN address), on `file://` pages, or reliably inside iframes, so call it from the top-level window.

The extension usually injects before your code runs. If it has not, wait briefly for the `retium#initialized` event.

```js
async function waitForRetium(timeoutMs = 300) {
  if (window.retium) return window.retium;
  return new Promise((resolve) => {
    const t = setTimeout(() => resolve(window.retium ?? null), timeoutMs);
    window.addEventListener('retium#initialized',
      () => { clearTimeout(t); resolve(window.retium); }, { once: true });
  });
}
```

## Connect

Call it from a click handler. Popup blockers and the approval window both depend on that.

```js
async function connect() {
  const retium = await waitForRetium();
  if (!retium) { window.open('https://wallet.retium.org'); return null; }
  try {
    const { address, publicKey, chainId } =
      await retium.request({ method: 'retium_requestAccounts' });
    return { address, publicKey, chainId };
  } catch (e) {
    if (e?.code === 4001) return null;   // the user said no, so stay quiet
    throw e;
  }
}
```

Call `retium_requestAccounts` again on every page load instead of keeping the address in storage. For a site the user already approved, it answers without a popup, and it picks up a switched or revoked account.

## Methods

Every method is `await retium.request({ method, params })`.

| Method | Params | Result |
|---|---|---|
| `retium_requestAccounts` | none | `{address, publicKey, chainId}` |
| `retium_chainId` | none | `{chainId}` |
| `retium_sendTransfer` | `{to, amountAtomic}` | `{txId}` |
| `retium_call` | `{contractId, method, args, amountAtomic?}` | `{txId}` |
| `retium_signAuth` | `{nonce, statement?}` | `{message, signature, publicKey, address}` |
| `retium_apiKey` | `{op, keyHash, label?, tier?}` | `{txId}` |

### Sending RTM

```js
const { txId } = await retium.request({
  method: 'retium_sendTransfer',
  params: { to: 'rete...', amountAtomic: '1500000000000000000' },   // 1.5 RTM
});
```

`amountAtomic` is a decimal string of atomic units. Never pass a JavaScript number, which loses precision. Convert with `BigInt`.

```js
function toAtomic(s) {   // "1.5" gives "1500000000000000000"
  const [w, f = ''] = s.split('.');
  return (BigInt(w) * 10n ** 18n + BigInt((f + '0'.repeat(18)).slice(0, 18))).toString();
}
```

### Calling a contract

```js
const { txId } = await retium.request({
  method: 'retium_call',
  params: {
    contractId: '<64 hex>',
    method: 'transfer',
    args: ['rete...', '2500000'],
    amountAtomic: '0',          // RTM attached to the call, if the contract expects some
  },
});
```

The signature covers the exact argument text. An array is joined as `args.map(String).join(',')`, with no spaces, quotes or JSON. Pass large numbers as strings, because `String(1e21)` is `"1e+21"`. An argument must not contain `,` `(` `)` or `::` (see [Writing contracts](04-writing-contracts.md#arguments)).

### Logging in with a wallet

`retium_signAuth` signs a login message so your backend can tie a session to an address.

* The `nonce` must come from your backend, used once, 16 to 64 letters and digits. A nonce made in the browser makes the login replayable.
* Send `message` to your backend exactly as you received it. Parsing and re-serialising it breaks the signature.
* The wallet signs with its identity key, not with a transaction key. The backend must check the signature against `publicKey`, then read `GET https://rpc.retium.org/api/wallet/{address}` and check that `blake3(publicKey)` equals its `login_key_hash`. A wallet that has never sent a transaction has no `login_key_hash` yet, and then `publicKey` must derive `address` itself (see [Wire reference](08-wire-reference.md#2-accounts)). Then check the domain and nonce in the message.
* Never accept a login because `publicKey` derives the address alone once `login_key_hash` is published. A transaction key seen on the chain is public, and the identity key is the one that proves the user.

### Deploying

Deploy contracts with the SDK command line rather than from a page. The dry run there runs every check the chain runs before you pay.

## After you get a `txId`

A `txId` means the transaction was submitted, not that it is settled. Poll `GET https://rpc.retium.org/api/transaction/{txId}` until `finality` is `HardFinal`, and for a contract call check that `exec_status` is `ok`. Reads like this need no API key.

## Errors

| Code | Meaning | What to do |
|---|---|---|
| 4001 | the user rejected it | nothing, it is not an error |
| 4100 | no wallet set up | send the user to wallet.retium.org |
| 4101 | the wallet is on a different account than the one connected | connect again |
| 4102 | the site is not connected | call `retium_requestAccounts` |
| 4200 | method not supported | |
| 4300 | the network refused it, nothing was spent | show the message |
| 4301 | the outcome is unknown | check the transaction, do **not** resubmit |
| 4900 | the wallet could not reach the network | try again later |
| -32002 | a request is already open | wait for it |
| -32602 | invalid params | fix the request |

## Rules that save you a support queue

* Call the wallet only from a click handler.
* Send one request at a time and disable the button while it is open. Double clicks are the most common cause of confused users.
* Do not put a timeout on a request. A user with two-factor signing may take minutes on their phone.
* Never use floating point for amounts.
* Show the contract id and the amount the user is approving, not just a name.

Read on with [Errors](11-errors.md).
