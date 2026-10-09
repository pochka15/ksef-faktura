//! A tiny localhost HTTP server on std threads: the page (`src/assets/index.html`, built from `web/`), JSON
//! endpoints from `api`, and PDFs. Every request must carry the random token from the start URL (so other
//! websites can't call it) and a localhost `Host` header (so DNS rebinding can't either).

use crate::api;
use crate::app::App;
use crate::keychain;
use crate::ksef::Env;
use serde_json::{Value, json};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const INDEX_HTML: &str = include_str!("assets/index.html");
const PREFERRED_PORT: u16 = 4848;
const MAX_BODY: usize = 1 << 20;

/// Loads the app for one request: config edits in the terminal show up on the next click.
pub type Loader = Arc<dyn Fn() -> Result<App, String> + Send + Sync>;

/// What the page serves: the app, and the KSeF environments it may use (only prod unless `--test-envs`).
pub struct Site {
    pub load: Loader,
    pub envs: Vec<Env>,
}

/// Starts serving in the background; returns the URL to open.
pub fn start(site: Site) -> io::Result<String> {
    let site = Arc::new(site);
    let listener =
        TcpListener::bind(("127.0.0.1", PREFERRED_PORT)).or_else(|_| TcpListener::bind(("127.0.0.1", 0)))?;
    let port = listener.local_addr()?.port();
    let token = new_token();
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (site, token) = (site.clone(), token.clone());
            thread::spawn(move || {
                let _ = handle(stream, &site, &token, port);
            });
        }
    });
    Ok(url)
}

fn new_token() -> String {
    let part = || RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", part(), part())
}

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub host: String,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: &Value) -> Self {
        Response {
            status,
            content_type: "application/json",
            body: value.to_string().into_bytes(),
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(status, &json!({ "error": message }))
    }

    fn pdf(bytes: Vec<u8>) -> Self {
        Response {
            status: 200,
            content_type: "application/pdf",
            body: bytes,
        }
    }
}

fn handle(mut stream: TcpStream, site: &Site, token: &str, port: u16) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };
    write_response(&mut stream, &route(&request, site, token, port))
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break end;
        }
        if buf.len() > 64 * 1024 {
            return Ok(None);
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut request_line = lines.next().unwrap_or("").split_whitespace();
    let (Some(method), Some(target)) = (request_line.next(), request_line.next()) else {
        return Ok(None);
    };
    let header = |name: &str| {
        lines
            .clone()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.trim().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim().to_string())
    };
    let length: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if length > MAX_BODY {
        return Ok(None);
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    Ok(Some(Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        host: header("host").unwrap_or_default(),
        body,
    }))
}

fn write_response(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)
}

pub fn route(request: &Request, site: &Site, token: &str, port: u16) -> Response {
    let host_ok = request.host == format!("127.0.0.1:{port}") || request.host == format!("localhost:{port}");
    let token_ok = request
        .query
        .split('&')
        .any(|pair| pair.strip_prefix("t=") == Some(token));
    if !host_ok || !token_ok {
        return Response::error(403, "forbidden");
    }
    if (request.method.as_str(), request.path.as_str()) == ("GET", "/") {
        return Response {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: INDEX_HTML.replacen("@@TOKEN@@", token, 1).into_bytes(),
        };
    }
    let app = match (site.load)() {
        Ok(app) => app,
        Err(e) => return Response::error(400, &format!("config.json: {e}")),
    };
    let result = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/state") => api::state(&app, &site.envs, &keychain::has_token).map(Ok),
        ("GET", path) if path.starts_with("/pdf/") => api::invoice_pdf(&app, &path["/pdf/".len()..]).map(Err),
        ("POST", path) => post(&app, &site.envs, path, &request.body),
        _ => return Response::error(404, "not found"),
    };
    match result {
        Ok(Ok(value)) => Response::json(200, &value),
        Ok(Err(pdf)) => Response::pdf(pdf),
        Err(e) => Response::error(400, &e),
    }
}

/// `Ok(json)` or `Err(pdf bytes)` on success.
type Reply = Result<Value, Vec<u8>>;

fn post(app: &App, envs: &[Env], path: &str, body: &[u8]) -> Result<Reply, String> {
    let body: Value = serde_json::from_slice(body).map_err(|e| format!("request: {e}"))?;
    let text = |name: &str| {
        body[name]
            .as_str()
            .map(String::from)
            .ok_or_else(|| format!("request: missing {name}"))
    };
    let env = || {
        let env = Env::parse(&text("env")?)?;
        if !envs.contains(&env) {
            return Err(format!(
                "KSeF {env} is off: start with `ksef --test-envs` to use it"
            ));
        }
        Ok(env)
    };
    let form = || serde_json::from_value::<api::Form>(body["form"].clone()).map_err(|e| format!("form: {e}"));
    let json = match path {
        "/api/draft" => api::draft(app, form()?)?,
        "/api/draft/pdf" => return api::draft_pdf(app, form()?).map(Err),
        "/api/save" => api::save(app, form()?)?,
        "/api/invoice" => api::invoice(app, &text("number")?)?,
        "/api/open" => api::open_pdf(app, &text("number")?)?,
        "/api/plan" => api::plan(app, &text("number")?, env()?)?,
        "/api/send" => api::send(app, &text("number")?, env()?, &text("confirm")?)?,
        "/api/nbp" => api::nbp_rate(
            &text("currency")?,
            text("sale_date")?
                .parse()
                .map_err(|_| "request: sale_date is not YYYY-MM-DD")?,
        )?,
        "/api/fetch" => api::fetch(app, env()?, body["days"].as_i64().unwrap_or(90))?,
        "/api/compare" => api::compare(app, &text("number")?, env()?, &text("ksef_number")?)?,
        _ => return Err(format!("no endpoint {path}")),
    };
    Ok(Ok(json))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, path: &str, query: &str, body: &str) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            query: query.into(),
            host: "127.0.0.1:1".into(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn site() -> Site {
        Site {
            load: Arc::new(|| Ok(crate::app::tests::app())),
            envs: vec![Env::Prod],
        }
    }

    #[test]
    fn rejects_requests_without_the_token_or_with_a_foreign_host() {
        let site = site();
        assert_eq!(route(&request("GET", "/", "", ""), &site, "tok", 1).status, 403);
        assert_eq!(
            route(&request("GET", "/", "t=bad", ""), &site, "tok", 1).status,
            403
        );
        let mut foreign = request("GET", "/", "t=tok", "");
        foreign.host = "evil.example:1".into();
        assert_eq!(route(&foreign, &site, "tok", 1).status, 403);
        let page = route(&request("GET", "/", "t=tok", ""), &site, "tok", 1);
        assert_eq!(page.status, 200);
        assert!(String::from_utf8(page.body).unwrap().contains("tok"));
    }

    #[test]
    fn posts_reach_the_api_and_errors_come_back_as_json() {
        let site = site();
        let call = |path: &str, body: &str| route(&request("POST", path, "t=tok", body), &site, "tok", 1);
        let ok = call(
            "/api/draft",
            r#"{"form":{"issue_date":"2026-09-30","lines":[{"name":"IT","quantity":"1","unit":"h","net_price":"10","vat":"23"}]}}"#,
        );
        assert_eq!(ok.status, 200);
        let value: Value = serde_json::from_slice(&ok.body).unwrap();
        assert_eq!(value["invoice"]["number"], "1/09/2026");
        let pdf = call("/api/draft/pdf", r#"{"form":{}}"#);
        assert_eq!((pdf.status, pdf.content_type), (200, "application/pdf"));
        let bad = call("/api/plan", r#"{"number":"1/09/2026","env":"mars"}"#);
        assert_eq!(bad.status, 400);
        let off = call("/api/fetch", r#"{"env":"test"}"#);
        assert!(String::from_utf8(off.body).unwrap().contains("--test-envs"));
        assert_eq!(call("/api/nope", "{}").status, 400);
        assert_eq!(call("/api/draft", "not json").status, 400);
    }
}
