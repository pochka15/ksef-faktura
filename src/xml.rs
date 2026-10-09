//! The FA(3) e-invoice XML that KSeF accepts. Element order follows `assets/xsd/FA3.xsd`; when KSeF bumps
//! the schema, this file and the XSD are the two things to update.

use crate::ids;
use crate::invoice::{Invoice, Line, Party};
use crate::money::Rate;

pub const NAMESPACE: &str = "http://crd.gov.pl/wzor/2025/06/25/13775/";
pub const SYSTEM_CODE: &str = "FA (3)";
pub const SCHEMA_VERSION: &str = "1-0E";
pub const FORM_VALUE: &str = "FA";
const SYSTEM_INFO: &str = concat!("ksef-cli ", env!("CARGO_PKG_VERSION"));
/// `FormaPlatnosci` 6 = przelew (bank transfer).
const PAYMENT_TRANSFER: &str = "6";

pub fn fa3(inv: &Invoice) -> String {
    let mut x = Xml::default();
    x.raw(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    x.raw(&format!(
        r#"<Faktura xmlns:etd="http://crd.gov.pl/xml/schematy/dziedzinowe/mf/2022/01/05/eD/DefinicjeTypy/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns="{NAMESPACE}">"#
    ));

    x.open("Naglowek");
    x.raw(&format!(
        r#"<KodFormularza kodSystemowy="{SYSTEM_CODE}" wersjaSchemy="{SCHEMA_VERSION}">{FORM_VALUE}</KodFormularza>"#
    ));
    x.leaf("WariantFormularza", "3");
    x.leaf(
        "DataWytworzeniaFa",
        &inv.created_at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
    );
    x.leaf("SystemInfo", SYSTEM_INFO);
    x.close("Naglowek");

    x.open("Podmiot1");
    party(&mut x, &inv.seller.party, false);
    x.close("Podmiot1");

    x.open("Podmiot2");
    party(&mut x, &inv.buyer, true);
    // JST / GV: buyer is not a local-government unit / VAT-group member (1 = yes, 2 = no).
    x.leaf("JST", "2");
    x.leaf("GV", "2");
    x.close("Podmiot2");

    let totals = inv.totals();
    x.open("Fa");
    x.leaf("KodWaluty", &inv.currency);
    // In a foreign currency, VAT is also stated in PLN (P_14_xW) and the rate is given per line.
    let rate = inv
        .exchange_rate
        .as_ref()
        .filter(|_| inv.currency != "PLN")
        .map(|e| e.rate);
    x.leaf("P_1", &inv.issue_date.to_string());
    if let Some(place) = &inv.place {
        x.leaf("P_1M", place);
    }
    x.leaf("P_2", &inv.number);
    x.leaf("P_6", &inv.sale_date.to_string());
    for total in &totals.rates {
        let (net_field, tax_field) = total.vat.fields();
        x.leaf(net_field, &total.net.xml());
        if let Some(tax_field) = tax_field {
            x.leaf(tax_field, &total.tax.xml());
            if let Some(rate) = rate {
                x.leaf(&format!("{tax_field}W"), &rate.to_pln(total.tax).xml());
            }
        }
    }
    x.leaf("P_15", &totals.gross.xml());

    // Adnotacje: the "no special procedure" answers. 1 = applies, 2 = does not.
    x.open("Adnotacje");
    x.leaf("P_16", "2"); // cash accounting
    x.leaf("P_17", "2"); // self-billing
    x.leaf("P_18", "2"); // reverse charge
    x.leaf("P_18A", "2"); // split payment mechanism
    x.open("Zwolnienie");
    x.leaf("P_19N", "1"); // no VAT exemption
    x.close("Zwolnienie");
    x.open("NoweSrodkiTransportu");
    x.leaf("P_22N", "1"); // no new means of transport
    x.close("NoweSrodkiTransportu");
    x.leaf("P_23", "2"); // simplified triangular procedure
    x.open("PMarzy");
    x.leaf("P_PMarzyN", "1"); // no margin scheme
    x.close("PMarzy");
    x.close("Adnotacje");

    match &inv.correction {
        None => {
            x.leaf("RodzajFaktury", "VAT");
            lines(&mut x, &inv.lines, rate, false);
        }
        Some(c) => {
            x.leaf("RodzajFaktury", "KOR");
            x.leaf("PrzyczynaKorekty", &c.reason);
            x.open("DaneFaKorygowanej");
            x.leaf("DataWystFaKorygowanej", &c.original.issue_date.to_string());
            x.leaf("NrFaKorygowanej", &c.original.number);
            x.leaf("NrKSeF", "1");
            x.leaf("NrKSeFFaKorygowanej", &c.original.ksef_number);
            x.close("DaneFaKorygowanej");
            if let Some(buyer) = &c.buyer_before {
                x.open("Podmiot2K");
                identity(&mut x, buyer, true);
                address(&mut x, buyer);
                x.close("Podmiot2K");
            }
            // The original rows marked StanPrzed, then the rows as they are after the correction.
            lines(&mut x, &c.before, rate, true);
            lines(&mut x, &inv.lines, rate, false);
        }
    }

    // A correction that lowers the amount (or only fixes data) has nothing to pay.
    if inv.correction.is_none() || totals.gross.0 > 0 {
        payment(&mut x, inv);
    }
    x.close("Fa");
    x.raw("</Faktura>");
    x.out
}

fn payment(x: &mut Xml, inv: &Invoice) {
    x.open("Platnosc");
    x.open("TerminPlatnosci");
    x.leaf("Termin", &inv.due_date.to_string());
    x.close("TerminPlatnosci");
    x.leaf("FormaPlatnosci", PAYMENT_TRANSFER);
    x.open("RachunekBankowy");
    x.leaf("NrRB", &ids::compact_account(&inv.seller.bank_account));
    if let Some(swift) = &inv.seller.swift {
        x.leaf("SWIFT", swift);
    }
    if let Some(bank) = &inv.seller.bank_name {
        x.leaf("NazwaBanku", bank);
    }
    x.close("RachunekBankowy");
    x.close("Platnosc");
}

/// `FaWiersz` rows numbered from 1; `before`: the original state on a correction (`StanPrzed`).
fn lines(x: &mut Xml, lines: &[Line], rate: Option<Rate>, before: bool) {
    for (i, line) in lines.iter().enumerate() {
        x.open("FaWiersz");
        x.leaf("NrWierszaFa", &(i + 1).to_string());
        x.leaf("P_7", &line.name);
        if let Some(pkwiu) = &line.pkwiu {
            x.leaf("PKWiU", pkwiu);
        }
        x.leaf("P_8A", &line.unit);
        x.leaf("P_8B", &line.quantity.xml());
        x.leaf("P_9A", &line.net_price.xml());
        x.leaf("P_11", &line.net().xml());
        x.leaf("P_12", &line.vat.code());
        if let Some(rate) = rate {
            x.leaf("KursWaluty", &rate.xml());
        }
        if before {
            x.leaf("StanPrzed", "1");
        }
        x.close("FaWiersz");
    }
}

/// `buyer`: only the buyer (Podmiot2) may be foreign; the seller always has a Polish NIP.
fn party(x: &mut Xml, p: &Party, buyer: bool) {
    identity(x, p, buyer);
    address(x, p);
    if p.email.is_some() || p.phone.is_some() {
        x.open("DaneKontaktowe");
        if let Some(email) = &p.email {
            x.leaf("Email", email);
        }
        if let Some(phone) = &p.phone {
            x.leaf("Telefon", phone);
        }
        x.close("DaneKontaktowe");
    }
}

fn identity(x: &mut Xml, p: &Party, buyer: bool) {
    x.open("DaneIdentyfikacyjne");
    match (buyer && !p.is_polish(), p.eu_vat(), &p.tax_id) {
        (false, _, _) => x.leaf("NIP", &p.nip),
        (true, Some((prefix, number)), _) => {
            x.leaf("KodUE", prefix);
            x.leaf("NrVatUE", &number);
        }
        (true, None, Some(_)) => {
            x.leaf("KodKraju", &p.country);
            x.leaf("NrID", &p.foreign_id().unwrap_or_default());
        }
        (true, None, None) => x.leaf("BrakID", "1"),
    }
    x.leaf("Nazwa", &p.name);
    x.close("DaneIdentyfikacyjne");
}

fn address(x: &mut Xml, p: &Party) {
    x.open("Adres");
    x.leaf("KodKraju", &p.country);
    x.leaf("AdresL1", &p.address_line1);
    if let Some(line2) = &p.address_line2 {
        x.leaf("AdresL2", line2);
    }
    x.close("Adres");
}

/// Tiny indenting writer; values are escaped and whitespace-collapsed (FA(3) text types are `xsd:token`).
#[derive(Default)]
struct Xml {
    out: String,
    depth: usize,
}

impl Xml {
    fn raw(&mut self, text: &str) {
        self.line(text);
    }

    fn open(&mut self, tag: &str) {
        self.line(&format!("<{tag}>"));
        self.depth += 1;
    }

    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.line(&format!("</{tag}>"));
    }

    fn leaf(&mut self, tag: &str, value: &str) {
        let value = escape(&value.split_whitespace().collect::<Vec<_>>().join(" "));
        self.line(&format!("<{tag}>{value}</{tag}>"));
    }

    fn line(&mut self, text: &str) {
        // Indentation starts inside <Faktura>, which is opened through raw().
        let depth = if text.starts_with("<?xml") || text.starts_with("<Faktura") || text == "</Faktura>" {
            0
        } else {
            self.depth + 1
        };
        self.out.push_str(&"\t".repeat(depth));
        self.out.push_str(text);
        self.out.push('\n');
    }
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::tests::{sample, sample_correction, sample_foreign};

    /// The exact bytes we send. If this changes on purpose, regenerate with
    /// `UPDATE_GOLDEN=1 cargo test golden` and review the diff: it is what KSeF will see.
    fn golden(name: &str, xml: &str) {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, xml).unwrap();
        }
        let golden = std::fs::read_to_string(&path).expect("run once with UPDATE_GOLDEN=1");
        assert_eq!(xml, golden, "{name} changed");
    }

    #[test]
    fn golden_fa3() {
        golden("golden-fa3.xml", &fa3(&sample()));
    }

    /// EUR, np I, a UK buyer with a foreign tax id, SWIFT and PKWiU.
    #[test]
    fn golden_fa3_eur_np() {
        golden("golden-fa3-eur-np.xml", &fa3(&sample_foreign()));
    }

    /// KOR: 1/09/2026 corrected from 168 to 160 hours.
    #[test]
    fn golden_fa3_kor() {
        golden("golden-fa3-kor.xml", &fa3(&sample_correction()));
    }

    #[test]
    fn corrections_refer_to_the_original_and_state_the_difference() {
        let xml = fa3(&sample_correction());
        for expected in [
            "<RodzajFaktury>KOR</RodzajFaktury>\n\t\t<PrzyczynaKorekty>Błędna liczba godzin</PrzyczynaKorekty>",
            "<NrKSeFFaKorygowanej>5265877635-20260930-0102030405A1-B2</NrKSeFFaKorygowanej>",
            "<P_13_1>-800.00</P_13_1>\n\t\t<P_14_1>-184.00</P_14_1>\n\t\t<P_15>-984.00</P_15>",
            "<P_8B>168</P_8B>",
            "<StanPrzed>1</StanPrzed>",
            "<P_8B>160</P_8B>",
        ] {
            assert!(xml.contains(expected), "missing {expected} in\n{xml}");
        }
        assert_eq!(xml.matches("<StanPrzed>").count(), 1, "only the original row");
        assert!(!xml.contains("<Platnosc>"), "a refund has no payment terms");
        assert!(!xml.contains("Podmiot2K"));

        let mut up = sample_correction();
        up.lines[0].quantity = crate::money::Quantity::parse("170").unwrap();
        up.buyer.address_line1 = "ul. Nowa 2".into();
        up.correction.as_mut().unwrap().buyer_before = Some(sample().buyer);
        let xml = fa3(&up);
        assert!(
            xml.contains("<P_15>246.00</P_15>") && xml.contains("<Platnosc>"),
            "{xml}"
        );
        assert!(
            xml.contains("<Podmiot2K>\n\t\t\t<DaneIdentyfikacyjne>\n\t\t\t\t<NIP>1111111111</NIP>"),
            "{xml}"
        );
    }

    #[test]
    fn escapes_and_collapses_text() {
        let xml = fa3(&sample());
        assert!(xml.contains("<Nazwa>Klient Sp. z o.o. &amp; Co</Nazwa>"), "{xml}");
        assert!(xml.contains("<NrRB>61109010140000071219812874</NrRB>"));
        assert!(xml.contains("<P_13_1>16800.00</P_13_1>"));
        assert!(xml.contains("<P_14_1>3864.00</P_14_1>"));
        assert!(xml.contains("<P_15>20664.00</P_15>"));
        assert!(xml.contains("<DataWytworzeniaFa>2026-09-30T10:00:00Z</DataWytworzeniaFa>"));
    }

    #[test]
    fn foreign_buyers_are_identified_by_country() {
        let uk = fa3(&sample_foreign());
        for expected in [
            "<KodKraju>GB</KodKraju>\n\t\t\t<NrID>123456789</NrID>",
            "<KodWaluty>EUR</KodWaluty>",
            "<P_13_8>3905.00</P_13_8>",
            "<P_12>np I</P_12>",
            "<PKWiU>62.10.B</PKWiU>",
            "<KursWaluty>4.3128</KursWaluty>",
            "<NrRB>PL61109010140000071219812874</NrRB>",
            "<SWIFT>WBKPPLPP</SWIFT>",
        ] {
            assert!(uk.contains(expected), "missing {expected} in\n{uk}");
        }
        assert!(!uk.contains("P_14_") && !uk.contains("<NIP>GB"), "{uk}");

        let mut eu = sample_foreign();
        eu.buyer.country = "DE".into();
        eu.buyer.tax_id = Some("DE123456789".into());
        eu.lines[0].vat = crate::money::Vat::NpII;
        let eu = fa3(&eu);
        assert!(
            eu.contains("<KodUE>DE</KodUE>\n\t\t\t<NrVatUE>123456789</NrVatUE>"),
            "{eu}"
        );
        assert!(eu.contains("<P_13_9>3905.00</P_13_9>"), "{eu}");
    }

    #[test]
    fn taxed_lines_in_a_foreign_currency_state_vat_in_pln() {
        let mut inv = sample_foreign();
        inv.lines[0].vat = crate::money::Vat::R23;
        let xml = fa3(&inv);
        // 3905.00 EUR * 23% = 898.15 EUR; * 4.3128 = 3873.54 PLN
        assert!(
            xml.contains("<P_14_1>898.15</P_14_1>\n\t\t<P_14_1W>3873.54</P_14_1W>"),
            "{xml}"
        );
    }

    #[test]
    fn passes_the_official_schema() {
        let mut eu = sample_foreign();
        eu.buyer.country = "DE".into();
        eu.buyer.tax_id = Some("DE123456789".into());
        let mut no_id = sample_foreign();
        no_id.buyer.tax_id = None;
        let mut taxed = sample_foreign();
        taxed.lines[0].vat = crate::money::Vat::R23;
        let mut kor_up = sample_correction();
        kor_up.lines[0].quantity = crate::money::Quantity::parse("170").unwrap();
        kor_up.correction.as_mut().unwrap().buyer_before = Some(sample().buyer);
        let mut kor_eur = sample_foreign();
        kor_eur.number = "KOR/1/08/2026".into();
        kor_eur.lines[0].quantity = crate::money::Quantity::parse("150").unwrap();
        kor_eur.correction = Some(crate::invoice::Correction {
            before: sample_foreign().lines,
            ..sample_correction().correction.unwrap()
        });
        let mut kor_cancel = sample_correction();
        kor_cancel.lines.clear();
        for (name, inv) in [
            ("PL", sample()),
            ("UK np I EUR", sample_foreign()),
            ("EU", eu),
            ("no id", no_id),
            ("EUR 23%", taxed),
            ("KOR down", sample_correction()),
            ("KOR up + buyer", kor_up),
            ("KOR EUR", kor_eur),
            ("KOR to zero", kor_cancel),
        ] {
            match crate::validate::xsd(&fa3(&inv)) {
                Err(e) if e.contains("xmllint not found") => eprintln!("skipped: {e}"),
                other => other.unwrap_or_else(|e| panic!("{name}: {e}")),
            }
        }
    }
}
