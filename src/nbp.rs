//! NBP average exchange rates (table A) from api.nbp.pl, for invoices in a foreign currency: the rate of the
//! last business day before the sale date (art. 31a ustawy o VAT). Read-only and public; no login.

use crate::invoice::ExchangeRate;
use crate::ksef::http::{Request, Transport};
use crate::money::Rate;
use jiff::ToSpan;
use jiff::civil::Date;
use serde_json::Value;

const API: &str = "https://api.nbp.pl/api/exchangerates/rates/a";

/// The newest table A rate published strictly before `sale_date`. Looks back two weeks (holidays included).
pub fn rate_before(http: &impl Transport, currency: &str, sale_date: Date) -> Result<ExchangeRate, String> {
    let end = sale_date.checked_sub(1.day()).map_err(|e| e.to_string())?;
    let start = sale_date.checked_sub(14.days()).map_err(|e| e.to_string())?;
    let url = format!("{API}/{}/{start}/{end}/?format=json", currency.to_lowercase());
    let response = http.send(&Request {
        method: "GET",
        url,
        bearer: None,
        json: None,
    })?;
    if response.status == 404 {
        return Err(format!(
            "NBP has no table A rate for {currency} between {start} and {end} (is the currency code right?)"
        ));
    }
    if !response.is_success() {
        return Err(format!(
            "NBP answered {}: {}",
            response.status,
            response.body.trim()
        ));
    }
    let body: Value = serde_json::from_str(&response.body).map_err(|e| format!("NBP response: {e}"))?;
    let newest = body["rates"]
        .as_array()
        .and_then(|rates| {
            rates
                .iter()
                .max_by_key(|r| r["effectiveDate"].as_str().unwrap_or(""))
        })
        .ok_or("NBP response has no rates")?;
    let field = |name: &str| newest[name].as_str().ok_or(format!("NBP response: no {name}"));
    let mid = match &newest["mid"] {
        Value::Number(n) => n.to_string(),
        _ => return Err("NBP response: no mid rate".into()),
    };
    Ok(ExchangeRate {
        rate: Rate::parse(&mid)?,
        table: field("no")?.to_string(),
        date: field("effectiveDate")?
            .parse()
            .map_err(|e| format!("NBP response date: {e}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ksef::http::tests::FakeTransport;

    #[test]
    fn picks_the_last_table_before_the_sale_date() {
        let http = FakeTransport::new(vec![(
            200,
            r#"{"table":"A","currency":"euro","code":"EUR","rates":[
                {"no":"146/A/NBP/2026","effectiveDate":"2026-07-30","mid":4.3001},
                {"no":"147/A/NBP/2026","effectiveDate":"2026-07-31","mid":4.3128}]}"#,
        )]);
        let r = rate_before(&http, "EUR", jiff::civil::date(2026, 8, 1)).unwrap();
        assert_eq!(
            (r.rate.xml(), r.table.as_str()),
            ("4.3128".into(), "147/A/NBP/2026")
        );
        assert_eq!(r.date, jiff::civil::date(2026, 7, 31));
        let url = &http.requests.borrow()[0].url;
        assert!(url.ends_with("/eur/2026-07-18/2026-07-31/?format=json"), "{url}");
    }

    #[test]
    fn explains_missing_rates() {
        let http = FakeTransport::new(vec![(404, "NotFound - Not Found")]);
        let e = rate_before(&http, "XYZ", jiff::civil::date(2026, 8, 1)).unwrap_err();
        assert!(e.contains("no table A rate for XYZ"), "{e}");
    }
}
