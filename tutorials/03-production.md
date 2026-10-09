# 3. Production

## Token

1. Log in at https://ap.ksef.mf.gov.pl with Profil Zaufany (login.gov.pl), a qualified signature or e-dowód,
   in the context of your NIP.
2. Generate a token with **issue (wystawianie)** and **view (przeglądanie)** invoice permissions. View is what
   fetching (tutorial 4) needs.
3. Press `t` in the terminal where `ksef` runs and paste the token (twice, hidden). Without `--test-envs` it is
   stored for production without asking.

Token login stays supported with no end date: in August 2026 the Ministry dropped the planned end-of-2026 cut-off.
If a token leaks, revoke it in the web app and generate a new one.

## First real send

Do tutorial 4 first: fetch the invoices your previous invoicing program sent and compare, so you know your XML has the same
shape as what you sent before.

Open the invoice → **Send to KSeF** → **Review send to KSeF**. The checks run again (own rules and
the FA(3) schema) before the summary appears.

On `prod` you confirm by typing the **invoice number**, not `yes`. After acceptance:

- `~/.config/ksef/invoices/<n>/sent-prod.json` has the KSeF number, and `upo-prod.xml` is the official receipt.
- The PDF on your Desktop is re-rendered with the KSeF number printed under the header.
- The invoice is locked: **Edit** is disabled and saving over that number is refused. Mistakes are fixed with
  a correction invoice, below.

## Corrections (faktura korygująca)

An invoice in KSeF can't be changed or deleted. If it was wrong (hours, price, VAT rate, the buyer's data), open
it and press **Correct**:

1. The editor opens with the original's lines. Change them to what they should have been (remove a line to
   take it back; remove all to cancel the invoice), and write the **reason**, e.g. "Błędna liczba godzin".
2. The number defaults to `KOR/<original number>`; dates follow today; the sale date, buyer, currency and
   exchange rate stay the original's. Pick a buyer from config only if the buyer's data was wrong; the old
   data is then sent as "before" (FA(3) `Podmiot2K`).
3. **Save and write PDF**: "Faktura korygująca" with the original's number, date and KSeF number, the
   reason, the lines before and after, and the difference: **Do zapłaty** if it went up, **Do zwrotu** (no
   payment terms) if it went down.
4. Send it like any invoice; the review says what it corrects and how much it adds or refunds.

In the XML it is `RodzajFaktury` `KOR` with `PrzyczynaKorekty`, `DaneFaKorygowanej` (the original's date,
number and KSeF number), the original rows marked `StanPrzed` and then the corrected rows; the sums are the
differences. Not covered: a second correction of the same invoice, correcting a correction, the seller's own
data, advance invoices, discounts for a period, and `TypKorekty` (ask your accountant if it matters; it is
optional). On test (with `--test-envs`), send the original to test first: the correction then refers to
its test copy.

## The monthly routine

`ksef` → **New invoice** → **Copy lines from …** → adjust the hours → check the preview → **Save and write PDF**
→ **Review send to KSeF** → type the invoice number → **Send to KSeF**.

Before that, spend five minutes on [tutorial 6](06-when-ksef-changes.md).
