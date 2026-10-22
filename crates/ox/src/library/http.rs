use std::time::Duration;

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::portable::{Failure, Portable};
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::print;
use crate::vm::build::full;

const MODULE: &str = "ox/http";
const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("ox/", env!("CARGO_PKG_VERSION"));
const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a string, got {}",
            print::write(heap, value)
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

fn get(url: &str) -> Result<Portable, Failure> {
    let unreachable = |error: &dyn std::fmt::Display| Failure {
        value: None,
        message: format!("Couldn't reach {url}: {error}"),
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(USER_AGENT)
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|error| unreachable(&error))?;
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

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "get", 1, Some(1), |heap, arguments| {
        let url = text(heap, "http/get", arguments[0])?.to_owned();
        heap.background("ox http/get", move || get(&url))
    });
    isolate.define_native_in(MODULE, "encode", 1, Some(1), |heap, arguments| {
        let encoded = encode(text(heap, "http/encode", arguments[0])?);
        heap.string(&encoded).map_err(full)
    });
}
