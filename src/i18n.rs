//! Invoice language and the printed labels. The KSeF XML is the same for both; only the PDF changes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Pl,
    En,
}

impl Lang {
    pub fn parse(text: &str) -> Result<Lang, String> {
        match text.trim().to_lowercase().as_str() {
            "pl" | "polish" => Ok(Lang::Pl),
            "en" | "english" => Ok(Lang::En),
            _ => Err(format!("language '{text}': use pl or en")),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Pl => "pl",
            Lang::En => "en",
        }
    }

    /// File name stem of the PDF: `Faktura 1-09-2026`, `Invoice 1-09-2026`.
    pub fn file_word(self) -> &'static str {
        match self {
            Lang::Pl => "Faktura",
            Lang::En => "Invoice",
        }
    }

    pub fn labels(self) -> serde_json::Value {
        let pairs: &[(&str, &str, &str)] = &[
            ("title", "Faktura", "Invoice"),
            ("number", "Nr", "No."),
            ("issue_date", "Data wystawienia", "Issue date"),
            ("sale_date", "Data sprzedaży", "Date of sale"),
            ("place", "Miejsce wystawienia", "Place of issue"),
            ("seller", "Sprzedawca", "Seller"),
            ("buyer", "Nabywca", "Buyer"),
            ("address", "Adres", "Address"),
            ("nip", "NIP", "Tax ID (NIP)"),
            ("phone", "Numer telefonu", "Phone"),
            ("email", "E-mail", "E-mail"),
            ("payment_method", "Sposób płatności", "Payment method"),
            ("transfer", "Przelew", "Bank transfer"),
            ("due", "Termin płatności", "Payment due"),
            ("bank", "Bank", "Bank"),
            ("account", "Numer konta", "Account number"),
            ("lp", "Lp.", "No."),
            ("name", "Nazwa", "Description"),
            ("qty", "Ilość", "Qty"),
            ("unit", "Jm", "Unit"),
            ("net_price", "Cena netto", "Net price"),
            ("net_value", "Wartość netto", "Net amount"),
            ("vat_rate", "Stawka VAT", "VAT rate"),
            ("vat_amount", "Kwota VAT", "VAT amount"),
            ("gross", "Wartość brutto", "Gross amount"),
            ("total", "Razem:", "Total:"),
            ("including", "W tym", "Including"),
            ("to_pay", "Razem do zapłaty", "Total due"),
            ("paid", "Zapłacono", "Paid"),
            ("remaining", "Pozostało do zapłaty", "Balance due"),
            ("in_words", "Słownie", "In words"),
            (
                "sig_issuer",
                "imię, nazwisko i podpis osoby upoważnionej do wystawienia dokumentu",
                "name and signature of the person authorised to issue the document",
            ),
            ("ksef_number", "Numer KSeF", "KSeF number"),
            ("title_correction", "Faktura korygująca", "Correcting invoice"),
            ("corrects", "Dotyczy faktury nr", "Corrects invoice no."),
            ("of_date", "z dnia", "of"),
            ("reason", "Przyczyna korekty", "Reason for correction"),
            (
                "buyer_before",
                "Dane nabywcy przed korektą",
                "Buyer before correction",
            ),
            ("before", "Przed korektą", "Before correction"),
            ("after", "Po korekcie", "After correction"),
            ("difference", "Różnica:", "Difference:"),
            ("to_refund", "Do zwrotu", "To be refunded"),
            ("tax_id", "Numer identyfikacji podatkowej", "Tax ID"),
            ("swift", "SWIFT", "SWIFT"),
            ("page", "Strona", "Page"),
            ("of", "z", "of"),
        ];
        let map = pairs
            .iter()
            .map(|(key, pl, en)| {
                let text = if self == Lang::Pl { pl } else { en };
                (key.to_string(), serde_json::Value::from(*text))
            })
            .collect();
        serde_json::Value::Object(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_labels() {
        assert_eq!(Lang::parse("EN").unwrap(), Lang::En);
        assert!(Lang::parse("de").is_err());
        assert_eq!(Lang::Pl.labels()["title"], "Faktura");
        assert_eq!(Lang::En.labels()["buyer"], "Buyer");
    }
}
