//! The invoice as this app stores it (`invoice.json`), with totals computed the way FA(3) wants them.

use crate::countries;
use crate::i18n::Lang;
use crate::money::{Money, Quantity, Rate, Vat};
use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

/// Seller or buyer. Addresses are two free-text lines, exactly as FA(3) `AdresL1` / `AdresL2`.
/// Polish parties have a `nip`; foreign buyers a `tax_id` (EU VAT number or any other tax number) instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Party {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nip: String,
    /// Foreign tax number, e.g. `GB123456789` or `DE123456789`. Only for `country` other than `PL`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    pub address_line1: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_line2: Option<String>,
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

fn default_country() -> String {
    "PL".into()
}

impl Party {
    pub fn is_polish(&self) -> bool {
        self.country == "PL"
    }

    /// What identifies the party on the invoice: the NIP, else the foreign tax id.
    pub fn id_text(&self) -> String {
        if self.is_polish() || self.tax_id.is_none() {
            self.nip.clone()
        } else {
            self.tax_id.clone().unwrap_or_default()
        }
    }

    /// Non-EU tax id for FA(3) `NrID`: without the country prefix, which `KodKraju` already carries
    /// (`GB123456789` -> `123456789`), as other invoicing programs send it.
    pub fn foreign_id(&self) -> Option<String> {
        let id: String = self
            .tax_id
            .as_ref()?
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        Some(match id.strip_prefix(self.country.as_str()) {
            Some(rest) if !rest.is_empty() => rest.to_string(),
            _ => id,
        })
    }

    /// EU VAT number split into FA(3) `KodUE` + `NrVatUE` (prefix dropped from the number if written).
    pub fn eu_vat(&self) -> Option<(&'static str, String)> {
        let prefix = countries::eu_vat_prefix(&self.country)?;
        let id: String = self
            .tax_id
            .as_ref()?
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let number = id.strip_prefix(prefix).unwrap_or(&id).to_string();
        Some((prefix, number))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seller {
    #[serde(flatten)]
    pub party: Party,
    pub bank_account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_name: Option<String>,
    /// SWIFT/BIC of the bank, e.g. `BPKOPLPW`; useful for payments from abroad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swift: Option<String>,
    /// Printed above "signature of the person authorised to issue". Defaults to nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub name: String,
    pub quantity: Quantity,
    pub unit: String,
    pub net_price: Money,
    pub vat: Vat,
    /// PKWiU classification symbol, e.g. `62.10.B` (FA(3) `PKWiU`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pkwiu: Option<String>,
}

impl Line {
    pub fn net(&self) -> Money {
        self.quantity.times(self.net_price)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    pub number: String,
    pub issue_date: Date,
    pub sale_date: Date,
    pub due_date: Date,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    pub language: Lang,
    pub seller: Seller,
    pub buyer: Party,
    pub lines: Vec<Line>,
    /// ISO 4217 code of the invoice amounts (FA(3) `KodWaluty`).
    #[serde(default = "pln", skip_serializing_if = "is_pln")]
    pub currency: String,
    /// Needed when `currency` is not PLN: the NBP rate used to state VAT in PLN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange_rate: Option<ExchangeRate>,
    /// Set on a correcting invoice (faktura korygująca, FA(3) `RodzajFaktury` KOR); `lines` are then the
    /// state after the correction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<Correction>,
    /// `DataWytworzeniaFa`: when the XML was produced. Fixed at generation so re-renders give the same XML.
    pub created_at: Timestamp,
}

pub fn pln() -> String {
    "PLN".into()
}

fn is_pln(currency: &String) -> bool {
    currency == "PLN"
}

/// NBP average rate (table A) of the last business day before the sale date (art. 31a ustawy o VAT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// PLN per unit of the invoice currency, e.g. `4.3128`.
    pub rate: Rate,
    /// NBP table number, e.g. `147/A/NBP/2026`.
    pub table: String,
    /// The table's publication date.
    pub date: Date,
}

/// What a correcting invoice corrects and how: the original's lines (and buyer) before the correction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correction {
    /// Printed and sent as `PrzyczynaKorekty`, e.g. "błędna liczba godzin".
    pub reason: String,
    pub original: Original,
    /// The original's lines (FA(3) rows with `StanPrzed`).
    pub before: Vec<Line>,
    /// The original's buyer, when the correction changes the buyer's data (FA(3) `Podmiot2K`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer_before: Option<Party>,
}

/// The corrected invoice as it is in KSeF (FA(3) `DaneFaKorygowanej`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Original {
    pub number: String,
    pub issue_date: Date,
    pub ksef_number: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateTotal {
    pub vat: Vat,
    pub net: Money,
    pub tax: Money,
}

impl RateTotal {
    pub fn gross(&self) -> Money {
        Money(self.net.0 + self.tax.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// One entry per rate used, highest rate first.
    pub rates: Vec<RateTotal>,
    pub net: Money,
    pub tax: Money,
    pub gross: Money,
}

impl Totals {
    /// VAT is computed per rate on the summed net values (not per line), as FA(3) `P_14_x` expects.
    pub fn of(lines: &[Line]) -> Totals {
        let rates = Vat::ALL
            .iter()
            .filter_map(|&vat| {
                let lines: Vec<&Line> = lines.iter().filter(|l| l.vat == vat).collect();
                if lines.is_empty() {
                    return None;
                }
                let net = Money(lines.iter().map(|l| l.net().0).sum());
                Some(RateTotal {
                    vat,
                    net,
                    tax: vat.of(net),
                })
            })
            .collect();
        Totals::from_rates(rates)
    }

    /// Per rate `self - before`, for every rate used on either side (a correction's amounts).
    fn minus(&self, before: &Totals) -> Totals {
        let find = |t: &Totals, vat| t.rates.iter().find(|r| r.vat == vat).copied();
        let rates = Vat::ALL
            .iter()
            .filter_map(|&vat| {
                let (a, b) = (find(self, vat), find(before, vat));
                if a.is_none() && b.is_none() {
                    return None;
                }
                let zero = RateTotal {
                    vat,
                    net: Money(0),
                    tax: Money(0),
                };
                let (a, b) = (a.unwrap_or(zero), b.unwrap_or(zero));
                Some(RateTotal {
                    vat,
                    net: Money(a.net.0 - b.net.0),
                    tax: Money(a.tax.0 - b.tax.0),
                })
            })
            .collect();
        Totals::from_rates(rates)
    }

    fn from_rates(rates: Vec<RateTotal>) -> Totals {
        let net = Money(rates.iter().map(|r| r.net.0).sum());
        let tax = Money(rates.iter().map(|r| r.tax.0).sum());
        Totals {
            rates,
            net,
            tax,
            gross: Money(net.0 + tax.0),
        }
    }
}

impl Invoice {
    /// The amounts the invoice states: its lines' totals, or for a correction the difference after - before
    /// (negative when the buyer gets money back).
    pub fn totals(&self) -> Totals {
        let after = Totals::of(&self.lines);
        match &self.correction {
            None => after,
            Some(c) => after.minus(&Totals::of(&c.before)),
        }
    }

    /// File-system friendly number: `1/09/2026` -> `1-09-2026`.
    pub fn slug(&self) -> String {
        slug(&self.number)
    }
}

pub fn slug(number: &str) -> String {
    number
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}

/// `{n}/{mm}/{yyyy}` with n counting invoices already issued in that month.
pub fn number_for(n: usize, issue_date: Date) -> String {
    format!("{n}/{:02}/{}", issue_date.month(), issue_date.year())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn party(name: &str, nip: &str) -> Party {
        Party {
            name: name.into(),
            nip: nip.into(),
            tax_id: None,
            address_line1: "ul. Testowa 1".into(),
            address_line2: Some("00-001 Warszawa".into()),
            country: "PL".into(),
            phone: None,
            email: None,
        }
    }

    pub fn line(name: &str, qty: &str, price: &str, vat: Vat) -> Line {
        Line {
            name: name.into(),
            quantity: Quantity::parse(qty).unwrap(),
            unit: "godz.".into(),
            net_price: Money::parse(price).unwrap(),
            vat,
            pkwiu: None,
        }
    }

    /// Fake data only; mirrors the shape of a monthly IT services invoice.
    pub fn sample() -> Invoice {
        Invoice {
            number: "1/09/2026".into(),
            issue_date: jiff::civil::date(2026, 9, 30),
            sale_date: jiff::civil::date(2026, 9, 30),
            due_date: jiff::civil::date(2026, 10, 7),
            place: None,
            language: Lang::Pl,
            seller: Seller {
                party: Party {
                    phone: Some("500600700".into()),
                    ..party("Jan Kowalski", "5265877635")
                },
                bank_account: "61 1090 1014 0000 0712 1981 2874".into(),
                bank_name: Some("Bank Testowy".into()),
                swift: None,
                issuer_name: Some("Jan Kowalski".into()),
            },
            buyer: party("Klient Sp. z o.o. & Co", "1111111111"),
            lines: vec![line("Usługi IT", "168", "100", Vat::R23)],
            currency: pln(),
            exchange_rate: None,
            correction: None,
            created_at: "2026-09-30T10:00:00Z".parse().unwrap(),
        }
    }

    /// Fake data only: 1/09/2026 corrected from 168 to 160 hours.
    pub fn sample_correction() -> Invoice {
        let original = sample();
        let mut inv = sample();
        inv.number = "KOR/1/09/2026".into();
        inv.issue_date = jiff::civil::date(2026, 10, 8);
        inv.due_date = jiff::civil::date(2026, 10, 15);
        inv.lines = vec![line("Usługi IT", "160", "100", Vat::R23)];
        inv.correction = Some(Correction {
            reason: "Błędna liczba godzin".into(),
            original: Original {
                number: original.number,
                issue_date: original.issue_date,
                ksef_number: "5265877635-20260930-0102030405A1-B2".into(),
            },
            before: original.lines,
            buyer_before: None,
        });
        inv
    }

    /// Fake data only: services to a UK company, in EUR, not subject to Polish VAT (np I).
    pub fn sample_foreign() -> Invoice {
        let mut inv = sample();
        inv.number = "1/08/2026".into();
        inv.issue_date = jiff::civil::date(2026, 9, 7);
        inv.sale_date = jiff::civil::date(2026, 8, 1);
        inv.due_date = jiff::civil::date(2026, 9, 21);
        inv.place = Some("Warszawa".into());
        inv.seller.bank_account = "PL61 1090 1014 0000 0712 1981 2874".into();
        inv.seller.swift = Some("WBKPPLPP".into());
        inv.buyer = Party {
            name: "Example Studio Ltd".into(),
            nip: String::new(),
            tax_id: Some("GB123456789".into()),
            address_line1: "1 Example Street".into(),
            address_line2: Some("EC1A 1AA London".into()),
            country: "GB".into(),
            phone: None,
            email: None,
        };
        inv.lines = vec![Line {
            pkwiu: Some("62.10.B".into()),
            ..line(
                "Usługi projektowania stron internetowych / Web design services",
                "156,2",
                "25",
                Vat::NpI,
            )
        }];
        inv.currency = "EUR".into();
        inv.exchange_rate = Some(ExchangeRate {
            rate: Rate::parse("4.3128").unwrap(),
            table: "147/A/NBP/2026".into(),
            date: jiff::civil::date(2026, 7, 31),
        });
        inv
    }

    #[test]
    fn totals_match_the_september_invoice() {
        let t = sample().totals();
        assert_eq!(t.net, Money(1680000));
        assert_eq!(t.tax, Money(386400));
        assert_eq!(t.gross, Money(2066400));
        assert_eq!(t.rates.len(), 1);
    }

    #[test]
    fn totals_group_by_rate_and_sum_before_rounding() {
        let mut inv = sample();
        inv.lines = vec![
            line("a", "1", "0.50", Vat::R8),
            line("b", "1", "0.50", Vat::R23),
            line("c", "1", "0.50", Vat::R23),
        ];
        let t = inv.totals();
        assert_eq!(
            t.rates.iter().map(|r| r.vat).collect::<Vec<_>>(),
            vec![Vat::R23, Vat::R8]
        );
        assert_eq!(t.rates[0].tax, Money(23), "1.00 * 23%, not 2 x round(0.115)");
        assert_eq!(t.gross, Money(150 + 23 + 4));
    }

    #[test]
    fn np_lines_have_no_vat() {
        let t = sample_foreign().totals();
        assert_eq!((t.net, t.tax, t.gross), (Money(390500), Money(0), Money(390500)));
        assert_eq!(t.rates[0].vat, Vat::NpI);
    }

    #[test]
    fn corrections_state_the_difference() {
        let inv = sample_correction();
        let t = inv.totals();
        // 160 h instead of 168 h at 100: -800.00 net, -184.00 VAT
        assert_eq!(
            (t.net, t.tax, t.gross),
            (Money(-80000), Money(-18400), Money(-98400))
        );
        let mut more = sample_correction();
        more.lines.push(line("Licencja", "1", "50", Vat::R8));
        let t = more.totals();
        assert_eq!(
            t.rates.iter().map(|r| (r.vat, r.net)).collect::<Vec<_>>(),
            vec![(Vat::R23, Money(-80000)), (Vat::R8, Money(5000))]
        );
        let mut cancel = sample_correction();
        cancel.lines.clear();
        assert_eq!(cancel.totals().gross, Money(-2066400), "everything back");
        let json = serde_json::to_string(&inv).unwrap();
        assert_eq!(serde_json::from_str::<Invoice>(&json).unwrap(), inv);
    }

    #[test]
    fn foreign_tax_ids() {
        let uk = sample_foreign().buyer;
        assert_eq!(
            (uk.is_polish(), uk.id_text(), uk.eu_vat()),
            (false, "GB123456789".into(), None)
        );
        let de = Party {
            country: "DE".into(),
            tax_id: Some("DE 123456789".into()),
            ..uk
        };
        assert_eq!(de.eu_vat(), Some(("DE", "123456789".into())));
    }

    #[test]
    fn numbers_and_slugs() {
        assert_eq!(number_for(2, jiff::civil::date(2026, 10, 31)), "2/10/2026");
        assert_eq!(slug("1/09/2026"), "1-09-2026");
        assert_eq!(slug(" FV 1/09 "), "FV-1-09");
    }

    #[test]
    fn round_trips_through_json() {
        let inv = sample();
        let json = serde_json::to_string_pretty(&inv).unwrap();
        assert!(json.contains("\"net_price\": \"100.00\""), "{json}");
        assert!(!json.contains("currency") && !json.contains("tax_id") && !json.contains("pkwiu"));
        assert_eq!(serde_json::from_str::<Invoice>(&json).unwrap(), inv);
        let foreign = sample_foreign();
        let json = serde_json::to_string(&foreign).unwrap();
        assert_eq!(serde_json::from_str::<Invoice>(&json).unwrap(), foreign);
    }
}
