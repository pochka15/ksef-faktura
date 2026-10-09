//! Everything the app keeps, under `~/.config/ksef` (or `$KSEF_HOME`):
//!
//! ```text
//! config.json
//! invoices/<1-09-2026>/invoice.json   the invoice as generated (source of truth for re-renders and sends)
//! invoices/<1-09-2026>/invoice.xml    FA(3) XML exactly as it would be sent
//! invoices/<1-09-2026>/sent-<env>.json, upo-<env>.xml   after a send
//! fetched/<env>/<ksef-number>.xml     invoices downloaded from KSeF, for `compare`
//! ```

use crate::invoice::{self, Invoice};
use crate::ksef::Env;
use crate::ksef::client::Sent;
use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub struct Store {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Stored {
    pub dir: PathBuf,
    pub invoice: Invoice,
}

impl Stored {
    pub fn xml_path(&self) -> PathBuf {
        self.dir.join("invoice.xml")
    }

    pub fn xml(&self) -> Result<String, String> {
        read(&self.xml_path())
    }

    pub fn sent(&self, env: Env) -> Option<SentRecord> {
        let text = std::fs::read_to_string(self.dir.join(format!("sent-{env}.json"))).ok()?;
        serde_json::from_str(&text).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentRecord {
    pub ksef_number: String,
    pub session_reference: String,
    pub invoice_reference: String,
    pub sent_at: Timestamp,
    /// Invoice number as sent (test sends add a suffix so they never collide).
    pub sent_number: String,
}

impl Store {
    pub fn new(root: PathBuf) -> Store {
        Store { root }
    }

    /// `$KSEF_HOME`, else `~/.config/ksef`.
    pub fn locate(home: &Path) -> Store {
        match std::env::var_os("KSEF_HOME") {
            Some(dir) => Store::new(PathBuf::from(dir)),
            None => Store::new(home.join(".config").join("ksef")),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn fetched_dir(&self, env: Env) -> PathBuf {
        self.root.join("fetched").join(env.name())
    }

    /// Oldest first.
    pub fn list(&self) -> Result<Vec<Stored>, String> {
        let dir = self.root.join("invoices");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path().join("invoice.json");
            if path.exists() {
                let invoice: Invoice =
                    serde_json::from_str(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))?;
                out.push(Stored {
                    dir: entry.path(),
                    invoice,
                });
            }
        }
        out.sort_by_key(|s| (s.invoice.issue_date, s.invoice.created_at));
        Ok(out)
    }

    /// By number (`1/09/2026` or `1-09-2026`), or the latest when `None`.
    pub fn find(&self, number: Option<&str>) -> Result<Stored, String> {
        let all = self.list()?;
        match number {
            None => all.into_iter().last().ok_or_else(|| "no invoices yet".into()),
            Some(n) => all
                .into_iter()
                .find(|s| s.invoice.number == n.trim() || s.invoice.slug() == invoice::slug(n))
                .ok_or_else(|| format!("no invoice '{n}'")),
        }
    }

    /// Next `{n}/{mm}/{yyyy}` for the month of `issue_date`.
    pub fn next_number(&self, issue_date: Date) -> Result<String, String> {
        let taken: Vec<String> = self.list()?.into_iter().map(|s| s.invoice.number).collect();
        let mut n = 1;
        loop {
            let candidate = invoice::number_for(n, issue_date);
            if !taken.contains(&candidate) {
                return Ok(candidate);
            }
            n += 1;
        }
    }

    /// Writes `invoice.json` + `invoice.xml`. Re-generating is fine until it reached KSeF demo/prod.
    pub fn save(&self, invoice: &Invoice, xml: &str) -> Result<Stored, String> {
        let dir = self.root.join("invoices").join(invoice.slug());
        let existing = Stored {
            dir: dir.clone(),
            invoice: invoice.clone(),
        };
        for env in [Env::Prod, Env::Demo] {
            if let Some(sent) = existing.sent(env) {
                return Err(format!(
                    "{} is already in KSeF {env} as {}; issue a correction instead of changing it",
                    invoice.number, sent.ksef_number
                ));
            }
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let _ = std::fs::remove_file(dir.join("sent-test.json"));
        let json = serde_json::to_string_pretty(invoice).map_err(|e| e.to_string())? + "\n";
        write(&dir.join("invoice.json"), json.as_bytes())?;
        write(&dir.join("invoice.xml"), xml.as_bytes())?;
        Ok(existing)
    }

    pub fn record_sent(
        &self,
        stored: &Stored,
        env: Env,
        sent: &Sent,
        sent_number: &str,
        at: Timestamp,
    ) -> Result<SentRecord, String> {
        let record = SentRecord {
            ksef_number: sent.ksef_number.clone(),
            session_reference: sent.session_reference.clone(),
            invoice_reference: sent.invoice_reference.clone(),
            sent_at: at,
            sent_number: sent_number.to_string(),
        };
        let json = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())? + "\n";
        write(&stored.dir.join(format!("sent-{env}.json")), json.as_bytes())?;
        if let Some(upo) = &sent.upo {
            write(&stored.dir.join(format!("upo-{env}.xml")), upo.as_bytes())?;
        }
        Ok(record)
    }

    /// Most recently downloaded invoice XML of any environment.
    pub fn latest_fetched(&self) -> Option<PathBuf> {
        Env::ALL
            .iter()
            .filter_map(|env| std::fs::read_dir(self.fetched_dir(*env)).ok())
            .flat_map(|entries| entries.flatten())
            .filter(|e| e.path().extension().is_some_and(|x| x == "xml"))
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
            .map(|e| e.path())
    }
}

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::invoice::tests::sample;

    /// A fresh store in a unique temp directory.
    pub fn temp_store() -> Store {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ksef-store-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Store::new(dir)
    }

    fn sent(n: &str) -> Sent {
        Sent {
            ksef_number: n.into(),
            session_reference: "S".into(),
            invoice_reference: "I".into(),
            upo: Some("<UPO/>".into()),
        }
    }

    #[test]
    fn saves_lists_and_numbers() {
        let store = temp_store();
        assert!(store.find(None).is_err());
        let sept = jiff::civil::date(2026, 9, 30);
        assert_eq!(store.next_number(sept).unwrap(), "1/09/2026");
        store.save(&sample(), "<xml/>").unwrap();
        assert_eq!(store.next_number(sept).unwrap(), "2/09/2026");
        assert_eq!(
            store.next_number(jiff::civil::date(2026, 10, 1)).unwrap(),
            "1/10/2026"
        );
        let found = store.find(Some("1-09-2026")).unwrap();
        assert_eq!(found.invoice, sample());
        assert_eq!(found.xml().unwrap(), "<xml/>");
        assert_eq!(store.find(None).unwrap().invoice.number, "1/09/2026");
    }

    #[test]
    fn a_prod_invoice_cannot_be_overwritten() {
        let store = temp_store();
        let stored = store.save(&sample(), "<xml/>").unwrap();
        let at = "2026-10-01T00:00:00Z".parse().unwrap();
        store
            .record_sent(&stored, Env::Test, &sent("T"), "1/09/2026-T1", at)
            .unwrap();
        assert_eq!(stored.sent(Env::Test).unwrap().ksef_number, "T");
        store.save(&sample(), "<xml/>").unwrap();
        assert!(
            stored.sent(Env::Test).is_none(),
            "a regenerated invoice forgets test sends"
        );

        store
            .record_sent(&stored, Env::Prod, &sent("P"), "1/09/2026", at)
            .unwrap();
        assert!(stored.dir.join("upo-prod.xml").exists());
        let err = store.save(&sample(), "<xml/>").unwrap_err();
        assert!(err.contains("already in KSeF prod as P"), "{err}");
    }
}
