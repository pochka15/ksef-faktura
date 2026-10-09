//! `ksef run job.json`: one JSON action in, one JSON result out (stdout). Progress goes to stderr.
//! This is the interface for scripts and the Claude skill; see `tutorials/05-json-mode.md`.

use crate::app::App;
use crate::draft::{Draft, Step};
use crate::i18n::Lang;
use crate::invoice::{Correction, ExchangeRate, Line};
use crate::ksef::Env;
use crate::ksef::http::UreqTransport;
use crate::nbp;
use crate::store::Stored;
use jiff::civil::Date;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Job {
    /// Builds an invoice like the REPL does; `dry_run` only validates and previews.
    Generate(GenerateJob),
    List,
    Show {
        #[serde(default)]
        invoice: Option<String>,
    },
    Validate {
        #[serde(default)]
        invoice: Option<String>,
    },
    /// Without the right `confirm`, nothing is sent: the result carries the summary and what to confirm with.
    /// `env` is required: there is no default environment.
    Send {
        #[serde(default)]
        invoice: Option<String>,
        env: Env,
        #[serde(default)]
        confirm: Option<String>,
    },
    Fetch {
        env: Env,
        #[serde(default)]
        days: Option<i64>,
    },
    Compare {
        #[serde(default)]
        invoice: Option<String>,
        #[serde(default)]
        reference: Option<String>,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateJob {
    pub buyer: Option<String>,
    pub language: Option<Lang>,
    pub number: Option<String>,
    pub issue_date: Option<Date>,
    pub sale_date: Option<Date>,
    pub due_date: Option<Date>,
    pub place: Option<String>,
    /// e.g. `EUR`; default: the buyer's currency from config, else the config's `currency` (PLN).
    pub currency: Option<String>,
    /// For a foreign currency: `{"rate": "4.3128", "table": "147/A/NBP/2026", "date": "2026-07-31"}`.
    /// Left out, it is fetched from NBP (last business day before the sale date).
    pub exchange_rate: Option<ExchangeRate>,
    /// Number of an invoice already in KSeF: makes this a correcting invoice (KOR) of it. `lines` are then all
    /// lines as they should be after the correction; number, sale date, language, buyer, currency and rate
    /// default to (and currency and rate stay) the original's. Default number: `KOR/<original number>`.
    pub corrects: Option<String>,
    /// Why the invoice is corrected (required with `corrects`), e.g. "błędna liczba godzin".
    pub reason: Option<String>,
    /// Copy the lines of the previous invoice (then `lines`, if given, are added after them).
    #[serde(default)]
    pub from_last: bool,
    #[serde(default)]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub dry_run: bool,
}

pub fn parse(text: &str) -> Result<Job, String> {
    serde_json::from_str(text).map_err(|e| format!("job JSON: {e}"))
}

/// Never fails: errors become `{"ok": false, "error": ...}`.
pub fn run(app: &App, job: Job) -> Value {
    match execute(app, job) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

fn execute(app: &App, job: Job) -> Result<Value, String> {
    match job {
        Job::Generate(g) => generate(app, g),
        Job::List => {
            let invoices: Vec<Value> = app.store.list()?.iter().map(summary).collect();
            Ok(json!({"ok": true, "invoices": invoices}))
        }
        Job::Show { invoice } => {
            let stored = app.store.find(invoice.as_deref())?;
            Ok(json!({"ok": true, "invoice": stored.invoice, "summary": summary(&stored)}))
        }
        Job::Validate { invoice } => {
            let p = app.validate(&app.store.find(invoice.as_deref())?)?;
            Ok(json!({"ok": p.is_ok(), "errors": p.errors, "warnings": p.warnings}))
        }
        Job::Send {
            invoice,
            env,
            confirm,
        } => {
            let stored = app.store.find(invoice.as_deref())?;
            let plan = app.plan_send(&stored, env)?;
            if confirm.as_deref().map(str::trim) != Some(plan.confirmation.as_str()) {
                return Ok(json!({
                    "ok": false,
                    "error": "not sent: confirmation missing or wrong",
                    "summary": plan.summary,
                    "confirm": plan.confirmation,
                }));
            }
            let record = app.send(&stored, env, &|msg| eprintln!("{msg}"))?;
            Ok(json!({"ok": true, "env": env, "sent": record}))
        }
        Job::Fetch { env, days } => {
            let fetched = app.fetch(env, days.unwrap_or(90))?;
            let invoices: Vec<Value> = fetched
                .iter()
                .map(|(m, path)| {
                    json!({
                        "ksef_number": m.ksef_number, "invoice_number": m.invoice_number,
                        "issue_date": m.issue_date, "buyer": m.buyer, "gross": m.gross,
                        "file": path.display().to_string(),
                    })
                })
                .collect();
            Ok(json!({"ok": true, "env": env, "invoices": invoices}))
        }
        Job::Compare { invoice, reference } => {
            let (ours, reference, c) = app.compare(invoice.as_deref(), reference.as_deref())?;
            Ok(json!({
                "ok": true, "ours": ours, "reference": reference, "consistent": c.is_consistent(),
                "missing": c.missing, "extra": c.extra, "different": c.different, "report": c.render(),
            }))
        }
    }
}

fn generate(app: &App, g: GenerateJob) -> Result<Value, String> {
    let dry_run = g.dry_run;
    let mut draft = build(app, g)?;
    let inv = &mut draft.invoice;
    if inv.currency != "PLN" && inv.exchange_rate.is_none() {
        inv.exchange_rate = Some(nbp::rate_before(
            &UreqTransport::new(),
            &inv.currency,
            inv.sale_date,
        )?);
    }
    if dry_run {
        let p = crate::validate::check(&draft.invoice, app.today);
        return Ok(json!({
            "ok": p.is_ok(), "dry_run": true, "errors": p.errors, "warnings": p.warnings,
            "preview": draft.preview(), "invoice": draft.invoice,
        }));
    }
    let generated = app.generate(draft.invoice)?;
    Ok(json!({
        "ok": true,
        "pdf": generated.pdf.display().to_string(),
        "xml": generated.stored.xml_path().display().to_string(),
        "warnings": generated.warnings,
        "summary": summary(&generated.stored),
    }))
}

/// The invoice a job (or the browser form) describes. Unset fields get the same defaults everywhere:
/// next free number for the issue month, sale date = issue date, due = issue date + payment_days.
pub fn build(app: &App, mut g: GenerateJob) -> Result<Draft, String> {
    let original = g.corrects.as_deref().map(|n| app.original(n)).transpose()?;
    let own_buyer = g.buyer.is_some();
    if let Some((o, _)) = &original {
        let o = &o.invoice;
        if g.currency
            .as_ref()
            .is_some_and(|c| !c.eq_ignore_ascii_case(&o.currency))
        {
            return Err(format!(
                "a correction keeps the currency of {} ({})",
                o.number, o.currency
            ));
        }
        g.currency = Some(o.currency.clone());
        g.number = g.number.or_else(|| Some(format!("KOR/{}", o.number)));
        g.sale_date = g.sale_date.or(Some(o.sale_date));
        g.language = g.language.or(Some(o.language));
    } else if g.reason.is_some() {
        return Err("'reason' is for corrections: say which invoice with 'corrects'".into());
    }
    let mut draft = app.new_draft()?;
    let mut commands = Vec::new();
    // Same commands the editor runs, so numbering and date defaults behave identically.
    commands.extend(g.buyer.map(|b| format!("buyer {b}")));
    commands.extend(g.language.map(|l| format!("lang {}", l.code())));
    commands.extend(g.issue_date.map(|d| format!("date {d}")));
    commands.extend(g.sale_date.map(|d| format!("sold {d}")));
    commands.extend(g.due_date.map(|d| format!("due {d}")));
    commands.extend(g.number.map(|n| format!("number {n}")));
    commands.extend(g.place.map(|p| format!("place {p}")));
    commands.extend(g.currency.map(|c| format!("currency {c}")));
    if g.from_last {
        commands.push("last".into());
    }
    for command in commands {
        if let Step::Leave | Step::Save = draft.apply(&command, &app.config, app)? {
            unreachable!("job commands never save or leave");
        }
    }
    draft.invoice.lines.extend(g.lines);
    draft.invoice.exchange_rate = g.exchange_rate;
    if let Some((o, reference)) = original {
        let inv = &mut draft.invoice;
        let o = o.invoice;
        if !own_buyer {
            inv.buyer = o.buyer.clone();
        }
        // The rate of the original applies to its correction too.
        inv.exchange_rate = o.exchange_rate;
        if let Some(other) = app
            .corrections_of(&o.number)?
            .into_iter()
            .find(|n| *n != inv.number)
        {
            return Err(format!(
                "{} is already corrected by {other}; change {other} instead (a second correction is not supported)",
                o.number
            ));
        }
        inv.correction = Some(Correction {
            reason: g.reason.unwrap_or_default().trim().to_string(),
            original: reference,
            before: o.lines,
            buyer_before: (inv.buyer != o.buyer).then_some(o.buyer),
        });
    }
    Ok(draft)
}

pub fn summary(stored: &Stored) -> Value {
    let inv = &stored.invoice;
    let t = inv.totals();
    let sent: serde_json::Map<String, Value> = Env::ALL
        .iter()
        .filter_map(|env| {
            stored
                .sent(*env)
                .map(|r| (env.name().to_string(), json!(r.ksef_number)))
        })
        .collect();
    json!({
        "number": inv.number, "slug": inv.slug(), "issue_date": inv.issue_date, "buyer": inv.buyer.name,
        "language": inv.language, "lines": inv.lines.len(),
        "net": t.net, "vat": t.tax, "gross": t.gross, "gross_text": t.gross.display(inv.language),
        "currency": inv.currency,
        "corrects": inv.correction.as_ref().map(|c| &c.original.number),
        "sent": sent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_action() {
        let job = parse(
            r#"{"action":"generate","buyer":"client","language":"en","issue_date":"2026-10-31",
               "lines":[{"name":"Usługi IT","quantity":160,"unit":"godz.","net_price":"100,00","vat":23}]}"#,
        )
        .unwrap();
        let Job::Generate(g) = job else { panic!() };
        assert_eq!(g.lines[0].quantity.xml(), "160");
        assert_eq!(g.language, Some(Lang::En));
        assert!(matches!(parse(r#"{"action":"list"}"#).unwrap(), Job::List));
        assert!(matches!(
            parse(r#"{"action":"send","env":"prod","confirm":"1/10/2026"}"#).unwrap(),
            Job::Send { env: Env::Prod, .. }
        ));
        assert!(parse(r#"{"action":"fetch","env":"prod","days":30}"#).is_ok());
        let Job::Generate(g) = parse(
            r#"{"action":"generate","currency":"EUR","exchange_rate":{"rate":"4.3128","table":"147/A/NBP/2026","date":"2026-07-31"},
               "lines":[{"name":"Web design","quantity":"156,2","unit":"godz","net_price":25,"vat":"np I","pkwiu":"62.10.B"}]}"#,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(g.exchange_rate.unwrap().rate.xml(), "4.3128");
        assert_eq!(g.lines[0].pkwiu.as_deref(), Some("62.10.B"));
        assert!(
            parse(r#"{"action":"fetch","days":30}"#)
                .unwrap_err()
                .contains("env")
        );
        assert!(parse(r#"{"action":"send"}"#).unwrap_err().contains("env"));
    }

    #[test]
    fn typos_in_jobs_are_rejected() {
        assert!(
            parse(r#"{"action":"generate","linez":[]}"#)
                .unwrap_err()
                .contains("linez")
        );
        assert!(parse(r#"{"action":"explode"}"#).is_err());
        assert!(parse(r#"{"action":"generate","lines":[{"name":"x","quantity":1,"unit":"h","net_price":1,"vat":"zw"}]}"#).is_err());
    }
}
