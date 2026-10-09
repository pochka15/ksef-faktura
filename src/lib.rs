//! Polish VAT invoices: a PDF for people, FA(3) XML for KSeF, and an explicit, validated send.
//!
//! Layers, bottom up: `money` / `ids` / `words` / `i18n` (values), `invoice` (model), `xml` / `pdf` (outputs),
//! `validate` / `inspect` (checks), `ksef` (API client), `config` / `store` / `keychain` (local state),
//! `draft` (invoice-building commands), `app` (operations), `api` + `server` (the browser page), `shell`
//! (terminal menu) and `job` (JSON mode).

pub mod api;
pub mod app;
pub mod config;
pub mod countries;
pub mod draft;
pub mod editor;
pub mod i18n;
pub mod ids;
pub mod inspect;
pub mod invoice;
pub mod job;
pub mod keychain;
pub mod ksef;
pub mod money;
pub mod nbp;
pub mod pdf;
pub mod server;
pub mod shell;
pub mod store;
pub mod validate;
pub mod words;
pub mod xml;
