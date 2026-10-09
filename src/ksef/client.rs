//! The KSeF flows: token authentication, sending one invoice in an interactive session (with its UPO),
//! listing issued invoices and downloading their XML. Every call goes through `Transport`.

use super::Env;
use super::crypto;
use super::http::{Request, Response, Transport};
use crate::xml;
use rsa::RsaPublicKey;
use serde_json::{Value, json};
use std::time::Duration;

const POLL_ATTEMPTS: usize = 90;

pub struct Client<T: Transport> {
    http: T,
    env: Env,
    access_token: Option<String>,
    /// Waits between status polls; tests pass a no-op.
    pause: fn(Duration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub ksef_number: String,
    pub session_reference: String,
    pub invoice_reference: String,
    /// The official receipt (UPO) for this invoice; `None` if KSeF had not produced it yet.
    pub upo: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvoiceMeta {
    pub ksef_number: String,
    pub invoice_number: String,
    pub issue_date: String,
    pub buyer: String,
    pub gross: f64,
}

impl<T: Transport> Client<T> {
    pub fn new(http: T, env: Env) -> Self {
        Client {
            http,
            env,
            access_token: None,
            pause: std::thread::sleep,
        }
    }

    pub fn with_pause(mut self, pause: fn(Duration)) -> Self {
        self.pause = pause;
        self
    }

    pub fn transport(&self) -> &T {
        &self.http
    }

    /// Token login for the given NIP context; on success later calls are authorised.
    pub fn authenticate(&mut self, nip: &str, ksef_token: &str) -> Result<(), String> {
        let (key, key_id) = self.public_key("KsefTokenEncryption")?;
        let challenge = self.call("POST", "/auth/challenge", None, None)?;
        let challenge: Value = parse(&challenge, "auth challenge")?;
        let timestamp = challenge["timestampMs"]
            .as_i64()
            .ok_or("auth challenge: no timestampMs")?;
        let secret = format!("{}|{timestamp}", ksef_token.trim());
        let body = json!({
            "challenge": challenge["challenge"],
            "contextIdentifier": {"type": "Nip", "value": nip},
            "encryptedToken": crypto::b64(&crypto::rsa_encrypt(&key, secret.as_bytes())?),
            "publicKeyId": key_id,
        });
        let init: Value = parse(
            &self.call("POST", "/auth/ksef-token", None, Some(body))?,
            "token authentication",
        )?;
        let reference = text(&init["referenceNumber"], "authentication reference")?;
        let auth_token = text(&init["authenticationToken"]["token"], "authentication token")?;

        let status = self.poll(&format!("/auth/{reference}"), Some(&auth_token), &[100])?;
        if status["status"]["code"] != 200 {
            return Err(format!(
                "KSeF refused the login: {}",
                describe_status(&status["status"])
            ));
        }
        let tokens: Value = parse(
            &self.call("POST", "/auth/token/redeem", Some(&auth_token), None)?,
            "token redeem",
        )?;
        self.access_token = Some(text(&tokens["accessToken"]["token"], "access token")?);
        Ok(())
    }

    /// Ends this login on KSeF's side so the refresh token cannot be reused. Best effort.
    pub fn logout(&mut self) {
        if let Some(token) = self.access_token.take() {
            let _ = self.call("DELETE", "/auth/sessions/current", Some(&token), None);
        }
    }

    /// Opens an interactive session, sends one FA(3) invoice, waits for its KSeF number, closes the session
    /// and fetches the UPO. The session is closed even when the invoice is rejected.
    pub fn send_invoice(&self, invoice_xml: &[u8]) -> Result<Sent, String> {
        let access = self.access()?;
        let (key, key_id) = self.public_key("SymmetricKeyEncryption")?;
        let (aes_key, iv) = (crypto::random_bytes::<32>(), crypto::random_bytes::<16>());
        let open = json!({
            "formCode": {
                "systemCode": xml::SYSTEM_CODE,
                "schemaVersion": xml::SCHEMA_VERSION,
                "value": xml::FORM_VALUE,
            },
            "encryption": {
                "encryptedSymmetricKey": crypto::b64(&crypto::rsa_encrypt(&key, &aes_key)?),
                "initializationVector": crypto::b64(&iv),
                "publicKeyId": key_id,
            },
        });
        let session: Value = parse(
            &self.call("POST", "/sessions/online", Some(access), Some(open))?,
            "open session",
        )?;
        let session_ref = text(&session["referenceNumber"], "session reference")?;
        let result = self.send_in_session(access, &session_ref, &aes_key, &iv, invoice_xml);
        let closed = self.call(
            "POST",
            &format!("/sessions/online/{session_ref}/close"),
            Some(access),
            None,
        );
        let (invoice_ref, ksef_number) = result?;
        if let Err(e) = closed.and_then(|r| expect_success(&r, "close session").map(|_| ())) {
            eprintln!("warning: {e} (the invoice itself was accepted)");
        }
        let upo = self.fetch_upo(access, &session_ref, &ksef_number);
        Ok(Sent {
            ksef_number,
            session_reference: session_ref,
            invoice_reference: invoice_ref,
            upo,
        })
    }

    fn send_in_session(
        &self,
        access: &str,
        session_ref: &str,
        aes_key: &[u8; 32],
        iv: &[u8; 16],
        invoice_xml: &[u8],
    ) -> Result<(String, String), String> {
        let encrypted = crypto::aes_encrypt(aes_key, iv, invoice_xml);
        let body = json!({
            "invoiceHash": crypto::sha256_b64(invoice_xml),
            "invoiceSize": invoice_xml.len(),
            "encryptedInvoiceHash": crypto::sha256_b64(&encrypted),
            "encryptedInvoiceSize": encrypted.len(),
            "encryptedInvoiceContent": crypto::b64(&encrypted),
        });
        let sent: Value = parse(
            &self.call(
                "POST",
                &format!("/sessions/online/{session_ref}/invoices"),
                Some(access),
                Some(body),
            )?,
            "send invoice",
        )?;
        let invoice_ref = text(&sent["referenceNumber"], "invoice reference")?;
        let status = self.poll(
            &format!("/sessions/{session_ref}/invoices/{invoice_ref}"),
            Some(access),
            &[100, 150],
        )?;
        if status["status"]["code"] != 200 {
            return Err(format!(
                "KSeF rejected the invoice: {}",
                describe_status(&status["status"])
            ));
        }
        let ksef_number = text(&status["ksefNumber"], "KSeF number")?;
        Ok((invoice_ref, ksef_number))
    }

    fn fetch_upo(&self, access: &str, session_ref: &str, ksef_number: &str) -> Option<String> {
        let path = format!("/sessions/{session_ref}/invoices/ksef/{ksef_number}/upo");
        for attempt in 0..5 {
            if attempt > 0 {
                (self.pause)(Duration::from_secs(2));
            }
            if let Ok(response) = self.call("GET", &path, Some(access), None)
                && response.is_success()
            {
                return Some(response.body);
            }
        }
        None
    }

    /// Invoices this NIP issued (`Subject1`) with an issue date in the range, newest first.
    pub fn issued_invoices(&self, from: &str, to: &str) -> Result<Vec<InvoiceMeta>, String> {
        let access = self.access()?;
        let mut out = Vec::new();
        for page in 0.. {
            let body = json!({
                "subjectType": "Subject1",
                "dateRange": {"dateType": "Issue", "from": from, "to": to},
            });
            let path = format!("/invoices/query/metadata?sortOrder=Desc&pageSize=100&pageOffset={page}");
            let response: Value = parse(
                &self.call("POST", &path, Some(access), Some(body))?,
                "list invoices",
            )?;
            for meta in response["invoices"].as_array().into_iter().flatten() {
                out.push(InvoiceMeta {
                    ksef_number: text(&meta["ksefNumber"], "ksefNumber")?,
                    invoice_number: meta["invoiceNumber"].as_str().unwrap_or("").into(),
                    issue_date: meta["issueDate"].as_str().unwrap_or("").into(),
                    buyer: meta["buyer"]["name"].as_str().unwrap_or("").into(),
                    gross: meta["grossAmount"].as_f64().unwrap_or(0.0),
                });
            }
            if response["hasMore"] != true {
                break;
            }
        }
        Ok(out)
    }

    pub fn download(&self, ksef_number: &str) -> Result<String, String> {
        let response = self.call(
            "GET",
            &format!("/invoices/ksef/{ksef_number}"),
            Some(self.access()?),
            None,
        )?;
        expect_success(&response, &format!("download {ksef_number}")).map(|r| r.body.clone())
    }

    fn access(&self) -> Result<&str, String> {
        self.access_token
            .as_deref()
            .ok_or_else(|| "not authenticated".to_string())
    }

    /// The Ministry key for `usage`, as (key, publicKeyId). Picked fresh each time: KSeF rotates them.
    fn public_key(&self, usage: &str) -> Result<(RsaPublicKey, String), String> {
        let certs: Value = parse(
            &self.call("GET", "/security/public-key-certificates", None, None)?,
            "public keys",
        )?;
        let cert = certs
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| {
                c["usage"]
                    .as_array()
                    .is_some_and(|u| u.iter().any(|u| u == usage))
            })
            .max_by_key(|c| c["validFrom"].as_str().unwrap_or("").to_string())
            .ok_or_else(|| format!("KSeF published no {usage} key"))?;
        let key = crypto::public_key_from_certificate(&text(&cert["certificate"], "certificate")?)?;
        Ok((key, text(&cert["publicKeyId"], "publicKeyId")?))
    }

    /// Repeats a GET while `status.code` is one of `pending`.
    fn poll(&self, path: &str, bearer: Option<&str>, pending: &[i64]) -> Result<Value, String> {
        for attempt in 0..POLL_ATTEMPTS {
            if attempt > 0 {
                (self.pause)(Duration::from_secs(1));
            }
            let status: Value = parse(&self.call("GET", path, bearer, None)?, "status")?;
            let code = status["status"]["code"].as_i64().unwrap_or(0);
            if !pending.contains(&code) {
                return Ok(status);
            }
        }
        Err(format!(
            "KSeF still processing after {POLL_ATTEMPTS} checks ({path}); try again later"
        ))
    }

    fn call(
        &self,
        method: &'static str,
        path: &str,
        bearer: Option<&str>,
        json: Option<Value>,
    ) -> Result<Response, String> {
        self.http.send(&Request {
            method,
            url: format!("{}{path}", self.env.api_url()),
            bearer: bearer.map(String::from),
            json,
        })
    }
}

fn expect_success<'r>(response: &'r Response, what: &str) -> Result<&'r Response, String> {
    if response.is_success() {
        Ok(response)
    } else {
        Err(format!(
            "{what}: HTTP {} {}",
            response.status,
            describe_error(&response.body)
        ))
    }
}

fn parse(response: &Response, what: &str) -> Result<Value, String> {
    expect_success(response, what)?;
    serde_json::from_str(&response.body).map_err(|e| format!("{what}: unexpected response ({e})"))
}

fn text(value: &Value, what: &str) -> Result<String, String> {
    value
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("KSeF response has no {what}"))
}

/// `{code, description, details}` -> `450 Błąd weryfikacji semantyki (detail; detail)`.
fn describe_status(status: &Value) -> String {
    let mut out = format!(
        "{} {}",
        status["code"],
        status["description"].as_str().unwrap_or("")
    );
    let details: Vec<&str> = status["details"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !details.is_empty() {
        out.push_str(&format!(" ({})", details.join("; ")));
    }
    if let Some(ext) = status["extensions"].as_object() {
        let pairs: Vec<String> = ext.iter().map(|(k, v)| format!("{k}={v}")).collect();
        out.push_str(&format!(" [{}]", pairs.join(", ")));
    }
    out
}

/// KSeF errors come as `{"exception": {"exceptionDetailList": [...]}}` or RFC 7807 problem details.
fn describe_error(body: &str) -> String {
    let Ok(json) = serde_json::from_str::<Value>(body) else {
        return body.chars().take(300).collect();
    };
    if let Some(list) = json["exception"]["exceptionDetailList"].as_array() {
        let parts: Vec<String> = list
            .iter()
            .map(|d| {
                let details: Vec<&str> = d["details"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                format!(
                    "{} {} {}",
                    d["exceptionCode"],
                    d["exceptionDescription"].as_str().unwrap_or(""),
                    details.join("; ")
                )
                .trim()
                .to_string()
            })
            .collect();
        return parts.join(" | ");
    }
    let title = json["title"].as_str().unwrap_or("");
    let detail = json["detail"].as_str().unwrap_or("");
    let text = format!("{title} {detail}").trim().to_string();
    if text.is_empty() {
        body.chars().take(300).collect()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ksef::crypto::tests::{TEST_CERT_B64, aes_decrypt, rsa_decrypt};
    use crate::ksef::http::tests::FakeTransport;

    fn keys(usage: &str) -> String {
        json!([
            {"certificate": "MAo=", "publicKeyId": "OTHER", "usage": ["SomethingElse"], "validFrom": "2025-01-01"},
            {"certificate": TEST_CERT_B64.trim(), "publicKeyId": "KEY-ID", "usage": [usage],
             "validFrom": "2025-09-29T06:03:18+00:00", "validTo": "2027-09-29T06:03:18+00:00", "certificateId": "c"}
        ])
        .to_string()
    }

    fn client(replies: Vec<(u16, &str)>) -> Client<FakeTransport> {
        Client::new(FakeTransport::new(replies), Env::Test).with_pause(|_| {})
    }

    fn authenticated(replies: Vec<(u16, &str)>) -> Client<FakeTransport> {
        let mut c = client(replies);
        c.access_token = Some("ACCESS".into());
        c
    }

    fn body(c: &Client<FakeTransport>, i: usize) -> Value {
        c.transport().requests.borrow()[i].json.clone().unwrap()
    }

    #[test]
    fn token_login_encrypts_token_and_timestamp() {
        let k = keys("KsefTokenEncryption");
        let mut c = client(vec![
            (200, &k),
            (
                200,
                r#"{"challenge":"CH","timestampMs":1760000000000,"timestamp":"x","clientIp":"1.1.1.1"}"#,
            ),
            (
                202,
                r#"{"referenceNumber":"AUTH-REF","authenticationToken":{"token":"AUTH","validUntil":"x"}}"#,
            ),
            (200, r#"{"status":{"code":100,"description":"W toku"}}"#),
            (200, r#"{"status":{"code":200,"description":"OK"}}"#),
            (
                200,
                r#"{"accessToken":{"token":"ACCESS","validUntil":"x"},"refreshToken":{"token":"R","validUntil":"x"}}"#,
            ),
            (204, ""),
        ]);
        c.authenticate("5265877635", " the-token ").unwrap();
        assert_eq!(
            c.transport().paths(),
            vec![
                "GET /security/public-key-certificates",
                "POST /auth/challenge",
                "POST /auth/ksef-token",
                "GET /auth/AUTH-REF",
                "GET /auth/AUTH-REF",
                "POST /auth/token/redeem",
            ]
        );
        let login = body(&c, 2);
        assert_eq!(login["challenge"], "CH");
        assert_eq!(
            login["contextIdentifier"],
            json!({"type": "Nip", "value": "5265877635"})
        );
        assert_eq!(login["publicKeyId"], "KEY-ID");
        let secret = rsa_decrypt(&crypto::unb64(login["encryptedToken"].as_str().unwrap()).unwrap());
        assert_eq!(secret, b"the-token|1760000000000");
        assert_eq!(c.transport().requests.borrow()[3].bearer.as_deref(), Some("AUTH"));
        assert_eq!(c.access().unwrap(), "ACCESS");
        c.logout();
        assert_eq!(c.transport().paths()[6], "DELETE /auth/sessions/current");
    }

    #[test]
    fn refused_login_explains_why() {
        let k = keys("KsefTokenEncryption");
        let mut c = client(vec![
            (200, &k),
            (200, r#"{"challenge":"CH","timestampMs":1}"#),
            (
                202,
                r#"{"referenceNumber":"R","authenticationToken":{"token":"A"}}"#,
            ),
            (
                200,
                r#"{"status":{"code":450,"description":"Błędny token","details":["Token unieważniony"]}}"#,
            ),
        ]);
        let err = c.authenticate("5265877635", "t").unwrap_err();
        assert!(err.contains("450 Błędny token (Token unieważniony)"), "{err}");
    }

    #[test]
    fn sends_an_encrypted_invoice_and_collects_the_upo() {
        let k = keys("SymmetricKeyEncryption");
        let c = authenticated(vec![
            (200, &k),
            (201, r#"{"referenceNumber":"S1","validUntil":"x"}"#),
            (202, r#"{"referenceNumber":"I1"}"#),
            (
                200,
                r#"{"status":{"code":150,"description":"Trwa przetwarzanie"}}"#,
            ),
            (
                200,
                r#"{"status":{"code":200,"description":"Sukces"},"ksefNumber":"5265877635-20260930-0102030405A1-B2"}"#,
            ),
            (204, ""),
            (200, "<UPO/>"),
        ]);
        let xml = "<Faktura>zażółć</Faktura>".as_bytes();
        let sent = c.send_invoice(xml).unwrap();
        assert_eq!(sent.ksef_number, "5265877635-20260930-0102030405A1-B2");
        assert_eq!(sent.upo.as_deref(), Some("<UPO/>"));
        assert_eq!(
            c.transport().paths()[5..],
            [
                "POST /sessions/online/S1/close",
                "GET /sessions/S1/invoices/ksef/5265877635-20260930-0102030405A1-B2/upo"
            ]
        );

        let open = body(&c, 1);
        assert_eq!(
            open["formCode"],
            json!({"systemCode": "FA (3)", "schemaVersion": "1-0E", "value": "FA"})
        );
        let key = rsa_decrypt(
            &crypto::unb64(open["encryption"]["encryptedSymmetricKey"].as_str().unwrap()).unwrap(),
        );
        let iv = crypto::unb64(open["encryption"]["initializationVector"].as_str().unwrap()).unwrap();
        let send = body(&c, 2);
        let encrypted = crypto::unb64(send["encryptedInvoiceContent"].as_str().unwrap()).unwrap();
        assert_eq!(aes_decrypt(&key, &iv, &encrypted), xml);
        assert_eq!(send["invoiceHash"], crypto::sha256_b64(xml));
        assert_eq!(send["invoiceSize"], xml.len());
        assert_eq!(send["encryptedInvoiceHash"], crypto::sha256_b64(&encrypted));
    }

    #[test]
    fn a_rejected_invoice_still_closes_the_session() {
        let k = keys("SymmetricKeyEncryption");
        let c = authenticated(vec![
            (200, &k),
            (201, r#"{"referenceNumber":"S1"}"#),
            (202, r#"{"referenceNumber":"I1"}"#),
            (
                200,
                r#"{"status":{"code":440,"description":"Duplikat faktury","extensions":{"originalKsefNumber":"X"}}}"#,
            ),
            (204, ""),
        ]);
        let err = c.send_invoice(b"<Faktura/>").unwrap_err();
        assert!(err.contains("440 Duplikat faktury"), "{err}");
        assert!(err.contains("originalKsefNumber=\"X\""), "{err}");
        assert_eq!(
            c.transport().paths().last().unwrap(),
            "POST /sessions/online/S1/close"
        );
    }

    #[test]
    fn lists_issued_invoices_across_pages() {
        let page = |more: bool, n: &str| {
            json!({"hasMore": more, "isTruncated": false, "invoices": [
                {"ksefNumber": n, "invoiceNumber": "1/09/2026", "issueDate": "2026-09-30",
                 "buyer": {"name": "Klient"}, "grossAmount": 20664.00}
            ]})
            .to_string()
        };
        let (p1, p2) = (page(true, "A"), page(false, "B"));
        let c = authenticated(vec![(200, &p1), (200, &p2)]);
        let list = c
            .issued_invoices("2026-07-01T00:00:00Z", "2026-10-08T00:00:00Z")
            .unwrap();
        assert_eq!(
            list.iter().map(|m| m.ksef_number.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert!(c.transport().paths()[1].ends_with("pageOffset=1"));
        assert_eq!(body(&c, 0)["subjectType"], "Subject1");
    }

    #[test]
    fn http_errors_show_ksef_exception_details() {
        let c = authenticated(vec![(
            400,
            r#"{"exception":{"exceptionDetailList":[{"exceptionCode":21405,"exceptionDescription":"Błąd walidacji","details":["P_2 empty"]}]}}"#,
        )]);
        let err = c.download("X").unwrap_err();
        assert_eq!(err, "download X: HTTP 400 21405 Błąd walidacji P_2 empty");
    }
}
