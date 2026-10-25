use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

use ox::Isolate;

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn lines_flow_both_ways_over_a_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        writer
            .write_all(b":srv 001 oxtest :Welcome\r\nPING :x\n")
            .unwrap();
        line
    });
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox net)))
(def conn (await (net/connect \"127.0.0.1\" {port})))
(net/send conn \"NICK oxtest\")
[(receive conn.lines) (receive conn.lines) conn.host conn.port]"
        ),
    );

    assert_eq!(server.join().unwrap(), "NICK oxtest\r\n");
    assert_eq!(
        reply,
        format!(r#"[":srv 001 oxtest :Welcome" "PING :x" "127.0.0.1" {port}]"#)
    );
}

#[test]
fn the_lines_end_when_the_server_closes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(b"bye\r\n").unwrap();
    });
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox net)))
(def conn (await (net/connect \"127.0.0.1\" {port})))
[(receive conn.lines) (receive conn.lines) (done? conn.lines) (net/send conn \"late\")]"
        ),
    );

    assert_eq!(reply, r#"["bye" nil true false]"#);
}

#[test]
fn closing_flushes_what_was_sent_first() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        BufReader::new(stream)
            .lines()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    });
    let mut isolate = ox_library::isolate();

    evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox net)))
(def conn (await (net/connect \"127.0.0.1\" {port})))
(net/send conn \"QUIT :bye\")
(net/close conn)"
        ),
    );

    assert_eq!(server.join().unwrap(), ["QUIT :bye"]);
}

#[test]
fn unreachable_servers_fail_the_promise() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut isolate = ox_library::isolate();

    let failed = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox net))) (try (await (net/connect \"127.0.0.1\" {port})) (catch error e e.message))"
        ),
    );

    assert!(
        failed.starts_with(&format!("\"Couldn't connect to 127.0.0.1:{port}: ")),
        "{failed}"
    );
}

#[test]
#[ignore = "talks TLS to example.com"]
fn tls_reaches_a_public_server() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        "(module (import (ox net)))
(def conn (await (net/connect \"example.com\" 443 :tls true)))
(net/send conn \"HEAD / HTTP/1.1\")
(net/send conn \"Host: example.com\")
(net/send conn \"Connection: close\")
(net/send conn \"\")
(receive conn.lines)",
    );

    assert!(reply.starts_with("\"HTTP/1.1 "), "{reply}");
}
