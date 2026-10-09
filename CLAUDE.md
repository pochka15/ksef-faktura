# ksef

- Rust app for Polish invoices: PDF (embedded Typst, `assets/invoice.typ`) + FA(3) XML (`src/xml.rs`) + KSeF API 2.0
  client (`src/ksef/`). Errors are `String`s; no clap, no async. `make test` / `make lint` must pass, no network.
- UI: `ksef` serves `web/` (Svelte 5 + Vite) on 127.0.0.1 with a random URL token (`src/server.rs`, endpoints in
  `src/api.rs`) and keeps a terminal menu for config and tokens (`src/shell.rs`). After changing `web/`, run
  `make web` (Node 18+: `nvm use 24` first; this machine's default node is 16) and keep the rebuilt
  `src/assets/index.html`: it is embedded with `include_str!`. `make web-fmt` formats the Svelte sources.
- The page offers only KSeF prod; `ksef --test-envs` adds test and demo (`server::Site::envs`, enforced in
  `server::post`). JSON jobs always need an explicit `env`.
- Never send anything to KSeF (any environment) or read the Keychain without the user asking in this conversation.
  `ksef run` with `{"action":"send"}` and no `confirm` is safe: it only returns the plan. When trying the page,
  use a temp `KSEF_HOME` with `examples/config.example.json`, not the user's data.
- Real personal data (NIP, bank account, addresses) lives in `~/.config/ksef/config.json`, never in this repo.
  Fixtures and examples use fake data (`5265877635`, `1111111111`, account `61 1090 1014 0000 0712 1981 2874`).
- Changing `src/xml.rs` changes what KSeF receives: run `make golden`, show the user the diff of
  `tests/fixtures/golden-fa3.xml` (PLN, Polish buyer), `golden-fa3-eur-np.xml` (EUR, np I, UK buyer) and
  `golden-fa3-kor.xml` (a correction: KOR, StanPrzed rows, negative sums), and keep `validate::xsd` passing. Element order must follow `assets/xsd/FA3.xsd`.
- API reference: https://api-test.ksef.mf.gov.pl/docs/v2 (OpenAPI), changelog
  https://github.com/CIRFMF/ksef-api/blob/main/api-changelog.md, docs https://github.com/CIRFMF/ksef-docs.
- `INSTALL_PROMPT.md` is run by other people's Claude: keep its JSON actions, file paths and steps in sync with
  `src/job.rs`, `src/main.rs` and the tutorials. `make dist` zips it with the binary.
- Tutorials in `tutorials/` are user-facing; keep them in sync with the page (`web/src/`), the commands in
  `src/main.rs` and the JSON actions in `src/job.rs`.
