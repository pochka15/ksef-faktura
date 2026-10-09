# 1. Getting started

## Install

```bash
make install
```

You need Rust (`rustup`). `xmllint` for schema validation ships with macOS.

## Config

```bash
ksef config
```

The first run creates `~/.config/ksef/config.json` from `examples/config.example.json` and opens it in TextEdit.
Save it (⌘S), then press Enter in the terminal. To use vim instead, set `"editor": "vim"` (or `"code --wait"`).

```json
{
  "seller": {
    "name": "Your Name",
    "nip": "1234567890",
    "address_line1": "ul. Ulica 1/2",
    "address_line2": "00-001 Miasto",
    "phone": "500600700",
    "bank_name": "Twój Bank",
    "bank_account": "00 0000 0000 0000 0000 0000 0000",
    "swift": null,
    "issuer_name": "Your Name"
  },
  "buyers": {
    "client": { "name": "...", "nip": "...", "address_line1": "...", "address_line2": "..." },
    "uk": {
      "name": "...", "country": "GB", "tax_id": "GB123456789",
      "address_line1": "1 Example Street", "address_line2": "EC1A 1AA London", "currency": "EUR"
    }
  },
  "default_buyer": "client",
  "language": "pl",
  "currency": "PLN",
  "payment_days": 7,
  "place_of_issue": null,
  "output_dir": "~/Desktop",
  "editor": null,
  "test_nip": null
}
```

- The NIP has no dashes. The NIP checksum and the bank account checksum are verified, so typos show up right away.
- `address_line1` and `address_line2` are printed as written and map to `AdresL1` and `AdresL2` in the XML.
- `place_of_issue` (e.g. `"Warszawa"`) is printed and sent as `P_1M`; leave it `null` to skip it.

### Foreign buyers and other currencies

- A buyer abroad has a 2-letter `country` (`GB`, `DE`, ...) and a `tax_id` instead of `nip`. For an EU
  country the `tax_id` is the EU VAT number (with or without the prefix, e.g. `DE123456789`); elsewhere any tax
  number (`GB123456789`). The PDF then prints country names for both parties, and "Numer identyfikacji
  podatkowej" for the buyer.
- `currency` (top level) is the default invoice currency; a buyer's own `currency` (e.g. `"EUR"`) wins for
  invoices to that buyer. An invoice in a foreign currency needs the NBP rate of the last business day before
  the sale date (art. 31a ustawy o VAT): the page's **Get NBP rate** button fetches it from api.nbp.pl, and the
  PDF prints it with the VAT converted to PLN.
- `swift` (e.g. `"BPKOPLPW"`) is printed next to the account and sent in the XML. For a currency account, write
  it as an IBAN with the `PL` prefix (`"PL61 1090 ..."`); the PDF adds the currency (`| EUR`).
- VAT `np I` is for services whose place of supply is outside Poland (e.g. to a business outside the EU,
  art. 28b), `np II` for services to an EU business (art. 100 ust. 1 pkt 4). Both print as "np" with no VAT.
- The page sends to and fetches from KSeF production only. `ksef --test-envs` adds the test and demo
  environments for trying things out (tutorial 2).

## First invoice

```bash
ksef
```

This opens the page in your browser and leaves a small menu in the terminal (`o` reopens the page, `c` edits
the config, `t` stores a token, `q` quits and stops the page). Keep the terminal open while you use the page.

**New invoice** is a form with a live PDF preview next to it:

- **Lines**: name, quantity, unit, net price (`100`, `100,00` or `100.00`), VAT (23/8/5%). The net amount and
  the totals update as you type. **Copy lines from 1/09/2026** reuses your previous invoice's lines, so next
  month you copy, change the hours, save.
- **Issue date** moves the sale date, the due date (+`payment_days`) and the number (`n/MM/YYYY`, counting this
  month's invoices) until you change one of them yourself. Changed fields get an orange border; **auto**
  brings the default back.
- **Buyer** (from config.json) and **Polski / English** for the PDF language.

**Save and write PDF** refuses invoices with a wrong NIP, no lines, a future issue date or XML that fails the
official FA(3) schema, and shows why. It writes `~/Desktop/Faktura 1-10-2026.pdf` and opens the invoice's page.
Nothing is sent to KSeF. That happens only from an invoice's page, covered in tutorials 2 and 3.

On an invoice's page: **Open PDF** (in Preview), **Edit** (until it is in KSeF demo/prod), the checks, sending,
and comparing with KSeF. **Invoices** lists everything with where it was sent. `ksef paths` shows where files
are stored.
