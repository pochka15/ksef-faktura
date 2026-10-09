# 6. When KSeF changes

The API is versioned (2.x) and most releases only add things, but the XML rules have been tightened before
(2.4.0: stricter characters). Once a month, before sending:

1. Read the top of the [API changelog](https://github.com/CIRFMF/ksef-api/blob/main/api-changelog.md). This app
   uses only token auth (`/auth/*`), `/security/public-key-certificates`, online sessions (`/sessions/online*`,
   `/sessions/{ref}/invoices*`) and, for fetch, `/invoices/query/metadata` + `/invoices/ksef/{n}`. Changes elsewhere
   don't matter.
2. Check whether a new FA schema is announced (FA(4), or a new `wersjaSchemy`). The session sends
   `FA (3)` / `1-0E` (constants in `src/xml.rs`).
3. If in doubt, send to `test` first (tutorial 2). It runs the newest release candidate, so breaking changes
   show up there before production.

## If something broke

Ask Claude in this repo, for example: "KSeF changelog 2.9 changed X, update the client", or paste the error
the send showed. The usual places:

| change | file |
|---|---|
| endpoint / payload / status codes | `src/ksef/client.rs` (+ its tests with canned responses) |
| new XSD version | `assets/xsd/*.xsd`, `src/xml.rs`, then `make golden` and review the diff |
| new VAT rate or annotation | `src/money.rs` (`Vat`), `src/xml.rs` |
| PDF look | `assets/invoice.typ`, preview with `make preview` |
| the page | `web/src/*.svelte`, then `make web` (`make web-setup` once; Node 18+, pnpm) |

`make test` must pass before you send again.
