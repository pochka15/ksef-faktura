//! Exact amounts, quantities and VAT rates. No floats: money is grosze, quantities are thousandths.

use crate::i18n::Lang;
use serde::{Deserialize, Serialize};

/// An amount in grosze (1/100 PLN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Value", into = "String")]
pub struct Money(pub i64);

/// A quantity in thousandths (`168` hours is `168_000`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Value", into = "String")]
pub struct Quantity(pub i64);

impl Money {
    /// Accepts `100`, `100.5`, `100,50`, `1 234,56`.
    pub fn parse(text: &str) -> Result<Money, String> {
        parse_scaled(text, 2)
            .map(Money)
            .map_err(|e| format!("'{text}' is not an amount: {e}"))
    }

    /// Dot decimal, two places, no grouping: what FA(3) wants.
    pub fn xml(self) -> String {
        format_scaled(self.0, 2, "", ".")
    }

    /// `16 800,00` in Polish, `16,800.00` in English.
    pub fn display(self, lang: Lang) -> String {
        match lang {
            Lang::Pl => format_scaled(self.0, 2, "\u{a0}", ","),
            Lang::En => format_scaled(self.0, 2, ",", "."),
        }
    }

    pub fn zloty(self) -> i64 {
        self.0 / 100
    }

    pub fn grosze(self) -> i64 {
        self.0 % 100
    }
}

impl Quantity {
    pub fn parse(text: &str) -> Result<Quantity, String> {
        let q = parse_scaled(text, 3).map_err(|e| format!("'{text}' is not a quantity: {e}"))?;
        if q == 0 {
            return Err("quantity must be more than 0".into());
        }
        Ok(Quantity(q))
    }

    /// Shortest form: `168`, `7.5`.
    pub fn xml(self) -> String {
        trim_zeros(format_scaled(self.0, 3, "", "."))
    }

    pub fn display(self, lang: Lang) -> String {
        let text = self.xml();
        match lang {
            Lang::Pl => text.replace('.', ","),
            Lang::En => text,
        }
    }

    /// Net value of `self` units at `price`, rounded half up to the grosz.
    pub fn times(self, price: Money) -> Money {
        Money(round_div(self.0 as i128 * price.0 as i128, 1000))
    }
}

/// The VAT rates this app issues. FA(3) knows more (zw, oo, 0 KR...); add them here when needed.
/// Order = order of their `P_13_x` sums in the XSD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Value", into = "String")]
pub enum Vat {
    R23,
    R8,
    R5,
    /// "np I": not subject to Polish VAT, place of supply abroad (e.g. services to a non-EU business, art. 28b).
    NpI,
    /// "np II": services to an EU business taxed by the buyer (art. 100 ust. 1 pkt 4).
    NpII,
}

impl Vat {
    pub const ALL: [Vat; 5] = [Vat::R23, Vat::R8, Vat::R5, Vat::NpI, Vat::NpII];

    pub fn parse(text: &str) -> Result<Vat, String> {
        let t = text.trim().trim_end_matches('%');
        match t.to_lowercase().split_whitespace().collect::<String>().as_str() {
            "23" => Ok(Vat::R23),
            "8" => Ok(Vat::R8),
            "5" => Ok(Vat::R5),
            "npi" => Ok(Vat::NpI),
            "npii" => Ok(Vat::NpII),
            _ => Err(format!(
                "VAT rate '{text}' not supported (use 23, 8, 5, np I or np II)"
            )),
        }
    }

    /// Percent for taxed rates; 0 for "not subject" (np) ones.
    pub fn percent(self) -> i64 {
        match self {
            Vat::R23 => 23,
            Vat::R8 => 8,
            Vat::R5 => 5,
            Vat::NpI | Vat::NpII => 0,
        }
    }

    /// FA(3) `P_12` value: `23`, `np I`, ...
    pub fn code(self) -> String {
        match self {
            Vat::NpI => "np I".into(),
            Vat::NpII => "np II".into(),
            taxed => taxed.percent().to_string(),
        }
    }

    /// As printed on the PDF: `23%`, `np`.
    pub fn label(self) -> String {
        match self {
            Vat::NpI | Vat::NpII => "np".into(),
            taxed => format!("{}%", taxed.percent()),
        }
    }

    pub fn is_taxed(self) -> bool {
        !matches!(self, Vat::NpI | Vat::NpII)
    }

    /// FA(3) field summing net sales at this rate, and the one summing its VAT (none for np).
    pub fn fields(self) -> (&'static str, Option<&'static str>) {
        match self {
            Vat::R23 => ("P_13_1", Some("P_14_1")),
            Vat::R8 => ("P_13_2", Some("P_14_2")),
            Vat::R5 => ("P_13_3", Some("P_14_3")),
            Vat::NpI => ("P_13_8", None),
            Vat::NpII => ("P_13_9", None),
        }
    }

    /// VAT on a net sum, rounded half up (how the totals on Polish invoices are computed).
    pub fn of(self, net: Money) -> Money {
        Money(round_div(net.0 as i128 * self.percent() as i128, 100))
    }
}

impl std::fmt::Display for Vat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code())
    }
}

/// An exchange rate with up to 6 decimals (FA(3) `KursWaluty`), as millionths: 4,3128 is 4_312_800.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Value", into = "String")]
pub struct Rate(pub i64);

impl Rate {
    pub fn parse(text: &str) -> Result<Rate, String> {
        let r = parse_scaled(text, 6).map_err(|e| format!("'{text}' is not an exchange rate: {e}"))?;
        if r == 0 {
            return Err("exchange rate must be more than 0".into());
        }
        Ok(Rate(r))
    }

    /// Shortest form with a dot: `4.3128`.
    pub fn xml(self) -> String {
        trim_zeros(format_scaled(self.0, 6, "", "."))
    }

    /// `4,3128` in Polish.
    pub fn display(self, lang: Lang) -> String {
        match lang {
            Lang::Pl => self.xml().replace('.', ","),
            Lang::En => self.xml(),
        }
    }

    /// An amount in the invoice currency converted to PLN, rounded half up to the grosz.
    pub fn to_pln(self, amount: Money) -> Money {
        Money(round_div(amount.0 as i128 * self.0 as i128, 1_000_000))
    }
}

fn round_div(value: i128, divisor: i128) -> i64 {
    let rounded = if value >= 0 {
        (value + divisor / 2) / divisor
    } else {
        (value - divisor / 2) / divisor
    };
    rounded as i64
}

fn parse_scaled(text: &str, places: u32) -> Result<i64, String> {
    let cleaned: String = text
        .trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '_'))
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    let (whole, fraction) = cleaned.split_once('.').unwrap_or((&cleaned, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err("empty".into());
    }
    if !whole.chars().chain(fraction.chars()).all(|c| c.is_ascii_digit()) {
        return Err("use digits and one decimal separator".into());
    }
    if fraction.len() > places as usize {
        return Err(format!("at most {places} decimal places"));
    }
    let scale = 10i64.pow(places);
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| "too large".to_string())?
    };
    let fraction: i64 = format!("{fraction:0<width$}", width = places as usize)
        .parse()
        .unwrap_or(0);
    whole
        .checked_mul(scale)
        .and_then(|w| w.checked_add(fraction))
        .ok_or_else(|| "too large".into())
}

fn format_scaled(value: i64, places: u32, group: &str, decimal: &str) -> String {
    let scale = 10i64.pow(places);
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    let digits = (value / scale).to_string();
    let mut whole = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            whole.push_str(group);
        }
        whole.push(c);
    }
    format!(
        "{sign}{whole}{decimal}{:0width$}",
        value % scale,
        width = places as usize
    )
}

fn trim_zeros(text: String) -> String {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// JSON jobs may write `100`, `100.5` or `"100,50"`; all go through the same text parser.
fn json_text(value: serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        other => Err(format!("expected a number or string, got {other}")),
    }
}

impl TryFrom<serde_json::Value> for Money {
    type Error = String;
    fn try_from(value: serde_json::Value) -> Result<Self, String> {
        Money::parse(&json_text(value)?)
    }
}

impl From<Money> for String {
    fn from(m: Money) -> String {
        m.xml()
    }
}

impl TryFrom<serde_json::Value> for Quantity {
    type Error = String;
    fn try_from(value: serde_json::Value) -> Result<Self, String> {
        Quantity::parse(&json_text(value)?)
    }
}

impl From<Quantity> for String {
    fn from(q: Quantity) -> String {
        q.xml()
    }
}

impl TryFrom<serde_json::Value> for Vat {
    type Error = String;
    fn try_from(value: serde_json::Value) -> Result<Self, String> {
        Vat::parse(&json_text(value)?)
    }
}

impl From<Vat> for String {
    fn from(v: Vat) -> String {
        v.code()
    }
}

impl TryFrom<serde_json::Value> for Rate {
    type Error = String;
    fn try_from(value: serde_json::Value) -> Result<Self, String> {
        Rate::parse(&json_text(value)?)
    }
}

impl From<Rate> for String {
    fn from(r: Rate) -> String {
        r.xml()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amounts_in_both_notations() {
        assert_eq!(Money::parse("100").unwrap(), Money(10000));
        assert_eq!(Money::parse("100,5").unwrap(), Money(10050));
        assert_eq!(Money::parse("1 234.56").unwrap(), Money(123456));
        assert!(Money::parse("1.234").is_err(), "three decimals");
        assert!(Money::parse("12a").is_err());
        assert!(Money::parse("").is_err());
    }

    #[test]
    fn formats_amounts_per_language() {
        let m = Money(1680000);
        assert_eq!(m.xml(), "16800.00");
        assert_eq!(m.display(Lang::Pl), "16\u{a0}800,00");
        assert_eq!(m.display(Lang::En), "16,800.00");
        assert_eq!(Money(5).xml(), "0.05");
        assert_eq!(Money(100_000_000).display(Lang::En), "1,000,000.00");
    }

    #[test]
    fn quantities_keep_up_to_three_decimals() {
        assert_eq!(Quantity::parse("168").unwrap().xml(), "168");
        assert_eq!(Quantity::parse("7,5").unwrap().xml(), "7.5");
        assert_eq!(Quantity::parse("7.5").unwrap().display(Lang::Pl), "7,5");
        assert!(Quantity::parse("0").is_err());
    }

    #[test]
    fn line_value_and_vat_round_half_up() {
        let hours = Quantity::parse("168").unwrap();
        let net = hours.times(Money::parse("100").unwrap());
        assert_eq!(net, Money(1680000));
        assert_eq!(Vat::R23.of(net), Money(386400));
        assert_eq!(Quantity::parse("0.333").unwrap().times(Money(100)), Money(33));
        assert_eq!(Vat::R23.of(Money(50)), Money(12), "11.5 grosze round up");
    }

    #[test]
    fn vat_rates() {
        assert_eq!(Vat::parse("23%").unwrap(), Vat::R23);
        assert_eq!(Vat::parse(" 8 ").unwrap(), Vat::R8);
        assert!(Vat::parse("zw").is_err());
        assert!(Vat::parse("np").is_err(), "np I or np II must be explicit");
        assert_eq!(Vat::parse("np I").unwrap(), Vat::NpI);
        assert_eq!(Vat::parse("NP II").unwrap(), Vat::NpII);
        assert_eq!((Vat::NpI.code(), Vat::NpI.label()), ("np I".into(), "np".into()));
        assert_eq!(Vat::NpI.of(Money(390500)), Money(0));
        assert_eq!(Vat::NpII.fields(), ("P_13_9", None));
        let v: Vat = serde_json::from_str("\"np I\"").unwrap();
        assert_eq!(serde_json::to_string(&v).unwrap(), "\"np I\"");
    }

    #[test]
    fn exchange_rates_convert_half_up() {
        let r = Rate::parse("4,3128").unwrap();
        assert_eq!((r.xml(), r.display(Lang::Pl)), ("4.3128".into(), "4,3128".into()));
        assert_eq!(r.to_pln(Money(100)), Money(431), "1 EUR = 4,3128 PLN -> 4,31");
        assert_eq!(r.to_pln(Money(0)), Money(0));
        assert!(Rate::parse("4.12345678").is_err());
        assert!(Rate::parse("0").is_err());
    }

    #[test]
    fn json_accepts_numbers_and_strings() {
        let m: Money = serde_json::from_str("100.5").unwrap();
        assert_eq!(m, Money(10050));
        let m: Money = serde_json::from_str("\"100,50\"").unwrap();
        assert_eq!(m, Money(10050));
        let v: Vat = serde_json::from_str("23").unwrap();
        assert_eq!(serde_json::to_string(&v).unwrap(), "\"23\"");
    }
}
