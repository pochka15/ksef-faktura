//! Checks before anything leaves the machine: our own rules (checksums, dates, text) and the official
//! FA(3) XSD via macOS's built-in `xmllint`, fully offline.

use crate::countries;
use crate::ids;
use crate::invoice::{Invoice, Party};
use crate::money::Vat;
use jiff::civil::Date;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const XSD_FILES: [(&str, &str); 4] = [
    ("FA3.xsd", include_str!("../assets/xsd/FA3.xsd")),
    (
        "StrukturyDanych_v10-0E.xsd",
        include_str!("../assets/xsd/StrukturyDanych_v10-0E.xsd"),
    ),
    (
        "ElementarneTypyDanych_v10-0E.xsd",
        include_str!("../assets/xsd/ElementarneTypyDanych_v10-0E.xsd"),
    ),
    (
        "KodyKrajow_v10-0E.xsd",
        include_str!("../assets/xsd/KodyKrajow_v10-0E.xsd"),
    ),
];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Problems {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Problems {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn render(&self) -> String {
        let mut lines: Vec<String> = self.errors.iter().map(|e| format!("error: {e}")).collect();
        lines.extend(self.warnings.iter().map(|w| format!("warning: {w}")));
        lines.join("\n")
    }
}

/// Rules KSeF would reject (errors) or that look like a typo (warnings). `today` is the local date.
pub fn check(inv: &Invoice, today: Date) -> Problems {
    let mut p = Problems::default();
    party(&mut p, "seller", &inv.seller.party, false);
    party(&mut p, "buyer", &inv.buyer, true);
    currency(&mut p, inv);
    if let Some(swift) = &inv.seller.swift
        && !swift_is_valid(swift)
    {
        p.errors.push(format!(
            "seller SWIFT '{swift}' should be 8 or 11 letters/digits like BPKOPLPW"
        ));
    }
    if !ids::account_is_valid(&inv.seller.bank_account) {
        p.errors.push(format!(
            "seller bank account '{}' is not a valid 26-digit Polish account number",
            inv.seller.bank_account
        ));
    }
    if inv.number.trim().is_empty() {
        p.errors.push("invoice number is empty".into());
    }
    // A correction may remove every line (the whole invoice is cancelled).
    if inv.lines.is_empty() && inv.correction.is_none() {
        p.errors.push("no lines: add at least one".into());
    }
    correction(&mut p, inv);
    for (i, line) in inv.lines.iter().enumerate() {
        let n = i + 1;
        if line.name.trim().is_empty() || line.unit.trim().is_empty() {
            p.errors.push(format!("line {n}: name and unit are required"));
        }
        if line.name.chars().count() > 512 {
            p.errors
                .push(format!("line {n}: name longer than 512 characters"));
        }
        if line.net_price.0 <= 0 {
            p.errors.push(format!("line {n}: net price must be positive"));
        }
        if line
            .pkwiu
            .as_ref()
            .is_some_and(|k| k.trim().is_empty() || k.chars().count() > 50)
        {
            p.errors
                .push(format!("line {n}: PKWiU must be 1 to 50 characters"));
        }
        match (
            line.vat,
            inv.buyer.is_polish(),
            countries::eu_vat_prefix(&inv.buyer.country),
        ) {
            (Vat::NpI | Vat::NpII, true, _) => p.warnings.push(format!(
                "line {n}: 'np' (not subject to Polish VAT) with a Polish buyer; is the rate right?"
            )),
            (Vat::NpI, false, Some(_)) => p.warnings.push(format!(
                "line {n}: services to an EU business are usually 'np II' (art. 100 ust. 1 pkt 4), not 'np I'"
            )),
            (Vat::NpII, false, None) => p.warnings.push(format!(
                "line {n}: 'np II' is for EU business buyers; outside the EU it is usually 'np I'"
            )),
            _ => {}
        }
    }
    if inv.issue_date > today {
        p.errors.push(format!(
            "issue date {} is in the future (KSeF rejects that)",
            inv.issue_date
        ));
    }
    if inv.due_date < inv.issue_date {
        p.errors.push(format!(
            "payment due {} is before the issue date {}",
            inv.due_date, inv.issue_date
        ));
    }
    // A correction keeps the original's sale date, usually from an earlier month.
    if inv.correction.is_none()
        && (inv.sale_date.year() != inv.issue_date.year() || inv.sale_date.month() != inv.issue_date.month())
    {
        p.warnings.push(format!(
            "sale date {} is in a different month than the issue date {}",
            inv.sale_date, inv.issue_date
        ));
    }
    for text in texts(inv) {
        if let Some(c) = text.chars().find(|c| !allowed_char(*c)) {
            p.errors.push(format!(
                "character U+{:04X} in '{text}' is not allowed in KSeF XML",
                c as u32
            ));
        }
    }
    p
}

fn party(p: &mut Problems, who: &str, party: &Party, buyer: bool) {
    if party.name.trim().is_empty() || party.address_line1.trim().is_empty() {
        p.errors.push(format!("{who}: name and address are required"));
    }
    let (errors, warnings) = identity(who, party, buyer);
    p.errors.extend(errors);
    p.warnings.extend(warnings);
    if party.phone.as_deref().is_some_and(|ph| ph.chars().count() > 16) {
        p.errors.push(format!("{who}: phone longer than 16 characters"));
    }
}

/// Country and tax id rules, shared with the config check. Returns (errors, warnings).
/// Polish parties need a valid NIP; a foreign buyer has `tax_id` instead (EU VAT number or other tax id).
pub fn identity(who: &str, party: &Party, buyer: bool) -> (Vec<String>, Vec<String>) {
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    if !countries::is_code(&party.country) {
        errors.push(format!("{who}: country must be a 2-letter code like PL or GB"));
    } else if !party.is_polish() && !buyer {
        errors.push(format!("{who}: the seller must be Polish (country PL)"));
    }
    if party.is_polish() {
        if !ids::nip_is_valid(&party.nip) {
            errors.push(format!(
                "{who} NIP '{}' is not valid (10 digits, no dashes, checksum)",
                party.nip
            ));
        }
        if party.tax_id.is_some() {
            errors.push(format!(
                "{who}: tax_id is for foreign buyers; Polish ones have a nip"
            ));
        }
        return (errors, warnings);
    }
    if !party.nip.is_empty() {
        errors.push(format!(
            "{who}: a foreign buyer has no Polish NIP; put '{}' into tax_id instead",
            party.nip
        ));
    }
    match (&party.tax_id, party.eu_vat()) {
        (None, _) => warnings.push(format!(
            "{who}: no tax_id, the invoice will say the buyer has none"
        )),
        (Some(_), Some((prefix, number))) => {
            let ok = (1..=12).contains(&number.len())
                && number
                    .chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase() || c == '+' || c == '*');
            if !ok {
                errors.push(format!(
                    "{who}: EU VAT number '{number}' (after the {prefix} prefix) must be 1-12 digits/capital letters"
                ));
            }
        }
        (Some(id), None) => {
            if id.trim().is_empty() || id.chars().count() > 50 {
                errors.push(format!("{who}: tax_id must be 1 to 50 characters"));
            }
        }
    }
    (errors, warnings)
}

fn correction(p: &mut Problems, inv: &Invoice) {
    let Some(c) = &inv.correction else { return };
    if c.reason.trim().is_empty() {
        p.errors
            .push("correction: write the reason (e.g. 'błędna liczba godzin')".into());
    }
    if c.reason.chars().count() > 256 {
        p.errors
            .push("correction: reason longer than 256 characters".into());
    }
    if c.original.ksef_number.trim().is_empty() {
        p.errors.push(format!(
            "correction: {} has no KSeF number (only invoices in KSeF are corrected)",
            c.original.number
        ));
    }
    if c.original.number == inv.number {
        p.errors
            .push("correction: needs its own number, not the original's".into());
    }
    if inv.issue_date < c.original.issue_date {
        p.errors.push(format!(
            "correction: issue date {} is before the original's {}",
            inv.issue_date, c.original.issue_date
        ));
    }
    if inv.lines == c.before && c.buyer_before.is_none() {
        p.errors.push(format!(
            "correction: nothing changed compared with {}",
            c.original.number
        ));
    }
}

/// A foreign currency needs the NBP rate that converts its VAT to PLN.
fn currency(p: &mut Problems, inv: &Invoice) {
    let code = &inv.currency;
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
        p.errors.push(format!(
            "currency '{code}' must be a 3-letter code like PLN or EUR"
        ));
        return;
    }
    match (&inv.exchange_rate, code.as_str()) {
        (None, "PLN") => {}
        (Some(_), "PLN") => p
            .warnings
            .push("exchange rate given, but the invoice is in PLN; it is ignored".into()),
        (None, _) => p.errors.push(format!(
            "an invoice in {code} needs the NBP exchange rate (rate, table number and date)"
        )),
        (Some(rate), _) => {
            if rate.table.trim().is_empty() {
                p.errors
                    .push("exchange rate: NBP table number is missing (e.g. 147/A/NBP/2026)".into());
            }
            if rate.date >= inv.sale_date {
                p.warnings.push(format!(
                    "exchange rate dated {} should be from the last business day before the sale date {}",
                    rate.date, inv.sale_date
                ));
            }
        }
    }
}

fn swift_is_valid(swift: &str) -> bool {
    let b = swift.as_bytes();
    (b.len() == 8 || b.len() == 11)
        && b[..6].iter().all(u8::is_ascii_uppercase)
        && b[6..]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn texts(inv: &Invoice) -> Vec<&str> {
    let mut out = vec![inv.number.as_str()];
    for party in [&inv.seller.party, &inv.buyer] {
        out.extend([party.name.as_str(), party.address_line1.as_str()]);
        out.extend(party.address_line2.as_deref());
        out.extend(party.tax_id.as_deref());
    }
    for line in &inv.lines {
        out.extend([line.name.as_str(), line.unit.as_str()]);
        out.extend(line.pkwiu.as_deref());
    }
    if let Some(c) = &inv.correction {
        out.push(c.reason.as_str());
    }
    out
}

/// KSeF 2.4+ rejects control characters, private-use and non-characters ("unrecommended Unicode").
fn allowed_char(c: char) -> bool {
    let u = c as u32;
    !(c.is_control() && !matches!(c, '\t' | '\n' | '\r')
        || (0xE000..=0xF8FF).contains(&u)
        || (0xFDD0..=0xFDEF).contains(&u)
        || u & 0xFFFE == 0xFFFE)
}

/// Validates against the bundled FA(3) XSD. Needs `xmllint` (preinstalled on macOS).
pub fn xsd(xml: &str) -> Result<(), String> {
    let dir = schema_dir()?;
    let file = dir.join(format!("invoice-{}.xml", unique()));
    std::fs::write(&file, xml).map_err(|e| format!("writing {}: {e}", file.display()))?;
    let output = Command::new("xmllint")
        .args(["--noout", "--nonet", "--schema"])
        .arg(dir.join("FA3.xsd"))
        .arg(&file)
        .output();
    let _ = std::fs::remove_file(&file);
    let output =
        output.map_err(|e| format!("xmllint not found ({e}); install libxml2 or skip with --no-xsd"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reasons: Vec<&str> = stderr
        .lines()
        .filter(|l| !l.ends_with("fails to validate"))
        .map(|l| l.split_once("error : ").map_or(l, |(_, msg)| msg))
        .collect();
    Err(format!(
        "XML does not match the FA(3) schema:\n  {}",
        reasons.join("\n  ")
    ))
}

/// Distinct per process and per call, for temp file names.
fn unique() -> String {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    format!("{}-{}", std::process::id(), RUN.fetch_add(1, Ordering::Relaxed))
}

fn schema_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("ksef-cli-xsd-{}", env!("CARGO_PKG_VERSION")));
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    for (name, content) in XSD_FILES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content) {
            // Write-then-rename, so a concurrent run never reads a half-written schema.
            let partial = dir.join(format!("{name}.{}.tmp", unique()));
            std::fs::write(&partial, content)
                .and_then(|_| std::fs::rename(&partial, &path))
                .map_err(|e| format!("writing {}: {e}", path.display()))?;
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::tests::sample;

    fn today() -> Date {
        jiff::civil::date(2026, 10, 8)
    }

    #[test]
    fn corrections_need_a_reason_and_a_change() {
        let kor = crate::invoice::tests::sample_correction();
        let p = check(&kor, today());
        assert_eq!(p, Problems::default(), "{}", p.render());
        let mut same = kor.clone();
        same.lines = same.correction.as_ref().unwrap().before.clone();
        same.correction.as_mut().unwrap().reason = " ".into();
        let e = check(&same, today()).errors.join("\n");
        assert!(e.contains("nothing changed") && e.contains("reason"), "{e}");
        let mut cancel = kor;
        cancel.lines.clear();
        assert!(
            check(&cancel, today()).is_ok(),
            "cancelling every line is allowed"
        );
    }

    #[test]
    fn the_sample_is_clean() {
        let p = check(&sample(), today());
        assert_eq!(p, Problems::default(), "{}", p.render());
    }

    #[test]
    fn catches_typos_before_ksef_does() {
        let mut inv = sample();
        inv.buyer.nip = "1111111112".into();
        inv.seller.bank_account = "61 1090".into();
        inv.issue_date = jiff::civil::date(2026, 12, 1);
        inv.lines[0].name = "IT\u{0007}".into();
        let p = check(&inv, today());
        let all = p.render();
        for expected in ["buyer NIP", "bank account", "in the future", "U+0007"] {
            assert!(all.contains(expected), "missing '{expected}' in:\n{all}");
        }
    }

    #[test]
    fn a_foreign_np_invoice_in_eur_is_clean() {
        let p = check(&crate::invoice::tests::sample_foreign(), today());
        assert!(p.is_ok(), "{}", p.render());
        assert_eq!(p.warnings.len(), 1, "only the sale month differs: {}", p.render());
    }

    #[test]
    fn foreign_buyer_and_currency_rules() {
        let mut inv = crate::invoice::tests::sample_foreign();
        inv.buyer.nip = "GB123".into();
        inv.exchange_rate = None;
        inv.seller.swift = Some("bpko".into());
        let all = check(&inv, today()).render();
        for expected in ["into tax_id", "needs the NBP exchange rate", "SWIFT"] {
            assert!(all.contains(expected), "missing '{expected}' in:\n{all}");
        }
        let mut eu = crate::invoice::tests::sample_foreign();
        eu.buyer.country = "DE".into();
        eu.buyer.tax_id = Some("DE12345678901234".into());
        let all = check(&eu, today()).render();
        assert!(
            all.contains("EU VAT number") && all.contains("usually 'np II'"),
            "{all}"
        );
    }

    #[test]
    fn empty_invoice_is_an_error() {
        let mut inv = sample();
        inv.lines.clear();
        assert!(check(&inv, today()).render().contains("no lines"));
    }

    #[test]
    fn xsd_reports_the_reason() {
        let broken = crate::xml::fa3(&sample()).replace("<JST>2</JST>", "");
        match xsd(&broken) {
            Err(e) if e.contains("xmllint not found") => {}
            Err(e) => assert!(e.contains("GV") && e.contains("not expected"), "{e}"),
            Ok(()) => panic!("schema accepted an invoice without JST"),
        }
    }
}
