# 4. Consistency with your past invoices

Goal: the XML this app sends should look like what your previous invoicing program sent. Same structure, same fixed
fields (VAT rate, payment form, annotations, buyer data). Only the dates, number and amounts should differ.

## Fetch what you issued

Page **KSeF** → last 90 days → **Fetch from KSeF**. This logs in with your prod token (it needs the
"view invoices" permission) and downloads the XML of every invoice you issued in that window, from any program,
to `~/.config/ksef/fetched/prod/<ksef-number>.xml`. Nothing is sent.

## Compare

On the same page, pick your invoice in the selector and press **Compare** next to a downloaded one (or use
**Compare with KSeF** on an invoice's page). The result has up to four parts:

- **Only in the KSeF invoice**: the other program put something there that we don't. Paste the output to Claude and decide
  together whether to add it to `src/xml.rs` (with a `make golden` diff to review).
- **Only in ours**: we send something the other program didn't. Usually harmless, but worth a look.
- **Different values**: often a buyer name or address written differently in config.json. Copy theirs if it was
  right.
- A folded count of per-invoice values that differ as expected (dates, number, amounts).

Compare again after any change until it says "Same structure and same fixed values".

## Sharing the app with a friend

The XML depends only on the app version and the invoice data, so the same version produces the same XML
structure for her too. To make sure:

1. She clones the repo and runs `make test`. The golden test checks that her build produces byte-for-byte the
   same FA(3) XML as yours for the built-in sample, and the schema test checks that it passes the official XSD.
2. `ksef config` with her own seller and buyers.
3. `ksef token --env prod` with **her** token, then `ksef` → **KSeF** → **Fetch from KSeF** to download
   invoices she issued before with her old tool.
4. **New invoice** with her usual lines → save → **Compare** against one of hers. If the structure matches
   her past invoices, sending through this app has the same effect in KSeF as her old tool.
5. Optionally, a dry run on `test` (tutorial 2) before the first real send.

She uses her own Keychain and config, so nothing of yours is shared.
