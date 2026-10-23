use ox::Isolate;

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn crypto(source: &str) -> String {
    let mut isolate = Isolate::new();
    evaluated(
        &mut isolate,
        &format!("(module (import (ox crypto) (ox string))) {source}"),
    )
}

#[test]
fn totp_codes_match_rfc_6238() {
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    assert_eq!(
        crypto(&format!(
            "[(crypto/totp \"{secret}\" :time 59 :digits 8) (crypto/totp \"{secret}\" :time 1111111109 :digits 8) (crypto/totp \"gezd gnbv gy3t qojq gezd gnbv gy3t qojq\" :time 59)]"
        )),
        r#"["94287082" "07081804" "287082"]"#
    );
    assert_eq!(
        crypto("(crypto/totp \"not base32!\")"),
        "error: crypto/totp expects a base32 secret"
    );
}

#[test]
fn random_bytes_are_hex_and_differ() {
    assert_eq!(
        crypto("(let [a (crypto/random 16) b (crypto/random 16)] [(count a) (not= a b)])"),
        "[32 true]"
    );
}

#[test]
fn sealed_text_opens_with_the_same_key_only() {
    assert_eq!(
        crypto(
            "(let [salt (crypto/random 16)
      key (await (crypto/derive-key \"correct horse\" salt :memory 64 :iterations 1))
      other (await (crypto/derive-key \"wrong horse\" salt :memory 64 :iterations 1))
      sealed (crypto/seal key \"hunter2 🔑\")]
  [(count key) (= key (await (crypto/derive-key \"correct horse\" salt :memory 64 :iterations 1)))
   (crypto/open key sealed)
   (not= sealed (crypto/seal key \"hunter2 🔑\"))
   (try (crypto/open other sealed) (catch error e e.message))])"
        ),
        r#"[64 true "hunter2 🔑" true "The key doesn't open this, or it was changed"]"#
    );
}

#[test]
fn base64_and_sha256_round_trip() {
    assert_eq!(
        crypto(
            "[(crypto/to-base64 \"hi ✓\") (crypto/from-base64 (crypto/to-base64 \"hi ✓\")) (crypto/sha256 \"abc\")]"
        ),
        r#"["aGkg4pyT" "hi ✓" "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"]"#
    );
}

#[test]
fn random_numbers_stay_below_their_bound() {
    assert_eq!(
        crypto(
            "(let [drawn (map (fn [_] (crypto/random-below 6)) (range 600))] [(every? (fn [n] (and (>= n 0) (< n 6))) drawn) (count (distinct drawn))])"
        ),
        "[true 6]"
    );
    assert_eq!(
        crypto("(crypto/random-below 0)"),
        "error: crypto/random-below needs a bound above 0"
    );
}
