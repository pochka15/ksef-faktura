//! An invoice built from short text commands (`date 2026-09-30`, `last`, ...). JSON jobs and the browser form
//! go through these, so numbering and date defaults are the same everywhere. Pure: no I/O.

use crate::config::Config;
use crate::i18n::Lang;
use crate::invoice::{Invoice, Line};
use crate::money::{Money, Quantity, Vat};
use jiff::civil::Date;
use jiff::{Span, Timestamp};

pub const HELP: &str = "\
add <qty> <unit> <net price> <vat%> <name...>   add a line: add 168 godz. 100 23 Usługi IT
rm <n>                                          remove line n
last                                            copy the lines of your previous invoice
buyer <key>                                     pick a buyer from config.json
lang pl|en                                      language of the PDF
date <YYYY-MM-DD|today>                         issue date (also moves sale date, due date and number)
sold <YYYY-MM-DD>, due <YYYY-MM-DD|+days>       sale date, payment due
number <text>, place <text|->                   invoice number, place of issue
show                                            preview
save                                            validate, write the PDF and keep the invoice
back                                            leave without saving";

/// What the shell should do after a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Say(String),
    Save,
    Leave,
}

/// Lookups the draft needs from outside: numbering and the previous invoice.
pub trait Context {
    fn next_number(&self, issue_date: Date) -> Result<String, String>;
    fn last_lines(&self) -> Option<(String, Vec<Line>)>;
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub invoice: Invoice,
    pub buyer_key: String,
    /// Set once typed by hand, so date changes stop overriding them.
    custom_number: bool,
    custom_sale: bool,
    custom_due: bool,
    custom_currency: bool,
    payment_days: i64,
}

impl Draft {
    pub fn new(config: &Config, today: Date, ctx: &dyn Context) -> Result<Draft, String> {
        let (buyer_key, buyer) = config.buyer(None)?;
        let invoice = Invoice {
            number: ctx.next_number(today)?,
            issue_date: today,
            sale_date: today,
            due_date: add_days(today, config.payment_days)?,
            place: config.place_of_issue.clone(),
            language: config.language,
            seller: config.seller.clone(),
            buyer,
            lines: Vec::new(),
            currency: config.currency_for(&buyer_key),
            exchange_rate: None,
            correction: None,
            created_at: Timestamp::UNIX_EPOCH,
        };
        Ok(Draft {
            invoice,
            buyer_key,
            custom_number: false,
            custom_sale: false,
            custom_due: false,
            custom_currency: false,
            payment_days: config.payment_days,
        })
    }

    pub fn apply(&mut self, line: &str, config: &Config, ctx: &dyn Context) -> Result<Step, String> {
        let line = line.trim();
        let (name, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        let inv = &mut self.invoice;
        let said = match name {
            "" => return Ok(Step::Say(String::new())),
            "add" | "a" => {
                inv.lines.push(parse_line(rest)?);
                self.preview()
            }
            "rm" | "remove" => {
                let n: usize = rest.parse().map_err(|_| "usage: rm <line number>".to_string())?;
                if n == 0 || n > inv.lines.len() {
                    return Err(format!("no line {n} (have {})", inv.lines.len()));
                }
                inv.lines.remove(n - 1);
                self.preview()
            }
            "last" => {
                let (number, lines) = ctx.last_lines().ok_or("no previous invoice yet")?;
                inv.lines = lines;
                format!("lines copied from {number}\n{}", self.preview())
            }
            "buyer" => {
                let (key, buyer) = config.buyer(Some(rest))?;
                inv.buyer = buyer;
                if !self.custom_currency {
                    inv.currency = config.currency_for(&key);
                }
                self.buyer_key = key;
                format!("buyer: {} ({})", self.invoice.buyer.name, self.invoice.currency)
            }
            "currency" => {
                let code = rest.to_uppercase();
                if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
                    return Err("usage: currency <3-letter code>, e.g. currency EUR".into());
                }
                inv.currency = code;
                self.custom_currency = true;
                format!("currency: {}", inv.currency)
            }
            "lang" => {
                inv.language = Lang::parse(rest)?;
                format!("language: {}", inv.language.code())
            }
            "number" => {
                if rest.is_empty() {
                    return Err("usage: number <text>".into());
                }
                inv.number = rest.to_string();
                self.custom_number = true;
                format!("number: {rest}")
            }
            "date" => {
                let date = parse_date(rest, self.invoice.issue_date)?;
                self.set_issue_date(date, ctx)?;
                self.preview()
            }
            "sold" => {
                inv.sale_date = parse_date(rest, inv.issue_date)?;
                self.custom_sale = true;
                format!("sale date: {}", inv.sale_date)
            }
            "due" => {
                inv.due_date = match rest.strip_prefix('+') {
                    Some(days) => add_days(
                        inv.issue_date,
                        days.parse().map_err(|_| "usage: due +14".to_string())?,
                    )?,
                    None => parse_date(rest, inv.issue_date)?,
                };
                self.custom_due = true;
                format!("payment due: {}", inv.due_date)
            }
            "place" => {
                inv.place = (!rest.is_empty() && rest != "-").then(|| rest.to_string());
                format!("place of issue: {}", inv.place.as_deref().unwrap_or("(none)"))
            }
            "show" | "s" => self.preview(),
            "save" => return Ok(Step::Save),
            "help" | "?" => HELP.to_string(),
            "back" | "quit" | "q" => return Ok(Step::Leave),
            _ => return Err(format!("unknown command '{name}' (try: help)")),
        };
        Ok(Step::Say(said))
    }

    fn set_issue_date(&mut self, date: Date, ctx: &dyn Context) -> Result<(), String> {
        self.invoice.issue_date = date;
        if !self.custom_sale {
            self.invoice.sale_date = date;
        }
        if !self.custom_due {
            self.invoice.due_date = add_days(date, self.payment_days)?;
        }
        if !self.custom_number {
            self.invoice.number = ctx.next_number(date)?;
        }
        Ok(())
    }

    pub fn preview(&self) -> String {
        let inv = &self.invoice;
        let lang = inv.language;
        let mut out = vec![
            format!(
                "{} {} ({}): issued {}, sold {}, due {}",
                lang.file_word(),
                inv.number,
                lang.code(),
                inv.issue_date,
                inv.sale_date,
                inv.due_date
            ),
            format!("buyer: {} ({})", inv.buyer.name, inv.buyer.id_text()),
        ];
        if inv.lines.is_empty() {
            out.push("no lines yet: add <qty> <unit> <net price> <vat%> <name...>, or `last`".into());
            return out.join("\n");
        }
        for (i, l) in inv.lines.iter().enumerate() {
            out.push(format!(
                "{:>3}. {} | {} {} x {} | {} | net {}",
                i + 1,
                l.name,
                l.quantity.display(lang),
                l.unit,
                l.net_price.display(lang),
                l.vat.label(),
                l.net().display(lang)
            ));
        }
        let t = inv.totals();
        out.push(format!(
            "net {} + VAT {} = {} {}",
            t.net.display(lang),
            t.tax.display(lang),
            t.gross.display(lang),
            inv.currency
        ));
        out.join("\n")
    }
}

/// `<qty> <unit> <net price> <vat> <name...>`; the name is the rest of the line, no quotes needed.
/// `<vat>` is `23`, `8`, `5`, `np I` or `np II`.
pub fn parse_line(args: &str) -> Result<Line, String> {
    const USAGE: &str =
        "usage: add <qty> <unit> <net price> <vat> <name...>, e.g. add 168 godz. 100 23 Usługi IT";
    let mut words = args.split_whitespace().peekable();
    let (Some(qty), Some(unit), Some(price), Some(vat)) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return Err(USAGE.into());
    };
    let vat = match (vat.eq_ignore_ascii_case("np"), words.peek()) {
        (true, Some(&kind)) if kind == "I" || kind == "II" => {
            format!("np {}", words.next().unwrap_or_default())
        }
        _ => vat.to_string(),
    };
    let name = words.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(USAGE.into());
    }
    Ok(Line {
        name,
        quantity: Quantity::parse(qty)?,
        unit: unit.to_string(),
        net_price: Money::parse(price)?,
        vat: Vat::parse(&vat)?,
        pkwiu: None,
    })
}

pub fn parse_date(text: &str, today: Date) -> Result<Date, String> {
    match text.trim() {
        "today" | "" => Ok(today),
        t => t.parse().map_err(|_| format!("'{t}' is not a date (YYYY-MM-DD)")),
    }
}

pub fn add_days(date: Date, days: i64) -> Result<Date, String> {
    date.checked_add(Span::new().days(days))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EXAMPLE;
    use crate::invoice::tests::line;

    struct Fake;

    impl Context for Fake {
        fn next_number(&self, d: Date) -> Result<String, String> {
            Ok(crate::invoice::number_for(1, d))
        }
        fn last_lines(&self) -> Option<(String, Vec<Line>)> {
            Some((
                "1/09/2026".into(),
                vec![line("Usługi IT", "168", "100", Vat::R23)],
            ))
        }
    }

    fn draft() -> (Draft, Config) {
        let config = Config::parse(EXAMPLE).unwrap().0;
        (
            Draft::new(&config, jiff::civil::date(2026, 10, 8), &Fake).unwrap(),
            config,
        )
    }

    fn run(d: &mut Draft, config: &Config, line: &str) -> String {
        match d.apply(line, config, &Fake).unwrap() {
            Step::Say(text) => text,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn starts_empty_with_defaults_from_config() {
        let (d, _) = draft();
        assert_eq!(d.invoice.number, "1/10/2026");
        assert_eq!(d.invoice.due_date, jiff::civil::date(2026, 10, 15));
        assert!(d.invoice.lines.is_empty());
    }

    #[test]
    fn add_and_remove_lines() {
        let (mut d, c) = draft();
        let out = run(&mut d, &c, "add 168 godz. 100,00 23 Usługi IT dla klienta");
        assert!(out.contains("net 16\u{a0}800,00 + VAT 3\u{a0}864,00"), "{out}");
        assert_eq!(d.invoice.lines[0].name, "Usługi IT dla klienta");
        run(&mut d, &c, "add 1 szt. 100 8 Licencja");
        run(&mut d, &c, "rm 1");
        assert_eq!(d.invoice.lines.len(), 1);
        assert!(d.apply("rm 5", &c, &Fake).unwrap_err().contains("no line 5"));
        assert!(d.apply("add 168 godz.", &c, &Fake).unwrap_err().contains("usage"));
        assert!(d.apply("add x godz. 1 23 a", &c, &Fake).is_err());
        run(&mut d, &c, "add 156,2 godz 25 np I Web design");
        assert_eq!(
            (d.invoice.lines[1].vat, d.invoice.lines[1].name.as_str()),
            (Vat::NpI, "Web design")
        );
    }

    #[test]
    fn last_copies_the_previous_lines() {
        let (mut d, c) = draft();
        assert!(run(&mut d, &c, "last").contains("copied from 1/09/2026"));
        assert_eq!(d.invoice.lines.len(), 1);
    }

    #[test]
    fn date_moves_sale_due_and_number_unless_set_by_hand() {
        let (mut d, c) = draft();
        run(&mut d, &c, "date 2026-09-30");
        assert_eq!(d.invoice.number, "1/09/2026");
        assert_eq!(d.invoice.sale_date, jiff::civil::date(2026, 9, 30));
        assert_eq!(d.invoice.due_date, jiff::civil::date(2026, 10, 7));
        run(&mut d, &c, "due +14");
        run(&mut d, &c, "number FV/7");
        run(&mut d, &c, "date 2026-09-29");
        assert_eq!(d.invoice.number, "FV/7");
        assert_eq!(
            d.invoice.due_date,
            jiff::civil::date(2026, 10, 14),
            "+14 from 09-30 stays"
        );
    }

    #[test]
    fn buyer_language_save_and_leave() {
        let (mut d, c) = draft();
        assert!(d.apply("buyer nobody", &c, &Fake).is_err());
        run(&mut d, &c, "lang en");
        assert_eq!(d.invoice.currency, "PLN");
        run(&mut d, &c, "currency eur");
        assert_eq!(d.invoice.currency, "EUR");
        assert!(d.apply("currency euro", &c, &Fake).is_err());
        assert!(d.preview().starts_with("Invoice 1/10/2026 (en)"));
        assert_eq!(d.apply("save", &c, &Fake).unwrap(), Step::Save);
        assert_eq!(d.apply("back", &c, &Fake).unwrap(), Step::Leave);
        assert!(
            d.apply("frobnicate", &c, &Fake)
                .unwrap_err()
                .contains("unknown command")
        );
    }
}
