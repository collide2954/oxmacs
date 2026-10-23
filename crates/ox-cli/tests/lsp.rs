use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

fn read_message(reader: &mut impl BufRead) -> String {
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = value.parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

#[test]
fn ox_lsp_speaks_the_protocol_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ox"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());

    input
        .write_all(
            frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#)
                .as_bytes(),
        )
        .unwrap();
    let answer = read_message(&mut output);
    assert!(answer.contains("\"hoverProvider\":true"), "{answer}");
    assert!(answer.contains("ox lsp"), "{answer}");

    for message in [
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ] {
        input.write_all(frame(message).as_bytes()).unwrap();
    }
    let answer = read_message(&mut output);
    assert!(answer.contains("\"id\":2"), "{answer}");
    drop(input);
    assert!(child.wait().unwrap().success());
}
