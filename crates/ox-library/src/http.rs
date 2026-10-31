use std::io::{self, BufRead, BufReader, Read};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use crate::crypto::to_base64;
use ox_lang::Isolate;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::native::{name_of, text};
use ox_lang::portable::{Channel, Failure, Portable, Sent};
use ox_lang::value::Value;
use ox_lang::value::print;
use ureq::Body;
use ureq::http::Response;

const MODULE: &str = "ox/http";
const TIMEOUT: Duration = Duration::from_secs(15);
const ANSWER: Duration = Duration::from_secs(120);
const KEPT_LINES: usize = 4096;
const POLL: Duration = Duration::from_millis(20);
const USER_AGENT: &str = concat!("ox/", env!("CARGO_PKG_VERSION"));
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

fn streaming() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(TIMEOUT))
        .timeout_recv_response(Some(ANSWER))
        .user_agent(USER_AGENT)
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .build()
        .into()
}

fn unreachable(url: &str, error: &dyn std::fmt::Display) -> Failure {
    Failure {
        value: None,
        message: format!("Couldn't reach {url}: {error}"),
    }
}

fn run(asked: &Asked, agent: &ureq::Agent) -> Result<Response<Body>, Failure> {
    let url = &asked.url;
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
    match &asked.body {
        Some(body) => builder
            .body(body.clone())
            .map_err(|error| unreachable(url, &error))
            .and_then(|request| agent.run(request).map_err(|error| unreachable(url, &error))),
        None => builder
            .body(())
            .map_err(|error| unreachable(url, &error))
            .and_then(|request| agent.run(request).map_err(|error| unreachable(url, &error))),
    }
}

fn head(response: &Response<Body>) -> [(Portable, Portable); 2] {
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
    [
        (
            Portable::keyword("status"),
            Portable::Int(i64::from(response.status().as_u16())),
        ),
        (Portable::keyword("headers"), Portable::Map(headers)),
    ]
}

fn send(asked: &Asked) -> Result<Portable, Failure> {
    let mut response = run(asked, &agent())?;
    let [status, headers] = head(&response);
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| unreachable(&asked.url, &error))?;
    Ok(Portable::Map(vec![
        status,
        headers,
        (Portable::keyword("body"), Portable::String(body.into())),
    ]))
}

fn streamed(body: impl Read, lines: &Channel) -> io::Result<()> {
    let mut reader = BufReader::new(body);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes)? == 0 {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&bytes);
        let item = Portable::String(line.trim_end_matches(['\n', '\r']).into());
        loop {
            match lines.send(item.clone()) {
                Sent::Done => break,
                Sent::Full => thread::sleep(POLL),
                Sent::Closed => return Ok(()),
            }
        }
    }
}

fn open(
    asked: &Asked,
    lines: &Arc<Channel>,
    ended: Sender<Result<(), String>>,
) -> Result<Portable, Failure> {
    let response = match run(asked, &streaming()) {
        Ok(response) => response,
        Err(failure) => {
            lines.close();
            let _ = ended.send(Err(failure.message.clone()));
            return Err(failure);
        }
    };
    let [status, headers] = head(&response);
    let url = asked.url.clone();
    let reading = Arc::clone(lines);
    let body = response.into_body().into_reader();
    let started = thread::Builder::new()
        .name(format!("ox http/open {url} body"))
        .spawn(move || {
            let outcome = streamed(body, &reading)
                .map_err(|error| format!("The connection to {url} broke: {error}"));
            reading.close();
            let _ = ended.send(outcome);
        });
    if let Err(error) = started {
        lines.close();
        return Err(unreachable(&asked.url, &error));
    }
    Ok(Portable::Map(vec![
        status,
        headers,
        (
            Portable::keyword("lines"),
            Portable::Channel(Arc::clone(lines)),
        ),
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
        .suspends()
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
        .native(MODULE, "open")
        .suspends()
        .arity(2, Some(2))
        .keywords(&["headers", "body", "user", "password"])
        .outside()
        .function(|heap, arguments| {
            let asked = asked(heap, arguments)?;
            let lines = Arc::new(Channel::new(KEPT_LINES));
            let (ended, ending) = mpsc::channel();
            let done =
                heap.background(
                    &format!("ox http/open {} end", asked.url),
                    move || match ending.recv() {
                        Ok(Err(message)) => Err(Failure {
                            value: None,
                            message,
                        }),
                        Ok(Ok(())) | Err(_) => Ok(Portable::Nil),
                    },
                )?;
            heap.background_carrying(
                &format!("ox http/open {}", asked.url),
                &[("done", done)],
                move || open(&asked, &lines, ended),
            )
        });
    isolate
        .native(MODULE, "get")
        .suspends()
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let url = text(heap, "http/get", arguments[0])?.to_owned();
            heap.background("ox http/get", move || get(&url))
        });
}
