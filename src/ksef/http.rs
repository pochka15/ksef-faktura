//! HTTP transport behind a trait so the KSeF flows are unit-tested against canned responses.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub bearer: Option<String>,
    pub json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

pub trait Transport {
    /// `Err` only for transport failures (DNS, TLS, timeout); HTTP errors come back as a `Response`.
    fn send(&self, request: &Request) -> Result<Response, String>;
}

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("ksef-cli/", env!("CARGO_PKG_VERSION")))
            .build();
        UreqTransport { agent }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for UreqTransport {
    fn send(&self, request: &Request) -> Result<Response, String> {
        let mut call = self
            .agent
            .request(request.method, &request.url)
            .set("Accept", "application/json, application/xml");
        if let Some(token) = &request.bearer {
            call = call.set("Authorization", &format!("Bearer {token}"));
        }
        let result = match &request.json {
            Some(body) => call
                .set("Content-Type", "application/json")
                .send_string(&body.to_string()),
            None => call.call(),
        };
        let response = match result {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(e) => return Err(format!("{} {}: {e}", request.method, request.url)),
        };
        let status = response.status();
        let body = response
            .into_string()
            .map_err(|e| format!("reading response: {e}"))?;
        Ok(Response { status, body })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// Replays queued responses in order and records every request.
    #[derive(Default)]
    pub struct FakeTransport {
        pub replies: RefCell<VecDeque<Response>>,
        pub requests: RefCell<Vec<Request>>,
    }

    impl FakeTransport {
        pub fn new(replies: Vec<(u16, &str)>) -> Self {
            FakeTransport {
                replies: RefCell::new(
                    replies
                        .into_iter()
                        .map(|(status, body)| Response {
                            status,
                            body: body.to_string(),
                        })
                        .collect(),
                ),
                requests: RefCell::default(),
            }
        }

        pub fn paths(&self) -> Vec<String> {
            self.requests
                .borrow()
                .iter()
                .map(|r| format!("{} {}", r.method, r.url.split("/v2").nth(1).unwrap_or(&r.url)))
                .collect()
        }
    }

    impl Transport for FakeTransport {
        fn send(&self, request: &Request) -> Result<Response, String> {
            self.requests.borrow_mut().push(request.clone());
            self.replies
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| format!("unexpected request {} {}", request.method, request.url))
        }
    }
}
