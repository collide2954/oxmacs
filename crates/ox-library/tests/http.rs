use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use ox_lang::Isolate;

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
    let mut isolate = ox_library::isolate();

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
    let mut isolate = ox_library::isolate();

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
    let mut isolate = ox_library::isolate();

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
fn get_takes_a_url() {
    let mut isolate = ox_library::isolate();

    assert_eq!(
        evaluated(&mut isolate, "(module (import (ox http))) (http/get 5)"),
        "error: http/get expects a string, got 5"
    );
}

fn echo() -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut seen = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            seen.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&seen).to_string();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let wanted = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_owned)
                    })
                    .and_then(|length| length.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if body.len() >= wanted || read == 0 {
                    break;
                }
            }
        }
        let request = String::from_utf8_lossy(&seen).to_string();
        let reply = "<multistatus/>";
        stream
            .write_all(
                format!(
                    "HTTP/1.1 207 Multi-Status\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .as_bytes(),
            )
            .unwrap();
        request
    });
    (format!("http://{address}/calendars/ana/"), server)
}

#[test]
fn requests_take_any_method_headers_a_body_and_a_login() {
    let (url, server) = echo();
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) (let [reply (await (http/request :propfind \"{url}\" :headers {{depth: \"1\" content-type: \"application/xml\"}} :body \"<propfind/>\" :user \"ana\" :password \"secret\"))] [reply.status reply.body])"
        ),
    );

    assert_eq!(reply, r#"[207 "<multistatus/>"]"#);
    let request = server.join().unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(
        request.starts_with("PROPFIND /calendars/ana/ HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(lower.contains("\r\ndepth: 1\r\n"), "{request}");
    assert!(
        lower.contains("\r\ncontent-type: application/xml\r\n"),
        "{request}"
    );
    assert!(
        lower.contains("\r\nauthorization: basic yw5honnly3jlda==\r\n"),
        "{request}"
    );
    assert!(request.ends_with("\r\n\r\n<propfind/>"), "{request}");
}

#[test]
fn requests_without_a_body_send_none() {
    let (url, server) = echo();
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) (get (await (http/request \"DELETE\" \"{url}\")) :status)"
        ),
    );

    assert_eq!(reply, "207");
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("DELETE /calendars/ana/ HTTP/1.1\r\n")
    );
}

const COLLECTED: &str = "(def (collected lines) (loop [seen []] (let [line (receive lines)] (if (nil? line) seen (again (conj seen line))))))";

#[test]
fn opened_responses_stream_their_lines_and_finish() {
    let url = serve(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\nevent: a\ndata: 1\n\nevent: b\r\ndata: 2\n",
    );
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) {COLLECTED} (let [response (http/open :get \"{url}\") lines (collected response.lines)] [response.status response.headers.content-type lines (await response.done)])"
        ),
    );

    assert_eq!(
        reply,
        r#"[200 "text/event-stream" ["event: a" "data: 1" "" "event: b" "data: 2"] nil]"#
    );
}

#[test]
fn closing_the_lines_stops_a_stream_that_never_ends() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/events", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: one\n";
        stream.write_all(head.as_bytes()).unwrap();
        while stream.write_all(b"event: ping\n").is_ok() {
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) (let [response (http/open :get \"{url}\") first (receive response.lines)] (close response.lines) [first (await response.done)])"
        ),
    );

    assert_eq!(reply, r#"["data: one" nil]"#);
    server.join().unwrap();
}

#[test]
fn opened_error_statuses_still_give_their_body() {
    let url = serve(
        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"error\":\"no\"}\n",
    );
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) {COLLECTED} (let [response (http/open :get \"{url}\")] [response.status (collected response.lines)])"
        ),
    );

    assert_eq!(reply, r#"[401 ["{\"error\":\"no\"}"]]"#);
}

#[test]
fn opening_an_unreachable_server_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    let mut isolate = ox_library::isolate();

    let failed = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) (try (http/open :get \"{url}\") (catch error e e.message))"
        ),
    );

    assert!(
        failed.starts_with(&format!("\"Couldn't reach {url}: ")),
        "{failed}"
    );
}

#[test]
fn a_stream_cut_off_partway_fails_done() {
    let url = serve(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n8\r\ndata: 1\n\r\n",
    );
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http) (ox string))) {COLLECTED} (let [response (http/open :get \"{url}\") lines (collected response.lines)] [lines (try (await response.done) (catch error e (string/starts-with? e.message \"The connection to {url} broke\")))])"
        ),
    );

    assert_eq!(reply, r#"[["data: 1"] true]"#);
}

#[test]
fn opened_requests_send_their_method_headers_and_body() {
    let (url, server) = echo();
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox http))) {COLLECTED} (let [response (http/open :post \"{url}\" :headers {{x-api-key: \"k\"}} :body \"[1]\")] [response.status (collected response.lines)])"
        ),
    );

    assert_eq!(reply, r#"[207 ["<multistatus/>"]]"#);
    let request = server.join().unwrap();
    assert!(
        request.starts_with("POST /calendars/ana/ HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("\r\nx-api-key: k\r\n"),
        "{request}"
    );
    assert!(request.ends_with("\r\n\r\n[1]"), "{request}");
}

#[test]
fn request_options_are_checked() {
    let mut isolate = ox_library::isolate();

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (ox http))) (http/request :get \"http://localhost/\" :colour \"red\")"
        ),
        "error: http/request has no keyword parameter :colour"
    );
}
