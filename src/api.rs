//! What the browser page can ask for, as plain functions: JSON in, JSON (or PDF bytes) out. `server` maps
//! URLs onto these; everything here works on a freshly loaded `App`, so config edits apply immediately.

use crate::app::App;
use crate::i18n::Lang;
use crate::inspect;
use crate::invoice::{ExchangeRate, Invoice, Line};
use crate::job::{self, GenerateJob};
use crate::ksef::Env;
use crate::ksef::http::UreqTransport;
use crate::money::{Money, Quantity, Rate, Vat};
use crate::nbp;
use crate::store;
use crate::validate;
use crate::words;
use jiff::civil::Date;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

/// The invoice form. Empty fields are `null` and get the usual defaults; amounts are typed text.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    pub buyer: Option<String>,
    pub language: Option<Lang>,
    pub number: Option<String>,
    pub issue_date: Option<Date>,
    pub sale_date: Option<Date>,
    pub due_date: Option<Date>,
    pub place: Option<String>,
    pub currency: Option<String>,
    /// Typed rate fields; ignored for PLN.
    pub exchange_rate: Option<FormRate>,
    #[serde(default)]
    pub lines: Vec<FormLine>,
    /// Number of the invoice being edited; saving under any other existing number is refused.
    pub replaces: Option<String>,
    /// Makes this a correction of that invoice (see `GenerateJob::corrects`); `buyer` null = the original's.
    pub corrects: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FormLine {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub quantity: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub net_price: String,
    #[serde(default)]
    pub vat: String,
    #[serde(default)]
    pub pkwiu: Option<String>,
}

/// The exchange rate as typed: `4,3128`, `147/A/NBP/2026`, `2026-07-31`.
#[derive(Debug, Default, Deserialize)]
pub struct FormRate {
    #[serde(default)]
    pub rate: String,
    #[serde(default)]
    pub table: String,
    #[serde(default)]
    pub date: String,
}

impl FormRate {
    fn parse(&self) -> Result<Option<ExchangeRate>, String> {
        if [&self.rate, &self.table, &self.date]
            .iter()
            .all(|s| s.trim().is_empty())
        {
            return Ok(None);
        }
        Ok(Some(ExchangeRate {
            rate: Rate::parse(&self.rate)?,
            table: self.table.trim().to_string(),
            date: self
                .date
                .trim()
                .parse()
                .map_err(|_| format!("exchange rate date '{}' is not YYYY-MM-DD", self.date.trim()))?,
        }))
    }
}

impl FormLine {
    fn is_blank(&self) -> bool {
        [&self.name, &self.quantity, &self.unit, &self.net_price]
            .iter()
            .all(|s| s.trim().is_empty())
    }

    fn parse(&self) -> Result<Line, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("no name".into());
        }
        let unit = self.unit.trim();
        if unit.is_empty() {
            return Err("no unit".into());
        }
        Ok(Line {
            name: name.to_string(),
            quantity: Quantity::parse(&self.quantity).map_err(|e| format!("quantity: {e}"))?,
            unit: unit.to_string(),
            net_price: Money::parse(&self.net_price).map_err(|e| format!("price: {e}"))?,
            vat: Vat::parse(&self.vat).map_err(|e| format!("VAT: {e}"))?,
            pkwiu: self
                .pkwiu
                .as_ref()
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty()),
        })
    }
}

/// Parses the lines (blank rows are skipped) and fills defaults like a JSON job would.
/// Line problems are returned separately so the rest of the form still previews.
fn resolve(app: &App, form: Form) -> Result<(Invoice, Vec<String>), String> {
    let mut lines = Vec::new();
    let mut line_errors = Vec::new();
    for (i, l) in form.lines.iter().enumerate().filter(|(_, l)| !l.is_blank()) {
        match l.parse() {
            Ok(line) => lines.push(line),
            Err(e) => line_errors.push(format!("line {}: {e}", i + 1)),
        }
    }
    let text = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let exchange_rate = match form.exchange_rate.as_ref().map(FormRate::parse).transpose() {
        Ok(rate) => rate.flatten(),
        Err(e) => {
            line_errors.push(e);
            None
        }
    };
    let draft = job::build(
        app,
        GenerateJob {
            buyer: text(form.buyer),
            language: form.language,
            number: text(form.number),
            issue_date: form.issue_date,
            sale_date: form.sale_date,
            due_date: form.due_date,
            place: text(form.place),
            currency: text(form.currency),
            exchange_rate,
            lines,
            corrects: text(form.corrects),
            reason: form.reason,
            ..GenerateJob::default()
        },
    )?;
    Ok((draft.invoice, line_errors))
}

/// Everything the page shows around the editor: who we are, buyers, invoices, fetched XML.
/// `envs`: the KSeF environments the page offers (only prod unless started with `--test-envs`).
pub fn state(app: &App, envs: &[Env], has_token: &dyn Fn(Env) -> bool) -> Result<Value, String> {
    let c = &app.config;
    let buyers: Vec<Value> = c
        .buyers
        .iter()
        .map(|(key, b)| {
            json!({"key": key, "name": b.party.name, "nip": b.party.id_text(), "currency": c.currency_for(key)})
        })
        .collect();
    let invoices: Vec<Value> = app.store.list()?.iter().rev().map(job::summary).collect();
    let tokens: serde_json::Map<String, Value> = envs
        .iter()
        .map(|env| (env.name().to_string(), json!(has_token(*env))))
        .collect();
    Ok(json!({
        "seller": {"name": c.seller.party.name, "nip": c.seller.party.nip},
        "buyers": buyers,
        "default_buyer": c.buyer(None).ok().map(|(key, _)| key),
        "language": c.language,
        "test_nip": c.test_nip,
        "envs": envs,
        "tokens": tokens,
        "config_path": app.store.config_path().display().to_string(),
        "output_dir": c.output_dir(&app.home).display().to_string(),
        "config_problems": c.problems(),
        "today": app.today,
        "invoices": invoices,
        "fetched": fetched(app),
    }))
}

/// Live preview of the form: resolved fields, totals as printed, and what would block saving.
pub fn draft(app: &App, form: Form) -> Result<Value, String> {
    // Per form row (blank and broken rows included), so the page can show each row's net amount.
    let rows: Vec<Option<Line>> = form
        .lines
        .iter()
        .map(|l| if l.is_blank() { None } else { l.parse().ok() })
        .collect();
    let (invoice, line_errors) = resolve(app, form)?;
    let lang = invoice.language;
    let p = validate::check(&invoice, app.today);
    let mut errors = line_errors;
    errors.extend(p.errors);
    let rows: Vec<Option<String>> = rows
        .iter()
        .map(|l| l.as_ref().map(|l| l.net().display(lang)))
        .collect();
    let last = app.store.find(None).ok().map(
        |s| json!({"number": s.invoice.number, "lines": s.invoice.lines, "buyer": s.invoice.buyer.name}),
    );
    Ok(json!({
        "invoice": invoice,
        "rows": rows,
        "totals": totals_json(&invoice),
        "errors": errors,
        "warnings": p.warnings,
        "pdf_name": app.pdf_path(&invoice).file_name().map(|n| n.to_string_lossy().into_owned()),
        "exists": app.store.find(Some(&invoice.number)).is_ok(),
        "last": last,
    }))
}

pub fn draft_pdf(app: &App, form: Form) -> Result<Vec<u8>, String> {
    let (invoice, _) = resolve(app, form)?;
    crate::pdf::render(&invoice, None)
}

/// Totals as printed, in the invoice currency; for a foreign currency also the VAT in PLN. For a correction
/// these are the differences; `refund` when the buyer gets money back.
fn totals_json(inv: &Invoice) -> Value {
    let lang = inv.language;
    let t = inv.totals();
    let rate = inv.exchange_rate.as_ref().filter(|_| inv.currency != "PLN");
    json!({
        "net": t.net.display(lang), "vat": t.tax.display(lang), "gross": t.gross.display(lang),
        "currency": inv.currency,
        "in_words": words::amount(Money(t.gross.0.abs()), &inv.currency, lang),
        "vat_pln": rate.map(|r| r.rate.to_pln(t.tax).display(lang)),
        "correction": inv.correction.is_some(),
        "refund": t.gross.0 < 0,
        "due": Money(t.gross.0.abs()).display(lang),
    })
}

fn lines_json(lines: &[Line], lang: Lang) -> Vec<Value> {
    lines
        .iter()
        .map(|l| {
            json!({
                "name": l.name, "quantity": l.quantity.display(lang), "unit": l.unit,
                "net_price": l.net_price.display(lang), "vat": l.vat.label(), "net": l.net().display(lang),
            })
        })
        .collect()
}

/// The NBP rate for `currency` from the last business day before `sale_date` (a public, read-only lookup).
pub fn nbp_rate(currency: &str, sale_date: Date) -> Result<Value, String> {
    let r = nbp::rate_before(&UreqTransport::new(), currency, sale_date)?;
    Ok(json!({"rate": r.rate.display(Lang::Pl), "table": r.table, "date": r.date}))
}

/// Validates, writes the PDF and keeps the invoice (same as `generate` in JSON mode).
pub fn save(app: &App, form: Form) -> Result<Value, String> {
    let replaces = form.replaces.clone();
    let (invoice, line_errors) = resolve(app, form)?;
    if !line_errors.is_empty() {
        return Err(line_errors.join("\n"));
    }
    if replaces.as_deref() != Some(invoice.number.as_str()) && app.store.find(Some(&invoice.number)).is_ok() {
        return Err(format!(
            "invoice {} already exists: open it and use Edit, or pick another number",
            invoice.number
        ));
    }
    let generated = app.generate(invoice)?;
    Ok(json!({
        "pdf": generated.pdf.display().to_string(),
        "warnings": generated.warnings,
        "invoice": job::summary(&generated.stored),
    }))
}

/// One invoice with its checks and where it was sent.
pub fn invoice(app: &App, number: &str) -> Result<Value, String> {
    let stored = app.store.find(Some(number))?;
    let inv = &stored.invoice;
    let lang = inv.language;
    let problems = app.validate(&stored)?;
    let lines = lines_json(&inv.lines, lang);
    let correction = inv.correction.as_ref().map(|c| {
        json!({
            "reason": c.reason, "original": c.original,
            "original_slug": crate::invoice::slug(&c.original.number),
            "before": lines_json(&c.before, lang),
            "before_gross": crate::invoice::Totals::of(&c.before).gross.display(lang),
            "after_gross": crate::invoice::Totals::of(&inv.lines).gross.display(lang),
            "buyer_before": c.buyer_before.as_ref().map(|b| &b.name),
        })
    });
    let corrections: Vec<Value> = app
        .corrections_of(&inv.number)?
        .iter()
        .map(|n| json!({"number": n, "slug": crate::invoice::slug(n)}))
        .collect();
    // Why "Correct" is not offered, if it is not.
    let cannot_correct = app.original(&inv.number).err().or_else(|| {
        corrections
            .first()
            .map(|c| format!("already corrected by {}", c["number"].as_str().unwrap_or("")))
    });
    let sent: serde_json::Map<String, Value> = Env::ALL
        .iter()
        .filter_map(|env| stored.sent(*env).map(|r| (env.name().to_string(), json!(r))))
        .collect();
    let locked = [Env::Prod, Env::Demo]
        .iter()
        .any(|env| stored.sent(*env).is_some());
    Ok(json!({
        "invoice": inv,
        "summary": job::summary(&stored),
        "lines": lines,
        "totals": totals_json(inv),
        "errors": problems.errors,
        "warnings": problems.warnings,
        "sent": sent,
        "dir": stored.dir.display().to_string(),
        "pdf": app.pdf_path(inv).display().to_string(),
        "locked": locked,
        "correction": correction,
        "corrections": corrections,
        "cannot_correct": cannot_correct,
    }))
}

pub fn invoice_pdf(app: &App, number: &str) -> Result<Vec<u8>, String> {
    app.pdf(&app.store.find(Some(number))?)
}

/// Opens the PDF in the default viewer (Preview), writing it again if it was moved away.
pub fn open_pdf(app: &App, number: &str) -> Result<Value, String> {
    let path = app.ensure_pdf(&app.store.find(Some(number))?)?;
    std::process::Command::new("open")
        .arg(&path)
        .status()
        .map_err(|e| format!("open: {e}"))?;
    Ok(json!({"pdf": path.display().to_string()}))
}

/// What a send would do and the exact text the user has to type to confirm it.
pub fn plan(app: &App, number: &str, env: Env) -> Result<Value, String> {
    let plan = app.plan_send(&app.store.find(Some(number))?, env)?;
    Ok(json!({"summary": plan.summary, "confirm": plan.confirmation}))
}

/// Sends only when `confirm` matches the plan, exactly like the JSON `send` action.
pub fn send(app: &App, number: &str, env: Env, confirm: &str) -> Result<Value, String> {
    let stored = app.store.find(Some(number))?;
    let plan = app.plan_send(&stored, env)?;
    if confirm.trim() != plan.confirmation {
        return Err(format!("not sent: type '{}' to confirm", plan.confirmation));
    }
    let log = std::cell::RefCell::new(Vec::new());
    let result = app.send(&stored, env, &|msg| log.borrow_mut().push(msg.to_string()));
    let log = log.into_inner();
    match result {
        Ok(record) => Ok(json!({"sent": record, "log": log})),
        Err(e) if log.is_empty() => Err(e),
        Err(e) => Err(format!("{}\n{e}", log.join("\n"))),
    }
}

/// Logs in and downloads the XML of invoices issued in the last `days` days.
pub fn fetch(app: &App, env: Env, days: i64) -> Result<Value, String> {
    let list = app.fetch(env, days)?;
    Ok(json!({"count": list.len(), "fetched": fetched(app)}))
}

/// Downloaded invoices, newest issue date first, with the fields people recognise them by.
fn fetched(app: &App) -> Vec<Value> {
    let mut out: Vec<Value> = Env::ALL
        .iter()
        .filter_map(|env| Some((*env, std::fs::read_dir(app.store.fetched_dir(*env)).ok()?)))
        .flat_map(|(env, entries)| entries.flatten().map(move |e| (env, e.path())))
        .filter(|(_, path)| path.extension().is_some_and(|x| x == "xml"))
        .filter_map(|(env, path)| {
            let leaves = inspect::leaves(&store::read(&path).ok()?).ok()?;
            let get = |p: &str| leaves.iter().find(|(k, _)| k == p).map(|(_, v)| v.clone());
            Some(json!({
                "env": env,
                "ksef_number": path.file_stem()?.to_string_lossy(),
                "number": get("Faktura/Fa/P_2"),
                "issue_date": get("Faktura/Fa/P_1"),
                "buyer": get("Faktura/Podmiot2/DaneIdentyfikacyjne/Nazwa"),
                "gross": get("Faktura/Fa/P_15"),
            }))
        })
        .collect();
    out.sort_by(|a, b| b["issue_date"].as_str().cmp(&a["issue_date"].as_str()));
    out
}

/// A downloaded XML by environment and KSeF number; nothing outside `fetched/` can be named.
fn fetched_path(app: &App, env: Env, ksef_number: &str) -> Result<PathBuf, String> {
    if ksef_number.is_empty() || !ksef_number.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("not a KSeF number: '{ksef_number}'"));
    }
    let path = app.store.fetched_dir(env).join(format!("{ksef_number}.xml"));
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("{ksef_number} has not been fetched from KSeF {env}"))
    }
}

pub fn compare(app: &App, number: &str, env: Env, ksef_number: &str) -> Result<Value, String> {
    let reference = fetched_path(app, env, ksef_number)?;
    let ours = app.store.find(Some(number))?;
    let c = inspect::compare(&ours.xml()?, &store::read(&reference)?)?;
    let triples = |v: &[(String, String, String)]| -> Vec<Value> {
        v.iter()
            .map(|(path, ours, theirs)| json!({"path": path, "ours": ours, "reference": theirs}))
            .collect()
    };
    Ok(json!({
        "ours": ours.invoice.number,
        "reference": ksef_number,
        "consistent": c.is_consistent(),
        "missing": c.missing,
        "extra": c.extra,
        "different": triples(&c.different),
        "varying": triples(&c.varying),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::app;

    fn line(name: &str, qty: &str, price: &str) -> FormLine {
        FormLine {
            name: name.into(),
            quantity: qty.into(),
            unit: "godz.".into(),
            net_price: price.into(),
            vat: "23".into(),
            pkwiu: None,
        }
    }

    fn form() -> Form {
        Form {
            issue_date: Some(jiff::civil::date(2026, 9, 30)),
            lines: vec![line("Usługi IT", "168", "100,00"), FormLine::default()],
            ..Form::default()
        }
    }

    #[test]
    fn draft_fills_defaults_and_formats_totals() {
        let app = app();
        let d = draft(&app, form()).unwrap();
        assert_eq!(d["invoice"]["number"], "1/09/2026");
        assert_eq!(d["invoice"]["due_date"], "2026-10-07");
        assert_eq!(
            d["invoice"]["lines"].as_array().unwrap().len(),
            1,
            "blank row skipped"
        );
        assert_eq!(d["totals"]["gross"], "20\u{a0}664,00");
        assert_eq!(d["rows"], json!(["16\u{a0}800,00", null]));
        assert_eq!(d["errors"], json!([]));
        assert_eq!(d["pdf_name"], "Faktura 1-09-2026.pdf");
        assert_eq!(d["last"], Value::Null);
    }

    #[test]
    fn bad_lines_are_reported_by_row_without_breaking_the_preview() {
        let mut f = form();
        f.lines.push(line("Licencja", "x", "100"));
        let d = draft(&app(), f).unwrap();
        assert_eq!(d["invoice"]["lines"].as_array().unwrap().len(), 1);
        assert!(
            d["errors"][0].as_str().unwrap().starts_with("line 3: quantity"),
            "{d}"
        );
        let mut f = form();
        f.lines.push(line("Licencja", "x", "100"));
        assert!(save(&app(), f).unwrap_err().contains("line 3"));
    }

    #[test]
    fn save_then_view_plan_and_refuse_wrong_confirmation() {
        let app = app();
        let saved = save(&app, form()).unwrap();
        assert_eq!(saved["invoice"]["number"], "1/09/2026");
        assert!(draft_pdf(&app, form()).unwrap().starts_with(b"%PDF"));

        let one = invoice(&app, "1/09/2026").unwrap();
        assert_eq!(one["errors"], json!([]));
        assert_eq!(one["locked"], false);
        assert!(invoice_pdf(&app, "1-09-2026").unwrap().starts_with(b"%PDF"));

        let p = plan(&app, "1/09/2026", Env::Prod).unwrap();
        assert_eq!(p["confirm"], "1/09/2026");
        let e = send(&app, "1/09/2026", Env::Prod, "yes").unwrap_err();
        assert!(e.contains("not sent"), "{e}");

        let s = state(&app, &[Env::Prod], &|_| false).unwrap();
        assert_eq!(s["invoices"][0]["slug"], "1-09-2026");
        assert_eq!(s["envs"], json!(["prod"]));
        assert_eq!(
            s["tokens"],
            json!({"prod": false}),
            "only offered environments are checked"
        );
    }

    #[test]
    fn saving_over_another_invoice_needs_the_edit_flow() {
        let app = app();
        save(&app, form()).unwrap();
        let mut again = form();
        again.number = Some("1/09/2026".into());
        assert!(save(&app, again).unwrap_err().contains("already exists"));
        let mut edit = form();
        edit.number = Some("1/09/2026".into());
        edit.replaces = Some("1/09/2026".into());
        assert!(save(&app, edit).is_ok());
        let d = draft(&app, form()).unwrap();
        assert_eq!(d["invoice"]["number"], "2/09/2026", "next free number");
        assert_eq!(d["last"]["number"], "1/09/2026");
    }

    #[test]
    fn compare_reads_only_fetched_files() {
        let app = app();
        save(&app, form()).unwrap();
        assert!(compare(&app, "1/09/2026", Env::Prod, "../../config").is_err());
        assert!(
            compare(&app, "1/09/2026", Env::Prod, "nope")
                .unwrap_err()
                .contains("not been fetched")
        );
        let ours = app.store.find(None).unwrap().xml().unwrap();
        store::write(
            &app.store.fetched_dir(Env::Prod).join("5265877635-X.xml"),
            ours.as_bytes(),
        )
        .unwrap();
        let c = compare(&app, "1/09/2026", Env::Prod, "5265877635-X").unwrap();
        assert_eq!(c["consistent"], true);
        let s = state(&app, &Env::ALL, &|_| true).unwrap();
        assert_eq!(s["fetched"][0]["number"], "1/09/2026");
        assert_eq!(s["fetched"][0]["env"], "prod");
    }
}
