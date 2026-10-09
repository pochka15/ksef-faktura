//! The "Słownie" line: an amount spelled out, `dwanaście tysięcy ... PLN 67/100`.

use crate::i18n::Lang;
use crate::money::Money;

/// `... PLN 67/100`; in another currency, Polish spells the cents too (`... EUR zero centów`).
pub fn amount(money: Money, currency: &str, lang: Lang) -> String {
    let whole = match lang {
        Lang::Pl => polish(money.zloty()),
        Lang::En => english(money.zloty()),
    };
    match (lang, currency) {
        (Lang::Pl, c) if c != "PLN" => {
            let cents = money.grosze();
            let form = match (cents % 10, cents % 100) {
                _ if cents == 1 => "cent",
                (2..=4, n) if !(12..=14).contains(&n) => "centy",
                _ => "centów",
            };
            format!("{whole} {currency} {} {form}", polish(cents))
        }
        _ => format!("{whole} {currency} {:02}/100", money.grosze()),
    }
}

const PL_UNITS: [&str; 20] = [
    "zero",
    "jeden",
    "dwa",
    "trzy",
    "cztery",
    "pięć",
    "sześć",
    "siedem",
    "osiem",
    "dziewięć",
    "dziesięć",
    "jedenaście",
    "dwanaście",
    "trzynaście",
    "czternaście",
    "piętnaście",
    "szesnaście",
    "siedemnaście",
    "osiemnaście",
    "dziewiętnaście",
];
const PL_TENS: [&str; 10] = [
    "",
    "",
    "dwadzieścia",
    "trzydzieści",
    "czterdzieści",
    "pięćdziesiąt",
    "sześćdziesiąt",
    "siedemdziesiąt",
    "osiemdziesiąt",
    "dziewięćdziesiąt",
];
const PL_HUNDREDS: [&str; 10] = [
    "",
    "sto",
    "dwieście",
    "trzysta",
    "czterysta",
    "pięćset",
    "sześćset",
    "siedemset",
    "osiemset",
    "dziewięćset",
];
/// (one, two-to-four, five-and-more) forms for each power of a thousand.
const PL_SCALES: [(&str, &str, &str); 3] = [
    ("tysiąc", "tysiące", "tysięcy"),
    ("milion", "miliony", "milionów"),
    ("miliard", "miliardy", "miliardów"),
];

fn polish(n: i64) -> String {
    if n == 0 {
        return PL_UNITS[0].to_string();
    }
    let mut words = Vec::new();
    for (i, group) in groups(n).into_iter().enumerate().rev() {
        if group == 0 {
            continue;
        }
        if i == 0 {
            words.push(polish_below_thousand(group));
            continue;
        }
        let (one, few, many) = PL_SCALES[i - 1];
        if group == 1 {
            words.push(one.to_string());
            continue;
        }
        let form = if (2..=4).contains(&(group % 10)) && !(12..=14).contains(&(group % 100)) {
            few
        } else {
            many
        };
        words.push(format!("{} {form}", polish_below_thousand(group)));
    }
    words.join(" ")
}

fn polish_below_thousand(n: i64) -> String {
    let mut words = Vec::new();
    let (hundreds, rest) = (n / 100, n % 100);
    if hundreds > 0 {
        words.push(PL_HUNDREDS[hundreds as usize]);
    }
    if rest >= 20 {
        words.push(PL_TENS[(rest / 10) as usize]);
        if rest % 10 > 0 {
            words.push(PL_UNITS[(rest % 10) as usize]);
        }
    } else if rest > 0 {
        words.push(PL_UNITS[rest as usize]);
    }
    words.join(" ")
}

const EN_UNITS: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const EN_TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const EN_SCALES: [&str; 3] = ["thousand", "million", "billion"];

fn english(n: i64) -> String {
    if n == 0 {
        return EN_UNITS[0].to_string();
    }
    let mut words = Vec::new();
    for (i, group) in groups(n).into_iter().enumerate().rev() {
        if group == 0 {
            continue;
        }
        let text = english_below_thousand(group);
        words.push(if i == 0 {
            text
        } else {
            format!("{text} {}", EN_SCALES[i - 1])
        });
    }
    words.join(" ")
}

fn english_below_thousand(n: i64) -> String {
    let mut words = Vec::new();
    let (hundreds, rest) = (n / 100, n % 100);
    if hundreds > 0 {
        words.push(format!("{} hundred", EN_UNITS[hundreds as usize]));
    }
    if rest >= 20 {
        let tens = EN_TENS[(rest / 10) as usize];
        words.push(match rest % 10 {
            0 => tens.to_string(),
            unit => format!("{tens}-{}", EN_UNITS[unit as usize]),
        });
    } else if rest > 0 {
        words.push(EN_UNITS[rest as usize].to_string());
    }
    words.join(" ")
}

/// Splits into thousands, least significant first: 12345 -> [345, 12].
fn groups(mut n: i64) -> Vec<i64> {
    let mut out = Vec::new();
    while n > 0 {
        out.push(n % 1000);
        n /= 1000;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_amounts_with_grosze() {
        assert_eq!(
            amount(Money(1234567), "PLN", Lang::Pl),
            "dwanaście tysięcy trzysta czterdzieści pięć PLN 67/100"
        );
        assert_eq!(
            amount(Money(1234567), "PLN", Lang::En),
            "twelve thousand three hundred forty-five PLN 67/100"
        );
    }

    #[test]
    fn foreign_currencies_spell_the_cents_in_polish() {
        assert_eq!(
            amount(Money(390500), "EUR", Lang::Pl),
            "trzy tysiące dziewięćset pięć EUR zero centów"
        );
        assert_eq!(amount(Money(101), "EUR", Lang::Pl), "jeden EUR jeden cent");
        assert_eq!(
            amount(Money(122), "EUR", Lang::Pl),
            "jeden EUR dwadzieścia dwa centy"
        );
        assert_eq!(amount(Money(112), "EUR", Lang::Pl), "jeden EUR dwanaście centów");
        assert_eq!(
            amount(Money(390500), "EUR", Lang::En),
            "three thousand nine hundred five EUR 00/100"
        );
    }

    #[test]
    fn polish_plural_forms() {
        assert_eq!(polish(1000), "tysiąc");
        assert_eq!(polish(2000), "dwa tysiące");
        assert_eq!(polish(5000), "pięć tysięcy");
        assert_eq!(polish(12_000), "dwanaście tysięcy");
        assert_eq!(polish(22_000), "dwadzieścia dwa tysiące");
        assert_eq!(polish(1_001_015), "milion tysiąc piętnaście");
        assert_eq!(polish(215), "dwieście piętnaście");
        assert_eq!(polish(0), "zero");
    }

    #[test]
    fn english_numbers() {
        assert_eq!(english(1_000_000), "one million");
        assert_eq!(english(115), "one hundred fifteen");
        assert_eq!(amount(Money(5), "PLN", Lang::En), "zero PLN 05/100");
    }
}
