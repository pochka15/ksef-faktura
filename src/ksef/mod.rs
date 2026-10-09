//! KSeF API 2.0: environments, transport, crypto and the few flows this app uses.

pub mod client;
pub mod crypto;
pub mod http;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Env {
    /// Shared, public test environment: fake NIPs, anyone can log in, data is wiped.
    #[default]
    Test,
    /// Pre-production: real login (Trusted Profile / qualified signature), no legal effect.
    Demo,
    /// The real thing: invoices sent here are legally issued.
    Prod,
}

impl Env {
    pub const ALL: [Env; 3] = [Env::Test, Env::Demo, Env::Prod];

    pub fn parse(text: &str) -> Result<Env, String> {
        match text.trim().to_lowercase().as_str() {
            "test" | "te" => Ok(Env::Test),
            "demo" => Ok(Env::Demo),
            "prod" | "production" => Ok(Env::Prod),
            _ => Err(format!("environment '{text}': use test, demo or prod")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Env::Test => "test",
            Env::Demo => "demo",
            Env::Prod => "prod",
        }
    }

    pub fn api_url(self) -> &'static str {
        match self {
            Env::Test => "https://api-test.ksef.mf.gov.pl/v2",
            Env::Demo => "https://api-demo.ksef.mf.gov.pl/v2",
            Env::Prod => "https://api.ksef.mf.gov.pl/v2",
        }
    }

    /// The Ministry's web app ("Aplikacja Podatnika") where tokens are generated.
    pub fn web_app_url(self) -> &'static str {
        match self {
            Env::Test => "https://ap-test.ksef.mf.gov.pl",
            Env::Demo => "https://ap-demo.ksef.mf.gov.pl",
            Env::Prod => "https://ap.ksef.mf.gov.pl",
        }
    }
}

impl std::fmt::Display for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
