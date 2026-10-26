use std::time::Duration;

use crate::crypto::to_base64;
use ox_lang::Isolate;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::native::{name_of, text};
use ox_lang::portable::{Failure, Portable};
use ox_lang::value::Value;
use ox_lang::value::print;

const MODULE: &str = "ox/http";
const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("ox/", env!("CARGO_PKG_VERSION"));
const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0xF)]));
        }
    }
    encoded
}

#[derive(Debug, Default)]
struct Asked {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    login: Option<(String, String)>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(USER_AGENT)
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .build()
        .into()
}

fn get(url: &str) -> Result<Portable, Failure> {
    send(&Asked {
        method: "GET".to_owned(),
        url: url.to_owned(),
        ..Asked::default()
    })
}

fn send(asked: &Asked) -> Result<Portable, Failure> {
    let url = &asked.url;
    let unreachable = |error: &dyn std::fmt::Display| Failure {
        value: None,
        message: format!("Couldn't reach {url}: {error}"),
    };
    let mut builder = ureq::http::Request::builder()
        .method(asked.method.as_str())
        .uri(url.as_str());
    for (name, value) in &asked.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    if let Some((user, password)) = &asked.login {
        let token = to_base64(format!("{user}:{password}").as_bytes());
        builder = builder.header("Authorization", format!("Basic {token}"));
    }
    let ran = match &asked.body {
        Some(body) => builder
            .body(body.clone())
            .map_err(|error| unreachable(&error))
            .and_then(|request| agent().run(request).map_err(|error| unreachable(&error))),
        None => builder
            .body(())
            .map_err(|error| unreachable(&error))
            .and_then(|request| agent().run(request).map_err(|error| unreachable(&error))),
    };
    let mut response = ran?;
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
            (
                Portable::keyword(name.as_str()),
                Portable::String(value.into()),
            )
        })
        .collect();
    let status = Portable::Int(i64::from(response.status().as_u16()));
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| unreachable(&error))?;
    Ok(Portable::Map(vec![
        (Portable::keyword("status"), status),
        (Portable::keyword("headers"), Portable::Map(headers)),
        (Portable::keyword("body"), Portable::String(body.into())),
    ]))
}

fn asked(heap: &Heap, arguments: &[Value]) -> Result<Asked, Error> {
    let mut asked = Asked {
        method: name_of(heap, arguments[0]).to_ascii_uppercase(),
        url: text(heap, "http/request", arguments[1])?.to_owned(),
        ..Asked::default()
    };
    let [headers, body, user, password] = [arguments[2], arguments[3], arguments[4], arguments[5]];
    if headers != Value::Nil {
        let entries = heap.entries(headers).ok_or_else(|| {
            Error::new(format!(
                "http/request expects a map for :headers, got {}",
                print::brief(heap, headers)
            ))
            .of(ErrorKind::Type)
        })?;
        asked.headers = entries
            .into_iter()
            .map(|(name, given)| (name_of(heap, name), print::display(heap, given)))
            .collect();
    }
    asked.body = (body != Value::Nil).then(|| print::display(heap, body));
    let shown = |value: Value| (value != Value::Nil).then(|| print::display(heap, value));
    asked.login = shown(user).map(|user| (user, shown(password).unwrap_or_default()));
    Ok(asked)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "request")
        .arity(2, Some(2))
        .keywords(&["headers", "body", "user", "password"])
        .outside()
        .function(|heap, arguments| {
            let asked = asked(heap, arguments)?;
            heap.background(&format!("ox http/request {}", asked.url), move || {
                send(&asked)
            })
        });
    isolate
        .native(MODULE, "get")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let url = text(heap, "http/get", arguments[0])?.to_owned();
            heap.background("ox http/get", move || get(&url))
        });
    isolate
        .native(MODULE, "encode")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let encoded = encode(text(heap, "http/encode", arguments[0])?);
            heap.string(&encoded).map_err(Error::from)
        });
}
