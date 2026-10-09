# ksef-faktura 💯 Vibe-coded 🤷‍♂️

**Darmowy program do faktur VAT z wysyłką do KSeF**: PDF dla klienta i XML FA(3) dla Krajowego Systemu
e-Faktur, z czytelnym potwierdzeniem przed każdą wysyłką. Dla JDG i B2B, które wystawiają kilka faktur w
miesiącu. macOS, działa lokalnie, open source.

**Free Polish VAT invoicing with KSeF**: a small local app for macOS that makes the PDF you send your client
and the FA(3) e-invoice XML that KSeF (Krajowy System e-Faktur, API 2.0) requires, and sends it only after you
review and confirm.

> **Disclaimer.** This is an independent open-source project, not affiliated with or endorsed by the Ministry
> of Finance (Ministerstwo Finansów) or KSeF. It is not tax or accounting advice. You are responsible for the
> invoices you issue: try the KSeF test environment first, check the PDF and XML, and ask your accountant when
> unsure. An invoice sent to KSeF production cannot be withdrawn, only corrected.

## Po polsku w skrócie

- Wypełniasz fakturę w przeglądarce (na Twoim komputerze, nic nie trafia do chmury) i od razu widzisz PDF.
  W kolejnym miesiącu: „Copy lines from …”, zmieniasz liczbę godzin, zapisujesz.
- Faktury w PLN i w walutach obcych (np. EUR, z kursem NBP z dnia poprzedzającego sprzedaż, pobieranym jednym
  kliknięciem), stawki 23/8/5% oraz „np I” / „np II” dla usług dla firm zagranicznych, PKWiU, SWIFT.
- Faktury korygujące (KOR) dla faktur już wysłanych do KSeF.
- Przed wysyłką: walidacja (NIP, rachunek, sumy, daty, oficjalny schemat XSD FA(3)), podsumowanie i
  potwierdzenie numerem faktury. Token KSeF trzymany jest w pęku kluczy macOS (Keychain), nie w plikach.
- Pobieranie faktur wystawionych wcześniej (np. w innym programie) z KSeF i porównanie ich XML z Twoim,
  żeby mieć pewność, że nowe faktury wyglądają tak samo.
- PDF po polsku lub angielsku.

Instrukcje są po angielsku (poniżej i w [tutorials/](tutorials/)). Najprościej: pobierz zip z
[Releases](https://github.com/pochka15/ksef-faktura/releases/latest), otwórz folder w Claude Code i napisz
„follow INSTALL_PROMPT.md”: Claude przeprowadzi Cię przez instalację, konfigurację i pierwszą fakturę.

## Install

**From a release** (Apple Silicon Macs):

1. Download `ksef-macos-apple-silicon.zip` from
   [Releases](https://github.com/pochka15/ksef-faktura/releases/latest) and unzip it.
2. The binary is not signed by Apple, so macOS blocks it until you remove the download quarantine:
   ```bash
   xattr -d com.apple.quarantine ksef/ksef
   ```
3. Put it on your PATH, e.g. `mkdir -p ~/.local/bin && cp ksef/ksef ~/.local/bin/` (and add
   `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc` if it is not there yet).

Or let Claude Code do all of it, including your config: open the unzipped folder in Claude Code and say
"follow [INSTALL_PROMPT.md](INSTALL_PROMPT.md)". It asks before anything is sent to KSeF.

**From source** (Rust 1.85+, any Mac):

```bash
git clone https://github.com/pochka15/ksef-faktura && cd ksef-faktura
make install        # puts `ksef` in ~/.cargo/bin
```

## Use

```bash
ksef                # opens the page in your browser; the first run creates ~/.config/ksef/config.json first
```

`ksef` runs a page on `http://127.0.0.1` (only your machine, guarded by a random token in the URL) and keeps a
small menu in the terminal for the two things that need a terminal: `c` edits config.json, `t` stores a KSeF
token in the Keychain. Quitting the menu stops the page. The page talks only to KSeF production;
`ksef --test-envs` adds the test and demo environments for trying things out.

Start with the [tutorials](tutorials/), in order:

1. [Getting started](tutorials/01-getting-started.md): config and the first invoice
2. [The KSeF test environment](tutorials/02-test-environment.md): a fake NIP, a test token, a test send
3. [Production](tutorials/03-production.md): your real token, the first real send, corrections
4. [Consistency with your past invoices](tutorials/04-consistency.md): fetch from KSeF, compare, share with a friend
5. [JSON mode](tutorials/05-json-mode.md): `ksef run job.json` for scripts and Claude
6. [When KSeF changes](tutorials/06-when-ksef-changes.md): the monthly 5-minute check

## What it does

| | |
|---|---|
| New invoice (page) | a form with a live PDF preview; "Copy lines from …" reuses last month's lines |
| PDF | `~/Desktop/Faktura 1-09-2026.pdf` (or `Invoice ...` in English), a classic Polish invoice layout |
| XML | `~/.config/ksef/invoices/1-09-2026/invoice.xml`, checked against the official XSD on save |
| Currencies | PLN, or EUR and others with the NBP rate (one click), VAT shown in PLN, foreign buyers (EU VAT number or other tax id) |
| Send (invoice page) | review → type the invoice number → send → UPO saved (with `--test-envs`: pick test/demo/prod; `yes` on test/demo) |
| Correct (invoice page) | a correcting invoice (KOR) for one already in KSeF: lines before/after, the difference to pay or refund |
| KSeF (page) | downloads the invoices you issued (from any program) and compares their XML with yours |
| `ksef run job.json` | the same operations as JSON in / JSON out |
| `ksef config`, `ksef token --env E` | the terminal menu's `c` and `t` as commands |

Nothing reaches KSeF unless you send from an invoice's page (or a JSON job with `confirm`) and confirm it.
Tokens live in the macOS Keychain, not in files. Your invoices and config stay in `~/.config/ksef/`.

### Not supported (yet)

- Other systems than macOS (it uses the Keychain, `xmllint` and `open`).
- Payment methods other than bank transfer; split payment (MPP), cash accounting, reverse charge, margin
  schemes, VAT exemptions (`zw`), 0% rates, advance invoices.
- A second correction of the same invoice, correcting a correction or the seller's own data.
- Authentication by qualified certificate or seal: only KSeF tokens.

Issues and pull requests are welcome.

## Building a release zip

`make dist` writes `target/dist/ksef-macos-apple-silicon.zip` (~15 MB): the binary, `INSTALL_PROMPT.md`, the
tutorials, the example config and the license. Send it any way you like, or attach it to a GitHub release. For
Intel Macs, run `rustup target add x86_64-apple-darwin` once; `make dist` then builds a universal zip.

## Layout

`src/`: `money` / `ids` / `words` / `i18n` (values, checksums, amount in words), `invoice` (model),
`xml` (FA(3)), `pdf` + `assets/invoice.typ` (embedded Typst), `validate` (own rules + XSD via `xmllint`),
`inspect` (compare two XMLs), `ksef/` (API client: `http`, `crypto`, `client`), `config`, `store`, `keychain`,
`editor`, `draft` (invoice-building commands), `app` (operations), `api` + `server` (the page's endpoints and
the localhost HTTP server), `shell` (terminal menu), `job` (JSON), `main`.

`web/`: the page, Svelte 5 + Vite (`App.svelte` routes; `Editor`, `Invoice`, `Ksef`, `Invoices` are the
pages). `make web` builds it into `src/assets/index.html`, which is committed and embedded in the binary, so
building the app needs no Node. Only to change the page: Node 18+ and pnpm, `make web-setup`, then `make web`.

Unit tests sit next to the code; `make test` needs no network. `tests/fixtures/golden-fa3*.xml` are the exact
XML the app produces for fixed samples (PLN, EUR with np I, a correction). If they change, the bytes sent to
KSeF changed.

## License

MIT, see [LICENSE](LICENSE). Bundled third-party files keep their own terms:

- Liberation Sans, in the PDF: SIL Open Font License 1.1 ([assets/fonts/LICENSE-LiberationSans](assets/fonts/LICENSE-LiberationSans)).
- Bricolage Grotesque, on the page (embedded in `src/assets/index.html`): SIL Open Font License 1.1
  ([web/LICENSE-BricolageGrotesque](web/LICENSE-BricolageGrotesque)).
- `assets/xsd/`: the official FA(3) 1-0E schemas published by the Ministry of Finance, unchanged, from
  [CIRFMF/ksef-api](https://github.com/CIRFMF/ksef-api/tree/main/faktury/schemy/FA).
