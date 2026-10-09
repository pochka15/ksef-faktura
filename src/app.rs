//! The operations behind the menu, the subcommands and JSON jobs: generate, validate, send, fetch, compare.

use crate::config::Config;
use crate::draft::{self, Context};
use crate::ids;
use crate::inspect::{self, Comparison};
use crate::invoice::{Invoice, Line, Original, Party, Seller};
use crate::keychain;
use crate::ksef::Env;
use crate::ksef::client::{Client, InvoiceMeta};
use crate::ksef::http::UreqTransport;
use crate::pdf;
use crate::store::{self, SentRecord, Store, Stored};
use crate::validate::{self, Problems};
use crate::xml;
use jiff::civil::Date;
use jiff::{Timestamp, ToSpan};
use std::path::{Path, PathBuf};

pub struct App {
    pub store: Store,
    pub config: Config,
    pub home: PathBuf,
    pub today: Date,
}

#[derive(Debug)]
pub struct Generated {
    pub stored: Stored,
    pub pdf: PathBuf,
    pub warnings: Vec<String>,
}

/// What `send` will do, shown before asking for confirmation.
pub struct SendPlan {
    pub summary: String,
    /// What the user must type (or put in a JSON job's `confirm`) to go ahead.
    pub confirmation: String,
}

impl App {
    pub fn load(home: &Path) -> Result<App, String> {
        let store = Store::locate(home);
        let config = Config::load(&store.config_path())?;
        Ok(App {
            store,
            config,
            home: home.to_path_buf(),
            today: jiff::Zoned::now().date(),
        })
    }

    pub fn new_draft(&self) -> Result<draft::Draft, String> {
        draft::Draft::new(&self.config, self.today, self)
    }

    /// Validates (own rules + XSD), writes the PDF to the output directory and keeps the invoice.
    pub fn generate(&self, mut invoice: Invoice) -> Result<Generated, String> {
        invoice.created_at = now();
        let problems = validate::check(&invoice, self.today);
        if !problems.is_ok() {
            return Err(problems.render());
        }
        let xml = xml::fa3(&invoice);
        let mut warnings = problems.warnings;
        if let Some(w) = xsd_or_warning(&xml)? {
            warnings.push(w);
        }
        let pdf = self.write_pdf(&invoice, None)?;
        let stored = self.store.save(&invoice, &xml)?;
        Ok(Generated {
            stored,
            pdf,
            warnings,
        })
    }

    fn write_pdf(&self, invoice: &Invoice, ksef_number: Option<&str>) -> Result<PathBuf, String> {
        let bytes = pdf::render(invoice, ksef_number)?;
        let path = self.pdf_path(invoice);
        store::write(&path, &bytes)?;
        Ok(path)
    }

    /// `~/Desktop/Faktura 1-09-2026.pdf`
    pub fn pdf_path(&self, invoice: &Invoice) -> PathBuf {
        self.config.output_dir(&self.home).join(format!(
            "{} {}.pdf",
            invoice.language.file_word(),
            invoice.slug()
        ))
    }

    /// The PDF as it should look now: with the KSeF number once accepted by production.
    pub fn pdf(&self, stored: &Stored) -> Result<Vec<u8>, String> {
        let ksef_number = stored.sent(Env::Prod).map(|r| r.ksef_number);
        pdf::render(&stored.invoice, ksef_number.as_deref())
    }

    /// Writes the PDF again if it was moved or deleted from the output directory. Returns its path.
    pub fn ensure_pdf(&self, stored: &Stored) -> Result<PathBuf, String> {
        let path = self.pdf_path(&stored.invoice);
        if !path.exists() {
            store::write(&path, &self.pdf(stored)?)?;
        }
        Ok(path)
    }

    /// Invoice `number` as a correction refers to it: it must be in KSeF (prod's number preferred) and not
    /// a correction itself.
    pub fn original(&self, number: &str) -> Result<(Stored, Original), String> {
        let stored = self.store.find(Some(number))?;
        let inv = &stored.invoice;
        if inv.correction.is_some() {
            return Err(format!(
                "{} is itself a correction; correct the original invoice",
                inv.number
            ));
        }
        let sent = [Env::Prod, Env::Demo, Env::Test]
            .iter()
            .find_map(|env| stored.sent(*env))
            .ok_or_else(|| {
                format!(
                    "{} is not in KSeF yet: edit it instead of correcting it",
                    inv.number
                )
            })?;
        let original = Original {
            number: inv.number.clone(),
            issue_date: inv.issue_date,
            ksef_number: sent.ksef_number,
        };
        Ok((stored, original))
    }

    /// Numbers of the stored corrections of invoice `number`.
    pub fn corrections_of(&self, number: &str) -> Result<Vec<String>, String> {
        Ok(self
            .store
            .list()?
            .into_iter()
            .filter(|s| {
                s.invoice
                    .correction
                    .as_ref()
                    .is_some_and(|c| c.original.number == number)
            })
            .map(|s| s.invoice.number)
            .collect())
    }

    /// Own rules plus the official schema, on the XML exactly as stored.
    pub fn validate(&self, stored: &Stored) -> Result<Problems, String> {
        let mut problems = validate::check(&stored.invoice, self.today);
        if let Err(e) = validate::xsd(&stored.xml()?) {
            problems.errors.push(e);
        }
        Ok(problems)
    }

    /// The XML for `env`: unchanged for prod; with fake parties on the shared test environment. A correction
    /// refers to the original as `env` knows it (its test copy's number and KSeF number on test).
    pub fn xml_for(&self, stored: &Stored, env: Env) -> Result<(String, String), String> {
        let inv = &stored.invoice;
        let reference = self.original_in(inv, env)?;
        let stale = matches!((&reference, &inv.correction), (Some(r), Some(c)) if *r != c.original);
        match env {
            Env::Prod if stale => Err(format!(
                "{} refers to {} by its KSeF number from another environment: open it, Edit and Save to refer to the prod one",
                inv.number,
                inv.correction
                    .as_ref()
                    .map(|c| c.original.number.as_str())
                    .unwrap_or("")
            )),
            Env::Prod | Env::Demo if !stale => Ok((stored.xml()?, inv.number.clone())),
            _ => {
                let mut copy = match env {
                    Env::Test => test_copy(inv, &self.config.context_nip(env)?, now()),
                    _ => inv.clone(),
                };
                if let (Some(r), Some(c)) = (reference, copy.correction.as_mut()) {
                    c.original = r;
                }
                Ok((xml::fa3(&copy), copy.number))
            }
        }
    }

    /// For a correction: the original as KSeF `env` has it (its number there and its KSeF number).
    fn original_in(&self, inv: &Invoice, env: Env) -> Result<Option<Original>, String> {
        let Some(c) = &inv.correction else {
            return Ok(None);
        };
        let Ok(original) = self.store.find(Some(&c.original.number)) else {
            // No longer kept here: only what the correction recorded.
            return Ok(Some(c.original.clone()));
        };
        let sent = original.sent(env).ok_or_else(|| {
            format!(
                "{} corrects {}, which is not in KSeF {env}: send {} there first",
                inv.number, c.original.number, c.original.number
            )
        })?;
        Ok(Some(Original {
            number: sent.sent_number,
            issue_date: c.original.issue_date,
            ksef_number: sent.ksef_number,
        }))
    }

    /// Everything that can be checked offline, and the confirmation the user has to give.
    pub fn plan_send(&self, stored: &Stored, env: Env) -> Result<SendPlan, String> {
        if env != Env::Test
            && let Some(sent) = stored.sent(env)
        {
            return Err(format!(
                "{} was already sent to KSeF {env} as {} on {}",
                stored.invoice.number, sent.ksef_number, sent.sent_at
            ));
        }
        let problems = self.validate(stored)?;
        if !problems.is_ok() {
            return Err(format!("not sending, fix these first:\n{}", problems.render()));
        }
        let nip = self.config.context_nip(env)?;
        let (xml, number) = self.xml_for(stored, env)?;
        validate::xsd(&xml)?;
        let inv = &stored.invoice;
        let gross = inv.totals().gross.display(inv.language);
        let mut summary = vec![
            format!(
                "Send to KSeF {} ({}) as NIP {nip}:",
                env.name().to_uppercase(),
                env.api_url()
            ),
            format!(
                "  invoice {number}, issued {}, buyer {} ({}), {gross} {} gross",
                inv.issue_date,
                inv.buyer.name,
                inv.buyer.id_text(),
                inv.currency
            ),
            "  checks: FA(3) schema OK, NIPs, bank account, totals, dates".to_string(),
        ];
        if let Some(c) = &inv.correction {
            let (verb, amount) = if inv.totals().gross.0 < 0 {
                ("refunds", inv.totals().gross.0.abs())
            } else {
                ("adds", inv.totals().gross.0)
            };
            summary.insert(
                2,
                format!(
                    "  correction of {} (KSeF {}): {}; {verb} {} {}",
                    c.original.number,
                    c.original.ksef_number,
                    c.reason,
                    crate::money::Money(amount).display(inv.language),
                    inv.currency
                ),
            );
        }
        for w in &problems.warnings {
            summary.push(format!("  warning: {w}"));
        }
        let confirmation = match env {
            Env::Test => {
                summary.push("  test environment: seller, buyer and bank replaced with fake data".into());
                "yes".to_string()
            }
            Env::Demo => "yes".to_string(),
            Env::Prod => {
                summary.push("  PRODUCTION: this legally issues the invoice and cannot be undone.".into());
                inv.number.clone()
            }
        };
        Ok(SendPlan {
            summary: summary.join("\n"),
            confirmation,
        })
    }

    /// Call only after the user confirmed `plan_send`. Progress goes to `log`.
    pub fn send(&self, stored: &Stored, env: Env, log: &dyn Fn(&str)) -> Result<SentRecord, String> {
        let token = keychain::token(env)?;
        let nip = self.config.context_nip(env)?;
        let (xml, number) = self.xml_for(stored, env)?;
        let mut client = Client::new(UreqTransport::new(), env);
        log(&format!("logging in to KSeF {env} as {nip}..."));
        client.authenticate(&nip, &token)?;
        log(&format!("sending {number}..."));
        let result = client.send_invoice(xml.as_bytes());
        client.logout();
        let sent = result?;
        let record = self.store.record_sent(stored, env, &sent, &number, now())?;
        if sent.upo.is_none() {
            log("note: KSeF had no UPO ready yet; it is visible in the KSeF web app");
        }
        if env == Env::Prod {
            let pdf = self.write_pdf(&stored.invoice, Some(&sent.ksef_number))?;
            log(&format!("PDF updated with the KSeF number: {}", pdf.display()));
        }
        Ok(record)
    }

    /// Downloads XML of invoices issued in the last `days` days (KSeF allows up to ~90 per query).
    pub fn fetch(&self, env: Env, days: i64) -> Result<Vec<(InvoiceMeta, PathBuf)>, String> {
        let token = keychain::token(env)?;
        let nip = self.config.context_nip(env)?;
        let to = Timestamp::now();
        let from = to
            .checked_sub((days.clamp(1, 90) * 24).hours())
            .map_err(|e| e.to_string())?;
        let mut client = Client::new(UreqTransport::new(), env);
        client.authenticate(&nip, &token)?;
        let result = (|| {
            let list = client.issued_invoices(&from.to_string(), &to.to_string())?;
            let dir = self.store.fetched_dir(env);
            let mut out = Vec::new();
            for meta in list {
                let path = dir.join(format!("{}.xml", meta.ksef_number));
                if !path.exists() {
                    store::write(&path, client.download(&meta.ksef_number)?.as_bytes())?;
                }
                out.push((meta, path));
            }
            Ok(out)
        })();
        client.logout();
        result
    }

    /// Compares one of our invoices (default: latest) with a reference XML (default: latest fetched).
    pub fn compare(
        &self,
        ours: Option<&str>,
        reference: Option<&str>,
    ) -> Result<(String, String, Comparison), String> {
        let (ours_label, ours_xml) = match ours {
            Some(p) if p.ends_with(".xml") => (p.to_string(), store::read(Path::new(p))?),
            other => {
                let stored = self.store.find(other)?;
                (stored.invoice.number.clone(), stored.xml()?)
            }
        };
        let reference = match reference {
            Some(p) => PathBuf::from(p),
            None => self
                .store
                .latest_fetched()
                .ok_or("nothing fetched yet: fetch from KSeF first (KSeF page, or the JSON fetch action), or pass a reference XML")?,
        };
        let comparison = inspect::compare(&ours_xml, &store::read(&reference)?)?;
        Ok((ours_label, reference.display().to_string(), comparison))
    }

    /// Creates `test_nip` if missing. Returns it.
    pub fn ensure_test_nip(&mut self) -> Result<String, String> {
        if let Some(nip) = &self.config.test_nip {
            return Ok(nip.clone());
        }
        let nip = ids::random_nip(rand::random::<u32>);
        self.config.test_nip = Some(nip.clone());
        self.config.save(&self.store.config_path())?;
        Ok(nip)
    }
}

impl Context for App {
    fn next_number(&self, issue_date: Date) -> Result<String, String> {
        self.store.next_number(issue_date)
    }

    fn last_lines(&self) -> Option<(String, Vec<Line>)> {
        let last = self.store.find(None).ok()?;
        Some((last.invoice.number, last.invoice.lines))
    }
}

/// `xmllint` missing is a warning when only generating; schema errors are errors.
fn xsd_or_warning(xml: &str) -> Result<Option<String>, String> {
    match validate::xsd(xml) {
        Ok(()) => Ok(None),
        Err(e) if e.starts_with("xmllint not found") => Ok(Some(e)),
        Err(e) => Err(e),
    }
}

fn now() -> Timestamp {
    Timestamp::from_second(Timestamp::now().as_second()).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// Same invoice shape, but nobody's real data: the test environment is public and shared.
pub fn test_copy(inv: &Invoice, test_nip: &str, at: Timestamp) -> Invoice {
    let fake = |name: &str, nip: &str| Party {
        name: name.into(),
        nip: nip.into(),
        tax_id: None,
        address_line1: "ul. Testowa 1".into(),
        address_line2: Some("00-001 Warszawa".into()),
        country: "PL".into(),
        phone: inv.seller.party.phone.as_ref().map(|_| "500600700".into()),
        email: None,
    };
    let buyer = if inv.buyer.is_polish() {
        Party {
            phone: None,
            ..fake("Nabywca Testowy", "1111111111")
        }
    } else {
        // A foreign buyer stays foreign (same country, same kind of id), so the XML keeps its structure.
        Party {
            nip: String::new(),
            tax_id: inv
                .buyer
                .tax_id
                .as_ref()
                .map(|_| format!("{}123456789", inv.buyer.country)),
            address_line1: "1 Test Street".into(),
            address_line2: Some("00000 Test City".into()),
            country: inv.buyer.country.clone(),
            phone: None,
            ..fake("Test Buyer Ltd", "")
        }
    };
    Invoice {
        // A fresh number each time: KSeF rejects a second invoice with the same number as a duplicate.
        number: format!("{}-T{}", inv.number, at.as_second()),
        seller: Seller {
            party: fake("Sprzedawca Testowy", test_nip),
            // Keep the account's shape (IBAN with prefix or not), not its number.
            bank_account: if ids::compact_account(&inv.seller.bank_account).starts_with("PL") {
                "PL61109010140000071219812874".into()
            } else {
                "61109010140000071219812874".into()
            },
            bank_name: inv.seller.bank_name.as_ref().map(|_| "Bank Testowy".into()),
            swift: inv.seller.swift.as_ref().map(|_| "TESTPLPW".into()),
            issuer_name: None,
        },
        // The buyer before a correction gets the same fake data: the test environment only needs the shape.
        correction: inv.correction.clone().map(|mut c| {
            c.buyer_before = c.buyer_before.map(|_| buyer.clone());
            c
        }),
        buyer,
        ..inv.clone()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::invoice::tests::sample;
    use crate::store::tests::temp_store;

    pub fn app() -> App {
        let store = temp_store();
        let mut config = Config::parse(crate::config::EXAMPLE).unwrap().0;
        let out = store.root().join("out");
        config.output_dir = Some(out.display().to_string());
        config.test_nip = Some("5265877635".into());
        App {
            store,
            config,
            home: PathBuf::from("/nonexistent"),
            today: jiff::civil::date(2026, 10, 8),
        }
    }

    #[test]
    fn generate_writes_pdf_and_keeps_the_invoice() {
        let app = app();
        let g = app.generate(sample()).unwrap();
        assert!(g.pdf.ends_with("Faktura 1-09-2026.pdf"), "{}", g.pdf.display());
        assert!(std::fs::read(&g.pdf).unwrap().starts_with(b"%PDF"));
        let stored = app.store.find(None).unwrap();
        assert_ne!(
            stored.invoice.created_at,
            Timestamp::UNIX_EPOCH,
            "creation time is stamped"
        );
        assert!(app.validate(&stored).unwrap().is_ok());
        assert_eq!(app.last_lines().unwrap().1, sample().lines);
    }

    #[test]
    fn generate_refuses_invalid_invoices() {
        let mut inv = sample();
        inv.buyer.nip = "123".into();
        assert!(app().generate(inv).unwrap_err().contains("buyer NIP"));
    }

    #[test]
    fn send_plan_needs_the_invoice_number_on_prod() {
        let app = app();
        let stored = app.generate(sample()).unwrap().stored;
        let prod = app.plan_send(&stored, Env::Prod).unwrap();
        assert_eq!(prod.confirmation, "1/09/2026");
        assert!(prod.summary.contains("PRODUCTION"));
        let test = app.plan_send(&stored, Env::Test).unwrap();
        assert_eq!(test.confirmation, "yes");
        assert!(test.summary.contains("fake data"));
    }

    fn record(app: &App, number: &str, env: Env, ksef_number: &str, sent_number: &str) {
        let sent = crate::ksef::client::Sent {
            ksef_number: ksef_number.into(),
            session_reference: "S".into(),
            invoice_reference: "I".into(),
            upo: None,
        };
        let stored = app.store.find(Some(number)).unwrap();
        let at = "2026-10-08T12:00:00Z".parse().unwrap();
        app.store
            .record_sent(&stored, env, &sent, sent_number, at)
            .unwrap();
    }

    fn correction_job(reason: &str) -> crate::job::GenerateJob {
        crate::job::GenerateJob {
            corrects: Some("1/09/2026".into()),
            reason: Some(reason.into()),
            issue_date: Some(jiff::civil::date(2026, 10, 8)),
            lines: vec![crate::invoice::tests::line(
                "Usługi IT",
                "160",
                "100",
                crate::money::Vat::R23,
            )],
            ..Default::default()
        }
    }

    #[test]
    fn corrections_refer_to_the_original_in_each_environment() {
        let app = app();
        app.generate(sample()).unwrap();
        let e = crate::job::build(&app, correction_job("x")).unwrap_err();
        assert!(e.contains("not in KSeF yet"), "{e}");

        const TEST_K: &str = "5265877635-20261008-0102030405A1-B2";
        const PROD_K: &str = "5265877635-20260930-0102030405A1-C3";
        record(&app, "1/09/2026", Env::Test, TEST_K, "1/09/2026-T99");
        let draft = crate::job::build(&app, correction_job("Błędna liczba godzin")).unwrap();
        let inv = draft.invoice;
        assert_eq!(inv.number, "KOR/1/09/2026");
        assert_eq!(inv.sale_date, sample().sale_date, "the original's sale date");
        assert_eq!(
            inv.buyer,
            sample().buyer,
            "the original's buyer, not the config's"
        );
        let c = inv.correction.as_ref().unwrap();
        assert_eq!(
            (c.original.ksef_number.as_str(), c.before.clone()),
            (TEST_K, sample().lines)
        );
        assert!(c.buyer_before.is_none());
        let kor = app.generate(inv).unwrap().stored;

        let (xml, number) = app.xml_for(&kor, Env::Test).unwrap();
        assert!(number.starts_with("KOR/1/09/2026-T"));
        assert!(
            xml.contains("<NrFaKorygowanej>1/09/2026-T99</NrFaKorygowanej>"),
            "{xml}"
        );
        assert!(xml.contains(&format!("<NrKSeFFaKorygowanej>{TEST_K}<")));
        assert!(
            app.plan_send(&kor, Env::Test)
                .unwrap()
                .summary
                .contains("refunds 984,00 PLN")
        );
        let e = app.xml_for(&kor, Env::Demo).unwrap_err();
        assert!(e.contains("not in KSeF demo"), "{e}");

        // The original reaches prod: the correction must be saved again to carry the prod number.
        record(&app, "1/09/2026", Env::Prod, PROD_K, "1/09/2026");
        assert!(
            app.xml_for(&kor, Env::Prod)
                .unwrap_err()
                .contains("Edit and Save")
        );
        let kor = app
            .generate(
                crate::job::build(&app, correction_job("Błędna liczba godzin"))
                    .unwrap()
                    .invoice,
            )
            .unwrap()
            .stored;
        let (xml, number) = app.xml_for(&kor, Env::Prod).unwrap();
        assert_eq!((xml, number), (kor.xml().unwrap(), "KOR/1/09/2026".to_string()));
        assert!(kor.xml().unwrap().contains(PROD_K));

        let e = crate::job::build(
            &app,
            crate::job::GenerateJob {
                number: Some("KOR2/1/09/2026".into()),
                ..correction_job("again")
            },
        )
        .unwrap_err();
        assert!(e.contains("already corrected by KOR/1/09/2026"), "{e}");
        assert!(
            app.original("KOR/1/09/2026")
                .unwrap_err()
                .contains("itself a correction")
        );
    }

    #[test]
    fn test_copies_keep_the_shape_but_not_the_people() {
        let at = "2026-10-08T12:00:00Z".parse().unwrap();
        let copy = test_copy(&sample(), "5265877635", at);
        assert_eq!(copy.number, format!("1/09/2026-T{}", at.as_second()));
        assert_eq!(copy.seller.party.nip, "5265877635");
        assert!(!copy.buyer.name.contains("Klient"));
        assert_eq!(copy.lines, sample().lines);
        let c = inspect::compare(&xml::fa3(&copy), &xml::fa3(&sample())).unwrap();
        assert!(c.missing.is_empty() && c.extra.is_empty(), "{}", c.render());
        assert!(validate::check(&copy, jiff::civil::date(2026, 10, 8)).is_ok());
    }

    #[test]
    fn test_copies_of_foreign_invoices_stay_foreign() {
        let original = crate::invoice::tests::sample_foreign();
        let copy = test_copy(&original, "5265877635", "2026-10-08T12:00:00Z".parse().unwrap());
        assert_eq!(copy.buyer.country, "GB");
        assert_eq!(copy.buyer.tax_id.as_deref(), Some("GB123456789"));
        assert!(copy.seller.bank_account.starts_with("PL"));
        assert_eq!(copy.currency, "EUR");
        let c = inspect::compare(&xml::fa3(&copy), &xml::fa3(&original)).unwrap();
        assert!(c.missing.is_empty() && c.extra.is_empty(), "{}", c.render());
    }
}
