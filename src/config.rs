//! `config.json`: who you are (seller), whom you invoice (buyers) and a few defaults. Edited by hand.

use crate::i18n::Lang;
use crate::ids;
use crate::invoice::{self, Party, Seller};
use crate::ksef::Env;
use crate::validate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const EXAMPLE: &str = include_str!("../examples/config.example.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub seller: Seller,
    /// Buyers by a short key, e.g. `client`; the page shows them in a list.
    pub buyers: BTreeMap<String, Buyer>,
    #[serde(default)]
    pub default_buyer: Option<String>,
    #[serde(default)]
    pub language: Lang,
    #[serde(default = "default_payment_days")]
    pub payment_days: i64,
    /// Invoice currency unless the buyer has its own, e.g. `EUR` for a foreign client. Default: PLN.
    #[serde(default = "invoice::pln")]
    pub currency: String,
    /// `P_1M`, place of issue. Optional on invoices.
    #[serde(default)]
    pub place_of_issue: Option<String>,
    /// Where PDFs go. `~` works. Default: `~/Desktop`.
    #[serde(default)]
    pub output_dir: Option<String>,
    /// Command to edit files with, e.g. `vim` or `code --wait`. Default: TextEdit.
    #[serde(default)]
    pub editor: Option<String>,
    /// Removed: the environment is chosen for each send/fetch. Read only to warn; never written back.
    #[serde(default, rename = "ksef_env", skip_serializing)]
    pub legacy_ksef_env: Option<serde_json::Value>,
    /// Fake NIP you log into the public test environment with (`ksef test-nip` makes one).
    #[serde(default)]
    pub test_nip: Option<String>,
}

fn default_payment_days() -> i64 {
    7
}

/// A buyer in config.json: the party as printed on invoices, plus defaults for invoices to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buyer {
    #[serde(flatten)]
    pub party: Party,
    /// Currency of invoices to this buyer, e.g. `EUR`; default: the top-level `currency`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

impl Config {
    /// Parses, retrying with straight quotes if TextEdit's smart quotes broke the JSON. The second value is
    /// the repaired text, to be saved back.
    pub fn parse(text: &str) -> Result<(Config, Option<String>), String> {
        match serde_json::from_str(text) {
            Ok(config) => Ok((config, None)),
            Err(first) => {
                let straight = text.replace(['“', '”', '„'], "\"").replace(['‘', '’'], "'");
                match serde_json::from_str(&straight) {
                    Ok(config) if straight != text => Ok((config, Some(straight))),
                    _ => Err(format!("config.json: {first}")),
                }
            }
        }
    }

    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            format!(
                "no config at {} ({e}); run `ksef config` to create it",
                path.display()
            )
        })?;
        let (config, repaired) = Config::parse(&text)?;
        if let Some(repaired) = repaired {
            std::fs::write(path, repaired).map_err(|e| format!("writing {}: {e}", path.display()))?;
        }
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n";
        std::fs::write(path, text).map_err(|e| format!("writing {}: {e}", path.display()))
    }

    /// Typos in the config that would end up on every invoice.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let parties = std::iter::once(("seller".to_string(), &self.seller.party, false)).chain(
            self.buyers
                .iter()
                .map(|(k, b)| (format!("buyer '{k}'"), &b.party, true)),
        );
        for (who, party, buyer) in parties {
            out.extend(validate::identity(&who, party, buyer).0);
        }
        if !ids::account_is_valid(&self.seller.bank_account) {
            out.push(format!(
                "seller: bank account '{}' is not a valid Polish account number",
                self.seller.bank_account
            ));
        }
        if let Some(key) = &self.default_buyer
            && !self.buyers.contains_key(key)
        {
            out.push(format!("default_buyer '{key}' is not in buyers"));
        }
        if self.test_nip.as_deref().is_some_and(|n| !ids::nip_is_valid(n)) {
            out.push("test_nip fails the checksum; run `ksef test-nip` for a new one".into());
        }
        if self.legacy_ksef_env.is_some() {
            out.push("ksef_env is no longer used; delete that line".into());
        }
        out
    }

    pub fn buyer(&self, key: Option<&str>) -> Result<(String, Party), String> {
        let key = key
            .or(self.default_buyer.as_deref())
            .or_else(|| (self.buyers.len() == 1).then(|| self.buyers.keys().next().unwrap().as_str()))
            .ok_or_else(|| format!("which buyer? one of: {}", self.buyer_keys().join(", ")))?;
        self.buyers
            .get(key)
            .map(|b| (key.to_string(), b.party.clone()))
            .ok_or_else(|| {
                format!(
                    "no buyer '{key}' in config (have: {})",
                    self.buyer_keys().join(", ")
                )
            })
    }

    /// Currency for invoices to `key`: the buyer's own, else the default.
    pub fn currency_for(&self, key: &str) -> String {
        self.buyers
            .get(key)
            .and_then(|b| b.currency.clone())
            .unwrap_or_else(|| self.currency.clone())
    }

    pub fn buyer_keys(&self) -> Vec<String> {
        self.buyers.keys().cloned().collect()
    }

    pub fn output_dir(&self, home: &Path) -> PathBuf {
        let dir = self.output_dir.as_deref().unwrap_or("~/Desktop");
        match dir.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None if dir == "~" => home.to_path_buf(),
            None => PathBuf::from(dir),
        }
    }

    /// The NIP KSeF sees us as: a fake one on the shared test environment, the real one elsewhere.
    pub fn context_nip(&self, env: Env) -> Result<String, String> {
        match env {
            Env::Test => self
                .test_nip
                .clone()
                .ok_or_else(|| "no test_nip in config yet: run `ksef test-nip` (see tutorials/02)".into()),
            Env::Demo | Env::Prod => Ok(self.seller.party.nip.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_a_valid_config() {
        let (config, repaired) = Config::parse(EXAMPLE).unwrap();
        assert_eq!(repaired, None);
        assert_eq!(config.problems(), Vec::<String>::new());
        assert_eq!(config.payment_days, 7);
        assert_eq!(config.buyer(None).unwrap().0, "client");
    }

    #[test]
    fn an_old_ksef_env_still_loads_but_is_flagged_and_dropped_on_save() {
        let old = EXAMPLE.replacen("\"language\"", "\"ksef_env\": \"prod\",\n  \"language\"", 1);
        let (config, _) = Config::parse(&old).unwrap();
        assert!(config.problems()[0].contains("ksef_env is no longer used"));
        assert!(!serde_json::to_string(&config).unwrap().contains("ksef_env"));
    }

    #[test]
    fn repairs_textedit_smart_quotes() {
        let smart = EXAMPLE.replacen("\"language\": \"pl\"", "“language”: “en”", 1);
        let (config, repaired) = Config::parse(&smart).unwrap();
        assert_eq!(config.language, Lang::En);
        assert!(repaired.unwrap().contains("\"language\": \"en\""));
    }

    #[test]
    fn broken_json_names_the_problem() {
        let err = Config::parse("{ \"seller\": ").unwrap_err();
        assert!(err.starts_with("config.json:"), "{err}");
    }

    #[test]
    fn reports_typos() {
        let (mut config, _) = Config::parse(EXAMPLE).unwrap();
        config.buyers.get_mut("client").unwrap().party.nip = "1234567890".into();
        config.default_buyer = Some("nobody".into());
        let problems = config.problems().join("\n");
        assert!(problems.contains("buyer 'client' NIP"), "{problems}");
        assert!(problems.contains("default_buyer 'nobody'"), "{problems}");
        assert!(config.buyer(Some("x")).unwrap_err().contains("have: client"));
    }

    #[test]
    fn paths_and_contexts() {
        let (config, _) = Config::parse(EXAMPLE).unwrap();
        assert_eq!(
            config.output_dir(Path::new("/Users/me")),
            PathBuf::from("/Users/me/Desktop")
        );
        assert!(
            config
                .context_nip(Env::Test)
                .unwrap_err()
                .contains("ksef test-nip")
        );
        assert_eq!(config.context_nip(Env::Prod).unwrap(), "5265877635");
    }
}
