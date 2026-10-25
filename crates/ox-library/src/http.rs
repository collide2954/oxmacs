use std::time::Duration;

use crate::crypto::to_base64;
use ox::Isolate;
use ox::build::full;
use ox::error::{Error, ErrorKind};
use ox::heap::Heap;
use ox::portable::{Failure, Portable};
use ox::symbol::Sym;
use ox::value::Value;
use ox::value::print;

const MODULE: &str = "ox/http";
const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("ox/", env!("CARGO_PKG_VERSION"));
const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

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

fn keyword(name: &str) -> Portable {
    Portable::Keyword(Sym::intern(name))
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
            (keyword(name.as_str()), Portable::String(value.into()))
        })
        .collect();
    let status = Portable::Int(i64::from(response.status().as_u16()));
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| unreachable(&error))?;
    Ok(Portable::Map(vec![
        (keyword("status"), status),
        (keyword("headers"), Portable::Map(headers)),
        (keyword("body"), Portable::String(body.into())),
    ]))
}

fn named(heap: &Heap, value: Value) -> String {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    }
}

fn asked(heap: &Heap, arguments: &[Value]) -> Result<Asked, Error> {
    let mut asked = Asked {
        method: named(heap, arguments[0]).to_ascii_uppercase(),
        url: text(heap, "http/request", arguments[1])?.to_owned(),
        ..Asked::default()
    };
    let mut user = None;
    let mut password = None;
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "headers" => {
                let entries = heap.entries(*value).ok_or_else(|| {
                    Error::new(format!(
                        "http/request expects a map for :headers, got {}",
                        print::brief(heap, *value)
                    ))
                })?;
                asked.headers = entries
                    .into_iter()
                    .map(|(name, given)| (named(heap, name), print::display(heap, given)))
                    .collect();
            }
            [Value::Keyword(option), value] if option.name() == "body" => {
                asked.body = (*value != Value::Nil).then(|| print::display(heap, *value));
            }
            [Value::Keyword(option), value] if option.name() == "user" => {
                user = Some(print::display(heap, *value));
            }
            [Value::Keyword(option), value] if option.name() == "password" => {
                password = Some(print::display(heap, *value));
            }
            _ => {
                return Err(Error::new(format!(
                    "http/request takes :headers, :body, :user and :password, not {}",
                    print::brief(heap, pair[0])
                )));
            }
        }
    }
    asked.login = user.map(|user| (user, password.unwrap_or_default()));
    Ok(asked)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "request", 2, None, |heap, arguments| {
        let asked = asked(heap, arguments)?;
        heap.background(&format!("ox http/request {}", asked.url), move || {
            send(&asked)
        })
    });
    isolate.define_native_in(MODULE, "get", 1, Some(1), |heap, arguments| {
        let url = text(heap, "http/get", arguments[0])?.to_owned();
        heap.background("ox http/get", move || get(&url))
    });
    isolate.define_native_in(MODULE, "encode", 1, Some(1), |heap, arguments| {
        let encoded = encode(text(heap, "http/encode", arguments[0])?);
        heap.string(&encoded).map_err(full)
    });
}
