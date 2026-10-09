//! Structural comparison of two FA(3) invoices: "is my generated XML shaped like the ones I sent before?"
//! Values that legitimately change every month (dates, number, amounts) are listed separately.

use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::{BTreeMap, BTreeSet};

/// Leaves that are expected to differ between two monthly invoices.
const VARYING: [&str; 16] = [
    "Faktura/Naglowek/DataWytworzeniaFa",
    "Faktura/Naglowek/SystemInfo",
    "Faktura/Fa/P_1",
    "Faktura/Fa/P_2",
    "Faktura/Fa/P_6",
    "Faktura/Fa/P_13_",
    "Faktura/Fa/P_14_",
    "Faktura/Fa/P_15",
    "Faktura/Fa/FaWiersz/P_8B",
    "Faktura/Fa/FaWiersz/P_11",
    "Faktura/Fa/FaWiersz/P_7",
    "Faktura/Fa/FaWiersz/P_9A",
    "Faktura/Fa/FaWiersz/NrWierszaFa",
    "Faktura/Fa/FaWiersz/KursWaluty",
    "Faktura/Fa/Platnosc/TerminPlatnosci/Termin",
    "Faktura/Fa/Rozliczenie/DoZaplaty",
];

/// Every text leaf and attribute as `(path, value)`; namespaces are dropped, repeated elements share a path.
pub fn leaves(xml: &str) -> Result<Vec<(String, String)>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<String> = Vec::new();
    let mut out = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| format!("XML error at {}: {e}", reader.buffer_position()))?
        {
            Event::Start(e) => {
                stack.push(local(e.name().as_ref()));
                attributes(&e, &stack, &mut out)?;
            }
            Event::Empty(e) => {
                stack.push(local(e.name().as_ref()));
                attributes(&e, &stack, &mut out)?;
                out.push((stack.join("/"), String::new()));
                stack.pop();
            }
            Event::Text(t) => {
                let text = t.unescape().map_err(|e| e.to_string())?.trim().to_string();
                if !text.is_empty() {
                    out.push((stack.join("/"), text));
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn attributes(
    e: &quick_xml::events::BytesStart,
    stack: &[String],
    out: &mut Vec<(String, String)>,
) -> Result<(), String> {
    for attr in e.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = local(attr.key.as_ref());
        if key.starts_with("xmlns") || attr.key.as_ref().starts_with(b"xmlns") {
            continue;
        }
        let value = attr.unescape_value().map_err(|e| e.to_string())?;
        out.push((format!("{}@{key}", stack.join("/")), value.into_owned()));
    }
    Ok(())
}

fn local(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':').next().unwrap_or_default().to_string()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    /// In the reference, absent from ours: possibly something we should emit too.
    pub missing: Vec<String>,
    /// In ours, absent from the reference.
    pub extra: Vec<String>,
    /// Same path, different value, on a field that normally stays the same month to month.
    pub different: Vec<(String, String, String)>,
    /// Changed as expected (dates, number, amounts).
    pub varying: Vec<(String, String, String)>,
}

impl Comparison {
    pub fn is_consistent(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty() && self.different.is_empty()
    }

    pub fn render(&self) -> String {
        let mut out = Vec::new();
        if self.is_consistent() {
            out.push("same structure and same fixed values as the reference".to_string());
        }
        if !self.missing.is_empty() {
            out.push("only in the reference (consider adding):".into());
            out.extend(self.missing.iter().map(|p| format!("  - {p}")));
        }
        if !self.extra.is_empty() {
            out.push("only in ours:".into());
            out.extend(self.extra.iter().map(|p| format!("  + {p}")));
        }
        if !self.different.is_empty() {
            out.push("different values on fields that usually stay the same:".into());
            out.extend(
                self.different
                    .iter()
                    .map(|(p, ours, theirs)| format!("  ~ {p}: ours '{ours}', reference '{theirs}'")),
            );
        }
        if !self.varying.is_empty() {
            out.push(format!(
                "{} per-invoice values differ as expected (dates, number, amounts)",
                self.varying.len()
            ));
        }
        out.join("\n")
    }
}

pub fn compare(ours: &str, reference: &str) -> Result<Comparison, String> {
    let ours = grouped(&leaves(ours)?);
    let theirs = grouped(&leaves(reference)?);
    let paths: BTreeSet<&String> = ours.keys().chain(theirs.keys()).collect();
    let mut c = Comparison::default();
    for path in paths {
        match (ours.get(path), theirs.get(path)) {
            (Some(_), None) => c.extra.push(path.clone()),
            (None, Some(_)) => c.missing.push(path.clone()),
            (Some(a), Some(b)) if a != b => {
                let entry = (path.clone(), a.join(" | "), b.join(" | "));
                if VARYING.iter().any(|v| path.starts_with(v)) {
                    c.varying.push(entry);
                } else {
                    c.different.push(entry);
                }
            }
            _ => {}
        }
    }
    Ok(c)
}

fn grouped(leaves: &[(String, String)]) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, value) in leaves {
        map.entry(path.clone()).or_default().push(value.clone());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::tests::sample;
    use crate::xml::fa3;

    #[test]
    fn leaves_include_attributes_without_namespaces() {
        let l = leaves(r#"<a:Faktura xmlns:a="x"><a:K kod="FA (3)">FA</a:K><E/></a:Faktura>"#).unwrap();
        assert_eq!(
            l,
            vec![
                ("Faktura/K@kod".into(), "FA (3)".into()),
                ("Faktura/K".into(), "FA".into()),
                ("Faktura/E".into(), String::new()),
            ]
        );
    }

    #[test]
    fn next_month_is_consistent_with_this_month() {
        let mut next = sample();
        next.number = "1/10/2026".into();
        next.issue_date = jiff::civil::date(2026, 10, 31);
        next.lines[0].quantity = crate::money::Quantity::parse("160").unwrap();
        let c = compare(&fa3(&next), &fa3(&sample())).unwrap();
        assert!(c.is_consistent(), "{}", c.render());
        assert!(c.varying.len() >= 4, "{c:?}");
    }

    #[test]
    fn reports_structure_and_fixed_value_differences() {
        let ours = fa3(&sample());
        let reference = ours
            .replace("<GV>2</GV>", "<GV>2</GV>\n<NrKlienta>7</NrKlienta>")
            .replace("<P_12>23</P_12>", "<P_12>8</P_12>");
        let c = compare(&ours, &reference).unwrap();
        assert_eq!(c.missing, vec!["Faktura/Podmiot2/NrKlienta".to_string()]);
        assert_eq!(c.different[0].0, "Faktura/Fa/FaWiersz/P_12");
        assert!(c.render().contains("consider adding"));
    }
}
