# 2. The KSeF test environment

KSeF has three environments, each with its own tokens:

| env | web app | login | data |
|---|---|---|---|
| `test` | https://ap-test.ksef.mf.gov.pl | "test authentication" with any NIP, no real ID | public and shared with everyone, wiped regularly |
| `demo` | https://ap-demo.ksef.mf.gov.pl | your real login (Profil Zaufany / qualified signature) | your own, no legal effect |
| `prod` | https://ap.ksef.mf.gov.pl | your real login | legally issued invoices |

The test environment is shared with every developer in Poland, so **never put real data there**. When you send to
`test`, this app swaps the seller, buyer and bank account for fake values, adds a unique suffix to the number (KSeF
rejects duplicate numbers) and logs in with a fake NIP. The lines, amounts, dates and XML structure stay yours.

By default the app only offers production. Start it with the test and demo environments switched on:

```bash
ksef --test-envs
```

The page then has a test / demo / prod switch wherever it talks to KSeF, with nothing preselected, and the
header shows which environments have a token. Without the flag, the page talks only to production.

## Steps

In the terminal where `ksef --test-envs` runs press `t`, then answer `test` (or run `ksef token --env test`).

1. This creates your fake test NIP (saved as `test_nip` in config.json, shown again with `ksef test-nip`).
2. Open https://ap-test.ksef.mf.gov.pl → **Uwierzytelnij się w Krajowym Systemie e-Faktur** →
   **Zaloguj uwierzytelnieniem testowym**.
3. Context: **NIP firmy** = your test NIP. "Czy Twój certyfikat zawiera numer PESEL lub NIP" = **Tak**.
4. "Podpisz testowe żądanie autoryzacyjne": type **Pieczęć NIP**, number = the same test NIP → **Uwierzytelnij do
   aplikacji testowej**.
5. In the app, open the **tokens** section and generate a token with permissions to **issue (wystawianie)** and
   **view (przeglądanie)** invoices. Copy it. It is shown once.
6. Back in the terminal, paste it twice when `security` asks. It goes into the macOS Keychain (service `ksef-cli`,
   account `test`). You can view or delete it in Keychain Access.

## Send

Open an invoice (**Invoices** → click it) → **Send to KSeF** → **test** → **Review send to test**. The page shows
what will be sent; type `yes` and press **Send to test**. The app logs in, opens a session, sends the encrypted
XML, waits for the KSeF number, closes the session and saves the UPO (official receipt) next to the invoice
(`~/.config/ksef/invoices/<n>/sent-test.json`, `upo-test.xml`). It takes up to a minute. You can also see the
invoice in the test web app. Sending to test again is fine; each send gets a new number suffix.

Common answers from KSeF:

- `450 ... błędnego tokenu`: the token is wrong or was revoked. Store a new one with `t` in the terminal.
- `415 ... Brak przypisanych uprawnień`: the token lacks the invoice permissions. Generate a new one.
- `440 Duplikat faktury`: should not happen on `test` because of the suffix. On demo/prod, that number was already sent.
- `430/450 Błąd weryfikacji`: KSeF disagrees with the XML. The message names the field. Tell Claude.
- Maintenance window: the test environments may be down 16:00–18:00.

## Demo (optional dress rehearsal)

`demo` behaves like production (your real NIP and real login) but has no legal effect. If you want one last check
before the first real send, store a demo token (`t`, then `demo`; log in at https://ap-demo.ksef.mf.gov.pl),
then send with **demo** selected.
