# 5. JSON mode

`ksef run job.json` (or `ksef run -` reading stdin) runs one action and prints one JSON result. Progress messages
go to stderr. The exit code is 0 when `"ok": true`. This is the interface for scripts and for a Claude skill.

## Actions

```jsonc
{"action": "list"}
{"action": "show", "invoice": "1/09/2026"}            // invoice is optional everywhere: default latest
{"action": "validate"}
{"action": "fetch", "env": "prod", "days": 90}        // env is required for fetch and send: test, demo or prod
{"action": "compare", "invoice": "1/10/2026", "reference": "/path/to/fetched.xml"}
```

### generate

```json
{
  "action": "generate",
  "buyer": "client",
  "language": "en",
  "issue_date": "2026-10-31",
  "from_last": true,
  "lines": [
    {"name": "Usługi IT", "quantity": 160, "unit": "godz.", "net_price": "100,00", "vat": 23}
  ],
  "dry_run": false
}
```

A foreign-currency invoice for a buyer abroad, not subject to Polish VAT:

```json
{
  "action": "generate",
  "buyer": "uk",
  "issue_date": "2026-09-07",
  "sale_date": "2026-08-01",
  "number": "1/08/2026",
  "place": "Warszawa",
  "currency": "EUR",
  "exchange_rate": {"rate": "4.3128", "table": "147/A/NBP/2026", "date": "2026-07-31"},
  "lines": [
    {"name": "Web design services", "quantity": "156,2", "unit": "godz", "net_price": 25, "vat": "np I",
     "pkwiu": "62.10.B"}
  ]
}
```

- `vat`: `23`, `8`, `5`, `"np I"` or `"np II"`. `pkwiu` per line is optional (FA(3) `PKWiU`; the PDF adds it
  after the name unless the name already contains it).
- `currency`: default from the buyer in config, else the config's `currency` (PLN).
- `exchange_rate`: for a currency other than PLN. Leave it out and the job fetches the NBP table A rate of the
  last business day before the sale date from api.nbp.pl.
- `number`: pass it when your numbering follows the sale month (sale in August, issued in September:
  `1/08/2026`); the default counts the issue month.

A correction (faktura korygująca) of an invoice already in KSeF: `lines` are all lines as they should be after
the correction (empty cancels the invoice), `reason` is required:

```json
{
  "action": "generate",
  "corrects": "1/10/2026",
  "reason": "Błędna liczba godzin",
  "lines": [{"name": "Usługi IT", "quantity": 160, "unit": "godz.", "net_price": 100, "vat": 23}]
}
```

The number defaults to `KOR/1/10/2026`; sale date, language, buyer, currency and exchange rate to the
original's (a `buyer` key replaces the buyer and sends the old one as "before"). The totals are the
difference and may be negative; `send` works as for any invoice. One correction per invoice.

All fields are optional except that the invoice needs at least one line, either from `from_last` or from `lines`
(a correction may have none).
`lines` are added after the copied ones. Date, number and due-date defaults work exactly as in the editor.
`dry_run: true` validates and returns a preview without writing anything. Unknown fields are rejected, so a typo
like `"linez"` fails loudly instead of being ignored.

### send

Two steps, so nothing goes out by accident:

```bash
echo '{"action":"send","env":"prod"}' | ksef run -
# {"ok": false, "error": "not sent: confirmation missing or wrong",
#  "summary": "Send to KSeF PROD ... invoice 1/10/2026 ...", "confirm": "1/10/2026"}

echo '{"action":"send","env":"prod","confirm":"1/10/2026"}' | ksef run -
# {"ok": true, "env": "prod", "sent": {"ksef_number": "...", ...}}
```

`confirm` must equal what the first call returned: `yes` on test/demo, the invoice number on prod. A Claude
skill should show the user the `summary` and ask before making the second call.
