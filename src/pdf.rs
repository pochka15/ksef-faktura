//! Renders the invoice PDF with an embedded Typst compiler: `assets/invoice.typ` is the layout, this
//! module only formats the values (amounts, words, labels) into the JSON the template reads.

use crate::countries;
use crate::i18n::Lang;
use crate::ids;
use crate::invoice::{ExchangeRate, Invoice, Line, Party, Totals};
use crate::money::Money;
use crate::words;
use serde_json::{Value, json};
use typst::foundations::{Dict, IntoValue};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

static TEMPLATE: &str = include_str!("../assets/invoice.typ");
static FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");

/// `ksef_number` is printed once the invoice has been accepted by KSeF production.
pub fn render(inv: &Invoice, ksef_number: Option<&str>) -> Result<Vec<u8>, String> {
    let engine = TypstEngine::builder()
        .main_file(TEMPLATE)
        .fonts([FONT_REGULAR, FONT_BOLD])
        .build();
    let mut inputs = Dict::new();
    inputs.insert("data".into(), data(inv, ksef_number).to_string().into_value());
    let doc: PagedDocument = engine
        .compile_with_input(inputs)
        .output
        .map_err(|e| format!("PDF layout failed: {e}"))?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        format!("PDF export failed: {}", messages.join("; "))
    })
}

fn data(inv: &Invoice, ksef_number: Option<&str>) -> Value {
    let lang = inv.language;
    let money = |m: Money| m.display(lang);
    let totals = inv.totals();
    let lines = lines_json(&inv.lines, lang);
    let (total, rates) = totals_json(&totals, lang);
    let cur = |m: Money| format!("{} {}", money(m), inv.currency);
    let countries = !inv.seller.party.is_polish() || !inv.buyer.is_polish();
    // A correction prints the original's rows and the corrected ones; its totals are the difference.
    let correction = inv.correction.as_ref().map(|c| {
        let side = |lines: &[Line]| {
            let (total, rates) = totals_json(&Totals::of(lines), lang);
            json!({"lines": lines_json(lines, lang), "total": total, "rates": rates})
        };
        json!({
            "number": c.original.number,
            "issue_date": c.original.issue_date.to_string(),
            "ksef_number": c.original.ksef_number,
            "reason": c.reason,
            "buyer_before": c.buyer_before.as_ref().map(|b| party(b, lang, countries)),
            "before": side(&c.before),
            "after": side(&inv.lines),
        })
    });
    let refund = totals.gross.0 < 0;
    let due = Money(totals.gross.0.abs());
    // Country names only when someone is abroad; domestic invoices look as before.
    let foreign = inv.currency != "PLN";
    json!({
        "lang": lang.code(),
        "labels": lang.labels(),
        "number": inv.number,
        "issue_date": inv.issue_date.to_string(),
        "sale_date": inv.sale_date.to_string(),
        "place": inv.place,
        "ksef_number": ksef_number,
        "seller": party(&inv.seller.party, lang, countries),
        "buyer": party(&inv.buyer, lang, countries),
        "due": inv.due_date.to_string(),
        "bank": inv.seller.bank_name,
        "account": ids::format_account(&inv.seller.bank_account),
        "swift": inv.seller.swift,
        "account_currency": foreign.then(|| inv.currency.clone()),
        "lines": lines,
        "total": total,
        "rates": rates,
        "correction": correction,
        "refund": refund,
        "to_pay": cur(due),
        "paid": cur(Money(0)),
        "remaining": cur(due),
        "in_words": words::amount(due, &inv.currency, lang),
        "fx": inv.exchange_rate.as_ref().filter(|_| foreign).map(|fx| exchange_lines(inv, fx, totals.tax)),
        "issuer_name": inv.seller.issuer_name,
    })
}

fn lines_json(lines: &[Line], lang: Lang) -> Vec<Value> {
    let money = |m: Money| m.display(lang);
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let tax = l.vat.of(l.net());
            json!({
                "lp": (i + 1).to_string(),
                "name": line_name(l),
                "qty": l.quantity.display(lang),
                "unit": l.unit,
                "net_price": money(l.net_price),
                "net_value": money(l.net()),
                "vat_rate": l.vat.label(),
                "vat_amount": money(tax),
                "gross": money(Money(l.net().0 + tax.0)),
            })
        })
        .collect()
}

/// The "Razem" row and one "W tym" row per rate.
fn totals_json(t: &Totals, lang: Lang) -> (Value, Vec<Value>) {
    let money = |m: Money| m.display(lang);
    let rates = t
        .rates
        .iter()
        .map(|r| {
            json!({
                "rate": r.vat.label(),
                "net": money(r.net),
                "vat": money(r.tax),
                "gross": money(r.gross()),
            })
        })
        .collect();
    let total = json!({"net": money(t.net), "vat": money(t.tax), "gross": money(t.gross)});
    (total, rates)
}

/// The PKWiU symbol is printed after the name unless the name already contains it.
fn line_name(l: &crate::invoice::Line) -> String {
    match &l.pkwiu {
        Some(k) if !l.name.contains(k.as_str()) => format!("{} (PKWiU {k})", l.name),
        _ => l.name.clone(),
    }
}

/// `Kurs waluty PLN/EUR 4,3128, tabela kursów średnich NBP nr 147/A/NBP/2026 / z dnia 2026-07-31` and the
/// VAT in PLN, as Polish invoices in a foreign currency state them.
fn exchange_lines(inv: &Invoice, fx: &ExchangeRate, tax: Money) -> [String; 2] {
    let lang = inv.language;
    let (rate, vat) = (fx.rate.display(lang), fx.rate.to_pln(tax).display(lang));
    let c = &inv.currency;
    match lang {
        Lang::Pl => [
            format!(
                "Kurs waluty PLN/{c} {rate}, tabela kursów średnich NBP nr {} / z dnia {}",
                fx.table, fx.date
            ),
            format!("Przeliczona kwota VAT: {vat} PLN"),
        ],
        Lang::En => [
            format!(
                "Exchange rate PLN/{c} {rate}, NBP average rates table no. {} of {}",
                fx.table, fx.date
            ),
            format!("VAT converted to PLN: {vat} PLN"),
        ],
    }
}

fn party(p: &Party, lang: Lang, with_country: bool) -> Value {
    let mut address = match &p.address_line2 {
        Some(line2) => format!("{}, {line2}", p.address_line1),
        None => p.address_line1.clone(),
    };
    if with_country {
        address = format!("{address}, {}", countries::name(&p.country, lang));
    }
    let foreign_id = !p.is_polish();
    json!({
        "name": p.name,
        "address": address,
        "nip": p.id_text(),
        "id_label": if foreign_id { "tax_id" } else { "nip" },
        "phone": p.phone,
        "email": p.email,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::tests::sample;

    #[test]
    fn renders_both_languages() {
        for lang in [Lang::Pl, Lang::En] {
            let mut inv = sample();
            inv.language = lang;
            let pdf = render(&inv, Some("5265877635-20260930-0102030405A1-B2")).unwrap();
            assert!(pdf.starts_with(b"%PDF"), "{lang:?}");
            if let Some(dir) = std::env::var_os("KSEF_PDF_PREVIEW") {
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("sample-{}.pdf", lang.code())),
                    pdf,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn corrections_print_before_after_and_the_refund() {
        let mut up = crate::invoice::tests::sample_correction();
        up.lines.push(crate::invoice::tests::line(
            "Licencja",
            "1",
            "500",
            crate::money::Vat::R8,
        ));
        up.buyer.address_line1 = "ul. Nowa 2".into();
        up.correction.as_mut().unwrap().buyer_before = Some(sample().buyer);
        let mut cancel = crate::invoice::tests::sample_correction();
        cancel.lines.clear();
        for (name, inv) in [
            ("kor", crate::invoice::tests::sample_correction()),
            ("kor-up", up),
            ("kor-cancel", cancel),
        ] {
            let pdf = render(&inv, None).unwrap_or_else(|e| panic!("{name}: {e}"));
            if let Some(dir) = std::env::var_os("KSEF_PDF_PREVIEW") {
                std::fs::write(std::path::Path::new(&dir).join(format!("sample-{name}.pdf")), pdf).unwrap();
            }
        }
        let d = data(&crate::invoice::tests::sample_correction(), None);
        assert_eq!(d["refund"], true);
        assert_eq!(d["to_pay"], "984,00 PLN");
        assert_eq!(d["total"]["gross"], "-984,00");
        assert_eq!(d["in_words"], "dziewięćset osiemdziesiąt cztery PLN 00/100");
        assert_eq!(d["correction"]["before"]["lines"][0]["qty"], "168");
        assert_eq!(d["correction"]["after"]["total"]["gross"], "19\u{a0}680,00");
    }

    #[test]
    fn foreign_invoices_show_currency_rate_and_countries() {
        let inv = crate::invoice::tests::sample_foreign();
        let pdf = render(&inv, None).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        if let Some(dir) = std::env::var_os("KSEF_PDF_PREVIEW") {
            std::fs::write(std::path::Path::new(&dir).join("sample-eur.pdf"), pdf).unwrap();
        }
        let d = data(&inv, None);
        assert_eq!(d["to_pay"], "3\u{a0}905,00 EUR");
        assert_eq!(d["in_words"], "trzy tysiące dziewięćset pięć EUR zero centów");
        assert_eq!(d["lines"][0]["vat_rate"], "np");
        assert_eq!(d["lines"][0]["qty"], "156,2");
        assert!(
            d["lines"][0]["name"]
                .as_str()
                .unwrap()
                .ends_with("(PKWiU 62.10.B)")
        );
        assert_eq!(d["rates"][0]["rate"], "np");
        assert_eq!(d["account"], "PL61 1090 1014 0000 0712 1981 2874");
        assert_eq!(
            (d["swift"].clone(), d["account_currency"].clone()),
            (json!("WBKPPLPP"), json!("EUR"))
        );
        assert_eq!(
            d["buyer"]["address"],
            "1 Example Street, EC1A 1AA London, Wielka Brytania"
        );
        assert_eq!(d["seller"]["address"], "ul. Testowa 1, 00-001 Warszawa, Polska");
        assert_eq!(
            (d["buyer"]["id_label"].clone(), d["buyer"]["nip"].clone()),
            (json!("tax_id"), json!("GB123456789"))
        );
        assert_eq!(
            d["fx"][0],
            "Kurs waluty PLN/EUR 4,3128, tabela kursów średnich NBP nr 147/A/NBP/2026 / z dnia 2026-07-31"
        );
        assert_eq!(d["fx"][1], "Przeliczona kwota VAT: 0,00 PLN");
        let domestic = data(&crate::invoice::tests::sample(), None);
        assert_eq!(domestic["fx"], Value::Null);
        assert_eq!(
            domestic["buyer"]["address"], "ul. Testowa 1, 00-001 Warszawa",
            "no country for PL-only"
        );
    }

    #[test]
    fn template_data_is_formatted_for_the_reader() {
        let d = data(&sample(), None);
        assert_eq!(d["total"]["gross"], "20\u{a0}664,00");
        assert_eq!(d["to_pay"], "20\u{a0}664,00 PLN");
        assert_eq!(d["account"], "61 1090 1014 0000 0712 1981 2874");
        assert_eq!(d["lines"][0]["vat_rate"], "23%");
        assert_eq!(d["ksef_number"], Value::Null);
        assert!(d["in_words"].as_str().unwrap().starts_with("dwadzieścia tysięcy"));
    }
}
