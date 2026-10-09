//! Country codes: which buyers are in the EU (FA(3) `KodUE` + `NrVatUE`) and country names for the PDF.

use crate::i18n::Lang;

/// EU VAT prefix for an ISO country code, `None` outside the EU. Greece is `GR` in ISO but `EL` for VAT;
/// `XI` (Northern Ireland) has an EU VAT prefix too. Matches `TKodyKrajowUE` in `assets/xsd/FA3.xsd`.
pub fn eu_vat_prefix(country: &str) -> Option<&'static str> {
    const EU: [&str; 28] = [
        "AT", "BE", "BG", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "EL", "HR", "HU", "IE", "IT", "LV", "LT",
        "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE", "XI",
    ];
    let code = match country {
        "GR" => "EL",
        other => other,
    };
    EU.iter().find(|c| **c == code).copied()
}

/// Two uppercase letters, like `GB`.
pub fn is_code(country: &str) -> bool {
    country.len() == 2 && country.chars().all(|c| c.is_ascii_uppercase())
}

/// Name as printed on the invoice; the code itself for countries not listed here.
pub fn name(country: &str, lang: Lang) -> String {
    const NAMES: [(&str, &str, &str); 38] = [
        ("PL", "Polska", "Poland"),
        ("GB", "Wielka Brytania", "United Kingdom"),
        ("US", "Stany Zjednoczone", "United States"),
        ("CA", "Kanada", "Canada"),
        ("CH", "Szwajcaria", "Switzerland"),
        ("NO", "Norwegia", "Norway"),
        ("UA", "Ukraina", "Ukraine"),
        ("AU", "Australia", "Australia"),
        ("IL", "Izrael", "Israel"),
        ("AE", "Zjednoczone Emiraty Arabskie", "United Arab Emirates"),
        ("SG", "Singapur", "Singapore"),
        ("AT", "Austria", "Austria"),
        ("BE", "Belgia", "Belgium"),
        ("BG", "Bułgaria", "Bulgaria"),
        ("CY", "Cypr", "Cyprus"),
        ("CZ", "Czechy", "Czechia"),
        ("DK", "Dania", "Denmark"),
        ("EE", "Estonia", "Estonia"),
        ("FI", "Finlandia", "Finland"),
        ("FR", "Francja", "France"),
        ("DE", "Niemcy", "Germany"),
        ("GR", "Grecja", "Greece"),
        ("HR", "Chorwacja", "Croatia"),
        ("HU", "Węgry", "Hungary"),
        ("IE", "Irlandia", "Ireland"),
        ("IT", "Włochy", "Italy"),
        ("LV", "Łotwa", "Latvia"),
        ("LT", "Litwa", "Lithuania"),
        ("LU", "Luksemburg", "Luxembourg"),
        ("MT", "Malta", "Malta"),
        ("NL", "Holandia", "Netherlands"),
        ("PT", "Portugalia", "Portugal"),
        ("RO", "Rumunia", "Romania"),
        ("SK", "Słowacja", "Slovakia"),
        ("SI", "Słowenia", "Slovenia"),
        ("ES", "Hiszpania", "Spain"),
        ("SE", "Szwecja", "Sweden"),
        ("XI", "Irlandia Północna", "Northern Ireland"),
    ];
    NAMES
        .iter()
        .find(|(code, _, _)| *code == country)
        .map(|(_, pl, en)| match lang {
            Lang::Pl => pl.to_string(),
            Lang::En => en.to_string(),
        })
        .unwrap_or_else(|| country.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eu_membership_and_names() {
        assert_eq!(eu_vat_prefix("DE"), Some("DE"));
        assert_eq!(eu_vat_prefix("GR"), Some("EL"));
        assert_eq!(eu_vat_prefix("GB"), None, "after Brexit");
        assert_eq!(name("GB", Lang::Pl), "Wielka Brytania");
        assert_eq!(name("PL", Lang::En), "Poland");
        assert_eq!(name("BR", Lang::Pl), "BR");
        assert!(is_code("GB") && !is_code("gb") && !is_code("GBR"));
    }
}
