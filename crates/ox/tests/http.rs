use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use ox::Isolate;

fn serve(reply: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        stream.write_all(reply.as_bytes()).unwrap();
    });
    format!("http://{address}/forecast")
}

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn get_returns_the_status_headers_and_body() {
    let url = serve(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
    );
    let mut isolate = Isolate::new();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http) (ox json))) (let [reply (await (http/get \"{url}\"))] [reply.status reply.headers.content-type (json/read reply.body)])"
        ),
    );

    assert_eq!(reply, r#"[200 "application/json" {ok: true}]"#);
}

#[test]
fn error_statuses_still_reply() {
    let url = serve("HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\ngone");
    let mut isolate = Isolate::new();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) (let [reply (await (http/get \"{url}\"))] [reply.status reply.body])"
        ),
    );

    assert_eq!(reply, r#"[404 "gone"]"#);
}

#[test]
fn unreachable_servers_fail_the_promise() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    let mut isolate = Isolate::new();

    let failed = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) (try (await (http/get \"{url}\")) (catch error e e.message))"
        ),
    );

    assert!(
        failed.starts_with(&format!("\"Couldn't reach {url}: ")),
        "{failed}"
    );
}

#[test]
fn query_text_is_percent_encoded() {
    let mut isolate = Isolate::new();

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (ox http))) (http/encode \"São Paulo & co\")"
        ),
        "\"S%C3%A3o%20Paulo%20%26%20co\""
    );
    assert_eq!(
        evaluated(&mut isolate, "(module (import (ox http))) (http/get 5)"),
        "error: http/get expects a string, got 5"
    );
}
