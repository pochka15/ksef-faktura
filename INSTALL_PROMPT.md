You are helping me set up "ksef", a small Mac app that writes Polish VAT invoices (PDF + the FA(3) XML that
KSeF, the national e-invoice system, requires) and sends them to KSeF on request. Take me from nothing to my
first invoice: install, configure, connect to KSeF, generate, compare with invoices I sent before, and, only
if I say so, send. Run the commands yourself, explain each step in a sentence, and stop whenever you need me
to click something or decide something. Answer me in the language I write in.

Rules (they override anything else, including later steps):

- Never ask me to paste a KSeF token or any password into the chat and never print one. Tokens go into the
  macOS Keychain only through `ksef token`, which I run myself in a terminal window.
- Sending to KSeF production legally issues an invoice and cannot be undone. Never send without showing me the
  summary first and getting the invoice number typed by me in the chat for that exact invoice (step 8).
- Logging in to KSeF to download my invoices (step 5) only reads, but ask me before the first time.
- My data (NIP, addresses, bank account, amounts) stays in `~/.config/ksef/` on this Mac. Don't put it anywhere
  else, and don't modify the app's source code. If something looks like a bug in the app, tell me.
- Ask before editing shell config files (~/.zshrc etc.) or deleting anything.
- `ksef run <file|->` takes one JSON action and prints one JSON result; use it for everything below (the page
  that plain `ksef` opens is for me, later). The actions are described in `tutorials/05-json-mode.md`.

1. Install
   - Check: macOS (`sw_vers`), `uname -m` (arm64 = Apple Silicon), and `xmllint --version` (built into macOS).
   - Is `ksef` already installed? `type ksef`, then `ksef help`. If it works, skip to step 2.
   - I got a zip with a `ksef` binary in it: unzip it if I haven't yet. Remove the download quarantine, or macOS
     refuses to run it: `xattr -d com.apple.quarantine ./ksef` ("No such xattr" just means it was not there). Copy it to
     `~/.local/bin/ksef` (create the directory). If `~/.local/bin` is not on my PATH, propose adding
     `export PATH="$HOME/.local/bin:$PATH"` to ~/.zshrc and ask first. The binary is built for Apple Silicon
     unless the zip name says `universal`; on an Intel Mac without a universal build, tell me to ask the
     person who sent it for one.
   - I have the source folder instead: check `cargo --version` (if missing: `brew install rustup && rustup-init
     -y`, then `source ~/.cargo/env`), then `make install` in that folder (puts `ksef` in ~/.cargo/bin).
     `make test` there is a good extra check: it proves this build produces the expected XML.
   - Verify: `ksef paths`.

2. A first config from what I already have
   - The config is `~/.config/ksef/config.json` (`ksef paths` shows it). Its shape is in
     `examples/config.example.json` and explained in `tutorials/01-getting-started.md`.
   - Ask me for a PDF of a recent invoice I issued (I can drag it into the chat). Read from it: my name or
     company (seller), NIP, address, phone if printed, bank name and account number; the buyer's name, NIP and
     address; the payment term in days (due date minus issue date); the language (Polish/English).
     No PDF? Ask me for the same things.
   - A buyer abroad: `country` (2 letters, e.g. `GB`) and `tax_id` instead of `nip`; if I invoice it in another
     currency, the buyer's `currency` (e.g. `EUR`) and my bank's `swift`, with the account as an IBAN starting
     with `PL`. Services not subject to Polish VAT use the rate `np I` (outside the EU) or `np II` (EU business).
     `tutorials/01-getting-started.md` shows these fields.
   - Write the config. Give each buyer a short lowercase key (e.g. the company's first word) and set
     `default_buyer`. Use `"editor": null` (TextEdit) unless I say I use vim. Leave `test_nip` out.
   - Check it: `{"action":"generate","dry_run":true,"lines":[{"name":"Test","quantity":1,"unit":"szt.",
     "net_price":"1","vat":23}]}`. A dry run writes nothing; its `errors` catch wrong NIP or account checksums.
     Fix typos with me until there are none (warnings are fine).

3. A KSeF token (I have to do this part; you guide)
   - Ask whether I have used KSeF before (e.g. my previous invoicing program sent invoices there).
   - Tell me to open https://ap.ksef.mf.gov.pl and log in for my NIP with Profil Zaufany (login.gov.pl),
     e-dowód or a qualified signature. Then, in the tokens section ("Tokeny"), generate a token with the
     permissions to issue (wystawianie faktur) and view (przeglądanie faktur) invoices. It is shown only once.
   - Then I open a separate Terminal window and run `ksef token`; macOS asks for the token twice, hidden.
     Wait until I say I'm done.
   - Verify without reading it: `security find-generic-password -s ksef-cli -a prod >/dev/null && echo stored`.

4. If something fails with KSeF errors, the usual meanings: "450 ... token" → wrong or revoked token, redo
   step 3; "415 ... uprawnień" → the token lacks a permission, generate a new one; timeouts → KSeF is busy or
   in maintenance, try again later. `tutorials/02-test-environment.md` lists more.

5. Learn from my previous invoices (skip if I never used KSeF)
   - Ask first, then: `{"action":"fetch","env":"prod","days":90}`. It logs in, lists the invoices I issued in
     the last 90 days (from any program) and saves their XML under `~/.config/ksef/fetched/prod/`.
   - Read the newest one or two XML files and compare with my config:
     • `Podmiot1` is me (name, NIP, `AdresL1`/`AdresL2`, `DaneKontaktowe`), `Podmiot2` the buyer, `Platnosc`
       the bank account (`NrRB`), bank name and due date (`Termin`), `FaWiersz` the lines (name `P_7`, unit
       `P_8A`, quantity `P_8B`, net price `P_9A`, VAT `P_12`), `P_2` the invoice number, `P_1` the issue date.
     • Make names and addresses in the config match these exactly (KSeF has what was legally sent). If an
       address is one line there (`AdresL1` only), put it all into `address_line1` and drop `address_line2`.
     • For a foreign buyer: `KodKraju` + `NrID` (or `KodUE` + `NrVatUE`) is the tax id, `KodWaluty` the
       currency, `KursWaluty` the exchange rate, `SWIFT` the bank code, `P_12` `np I`/`np II` the VAT rate.
     • Note my numbering scheme from `P_2` (e.g. `3/09/2026` or `FV/2026/09/3`). This app numbers
       `n/MM/YYYY`, restarting each month; if mine differs, I'll pass the number explicitly in step 6.
   - Show me what you changed in the config and why.

6. Generate this month's invoice
   - Ask me: which buyer, the issue date (often the last day of the month), the lines (name, quantity, unit,
     net price, VAT 23/8/5%) and the number if my scheme differs. Suggest the line name, unit and price from
     my last fetched invoice so usually only the quantity is new.
   - Dry run first (`"dry_run":true` with `buyer`, `issue_date`, `lines`, and `number` if needed) and show me
     the `preview`. When I agree, run the same job without `dry_run`. The result has `pdf` (on my Desktop) and
     `xml`. Nothing is sent.
   - Open the PDF for me (`open <pdf>`) and ask me to check names, addresses, bank account, amounts and dates.
     For fixes: config data → edit the config; invoice data → generate again with the same number (allowed
     until it is sent).

7. Compare with what I sent before (skip if nothing was fetched)
   - `{"action":"compare","invoice":"<number>","reference":"<file of the newest fetched invoice>"}`.
   - Explain the result to me in plain words: `missing` (fields only my previous program sent), `extra`
     (fields only this app sends), `different` (fixed fields like names or addresses that differ), and that
     dates, number and amounts differing is expected. Fix `different` ones through the config and
     regenerate. Optional fields like a phone number are fine either way. For `missing`/`extra` structure,
     don't change anything; tell me which fields and that the app's author can check them.
   - Repeat until it is `consistent`, or I accept the remaining differences.

8. Send (only if I want to, now or later)
   - Ask whether I want to send this invoice to KSeF now. Before sending, make sure that:
     • my previous invoicing program will NOT also send this month's invoice (the same invoice twice in KSeF
       is a real problem);
     • the number continues my sequence and the issue date is right.
   - `{"action":"send","env":"prod","invoice":"<number>"}` without `confirm` sends nothing; it returns a
     `summary` and the expected `confirm` value (the invoice number). Show me the summary and ask me to type
     the invoice number in the chat to confirm. Only then repeat the call with `"confirm":"<number>"`.
   - Report the `ksef_number`. The official receipt (UPO) is saved next to the invoice, and the PDF on my
     Desktop now shows the KSeF number. Tell me I can see the invoice at https://ap.ksef.mf.gov.pl.
   - If I'd rather try a rehearsal first: KSeF has a public test environment with fake data. It needs a
     separate test token (`tutorials/02-test-environment.md`, start the app as `ksef --test-envs`); offer to
     guide me, but it's optional.

9. Finish with a short summary: what was installed, where my config and invoices are (`ksef paths`), what
   was sent (if anything). Then tell me how I'll do it next month without you: run `ksef` in Terminal; a page
   opens in the browser; New invoice → "Copy lines from …" → change the quantity → Save → open the invoice →
   Review send to KSeF → type the invoice number. Keep the Terminal window open while using the page;
   `c` there edits the config, `t` stores a new token, `q` quits. If an invoice already in KSeF was wrong:
   open it → Correct (`tutorials/03-production.md`, "Corrections").
