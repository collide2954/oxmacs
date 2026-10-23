use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hmac};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::paths::text;
use crate::portable::{Failure, Portable};
use crate::value::{Value, print};
use crate::vm::build::full;

const MODULE: &str = "ox/crypto";
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 24;
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const UNOPENED: &str = "The key doesn't open this, or it was changed";

fn hex(bytes: &[u8]) -> String {
    let mut shown = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(shown, "{byte:02x}");
    }
    shown
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

pub(crate) fn to_base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let joined = chunk.iter().enumerate().fold(0_u32, |all, (at, byte)| {
            all | u32::from(*byte) << (16 - 8 * at)
        });
        for at in 0..4 {
            if at <= chunk.len() {
                encoded.push(char::from(
                    BASE64[((joined >> (18 - 6 * at)) & 63) as usize],
                ));
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

fn from_base64(text: &str) -> Option<Vec<u8>> {
    let mut bits = 0_u32;
    let mut count = 0;
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    for symbol in text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace() && *byte != b'=')
    {
        let value = BASE64.iter().position(|known| *known == symbol)?;
        bits = (bits << 6) | u32::try_from(value).ok()?;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push(u8::try_from((bits >> count) & 0xFF).ok()?);
        }
    }
    Some(bytes)
}

fn from_base32(text: &str) -> Option<Vec<u8>> {
    let mut bits = 0_u64;
    let mut count = 0;
    let mut bytes = Vec::new();
    for symbol in text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace() && *byte != b'=' && *byte != b'-')
    {
        let value = BASE32
            .iter()
            .position(|known| *known == symbol.to_ascii_uppercase())?;
        bits = (bits << 5) | u64::try_from(value).ok()?;
        count += 5;
        if count >= 8 {
            count -= 8;
            bytes.push(u8::try_from((bits >> count) & 0xFF).ok()?);
        }
    }
    (!bytes.is_empty()).then_some(bytes)
}

fn random(count: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0; count];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| Error::new("The system has no random numbers to give"))?;
    Ok(bytes)
}

fn wrong(heap: &Heap, name: &str, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {wanted}, got {}",
        print::write(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn key(heap: &Heap, name: &str, value: Value) -> Result<XChaCha20Poly1305, Error> {
    heap.text(value)
        .and_then(unhex)
        .filter(|bytes| bytes.len() == KEY_BYTES)
        .and_then(|bytes| XChaCha20Poly1305::new_from_slice(&bytes).ok())
        .ok_or_else(|| wrong(heap, name, "a key from crypto/derive-key", value))
}

fn counted(heap: &Heap, name: &str, value: Value) -> Result<u64, Error> {
    match value {
        Value::Int(number) if number >= 0 => Ok(number.unsigned_abs()),
        _ => Err(wrong(heap, name, "a whole number", value)),
    }
}

fn options<'a>(
    heap: &Heap,
    name: &str,
    given: &[Value],
    known: &[&'a str],
) -> Result<Vec<(&'a str, u64)>, Error> {
    let mut found = Vec::new();
    for pair in given.chunks(2) {
        let option = match pair {
            [Value::Keyword(option), _] => known.iter().find(|known| **known == option.name()),
            _ => None,
        }
        .ok_or_else(|| {
            Error::new(format!(
                "{name} takes {}, not {}",
                known
                    .iter()
                    .map(|known| format!(":{known}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                print::write(heap, pair[0])
            ))
        })?;
        found.push((*option, counted(heap, name, pair[1])?));
    }
    Ok(found)
}

fn chosen(found: &[(&str, u64)], name: &str, default: u64) -> u64 {
    found
        .iter()
        .find(|(known, _)| *known == name)
        .map_or(default, |(_, value)| *value)
}

fn derive(
    password: &str,
    salt: &[u8],
    memory: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Portable, Failure> {
    let failed = |error: &dyn std::fmt::Display| Failure {
        value: None,
        message: format!("Couldn't derive a key: {error}"),
    };
    let params =
        Params::new(memory, iterations, lanes, Some(KEY_BYTES)).map_err(|error| failed(&error))?;
    let mut out = [0; KEY_BYTES];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|error| failed(&error))?;
    Ok(Portable::String(hex(&out).into()))
}

fn totp(secret: &[u8], time: u64, step: u64, digits: u32) -> String {
    let counter = (time / step.max(1)).to_be_bytes();
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret),
        &counter,
    );
    let bytes = tag.as_ref();
    let offset = usize::from(bytes[bytes.len() - 1] & 0x0F);
    let code = u32::from_be_bytes([
        bytes[offset] & 0x7F,
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]);
    let width = usize::try_from(digits).unwrap_or(6);
    format!("{:0width$}", code % 10_u32.pow(digits))
}

fn string(heap: &mut Heap, text: &str) -> Result<Value, Error> {
    heap.string(text).map_err(full)
}

fn derive_key(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let password = text(heap, "crypto/derive-key", arguments[0])?.to_owned();
    let salt = heap
        .text(arguments[1])
        .and_then(unhex)
        .filter(|salt| salt.len() >= 8)
        .ok_or_else(|| {
            wrong(
                heap,
                "crypto/derive-key",
                "a salt from crypto/random",
                arguments[1],
            )
        })?;
    let found = options(
        heap,
        "crypto/derive-key",
        &arguments[2..],
        &["memory", "iterations", "parallelism"],
    )?;
    let number = |name, default| u32::try_from(chosen(&found, name, default)).unwrap_or(u32::MAX);
    let (memory, iterations, lanes) = (
        number("memory", 65_536),
        number("iterations", 3),
        number("parallelism", 1),
    );
    heap.background("ox crypto/derive-key", move || {
        derive(&password, &salt, memory, iterations, lanes)
    })
}

fn seal(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let cipher = key(heap, "crypto/seal", arguments[0])?;
    let plain = text(heap, "crypto/seal", arguments[1])?.to_owned();
    let nonce = random(NONCE_BYTES)?;
    let nonce = XNonce::try_from(nonce.as_slice()).map_err(|_| Error::new("Bad nonce"))?;
    let sealed = cipher
        .encrypt(&nonce, plain.as_bytes())
        .map_err(|_| Error::new("Couldn't seal the text"))?;
    let mut joined = nonce.to_vec();
    joined.extend_from_slice(&sealed);
    string(heap, &to_base64(&joined))
}

fn open(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let cipher = key(heap, "crypto/open", arguments[0])?;
    let joined = heap
        .text(arguments[1])
        .and_then(from_base64)
        .filter(|joined| joined.len() > NONCE_BYTES)
        .ok_or_else(|| Error::new(UNOPENED))?;
    let (nonce, sealed) = joined.split_at(NONCE_BYTES);
    let nonce = XNonce::try_from(nonce).map_err(|_| Error::new(UNOPENED))?;
    let plain = cipher
        .decrypt(&nonce, sealed)
        .ok()
        .and_then(|plain| String::from_utf8(plain).ok())
        .ok_or_else(|| Error::new(UNOPENED))?;
    string(heap, &plain)
}

fn one_time(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let secret = heap
        .text(arguments[0])
        .and_then(from_base32)
        .ok_or_else(|| Error::new("crypto/totp expects a base32 secret"))?;
    let found = options(
        heap,
        "crypto/totp",
        &arguments[1..],
        &["time", "step", "digits"],
    )?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let digits = u32::try_from(chosen(&found, "digits", 6))
        .unwrap_or(6)
        .clamp(6, 10);
    let code = totp(
        &secret,
        chosen(&found, "time", now),
        chosen(&found, "step", 30),
        digits,
    );
    string(heap, &code)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "random", 1, Some(1), |heap, arguments| {
        let count = usize::try_from(counted(heap, "crypto/random", arguments[0])?).unwrap_or(0);
        let bytes = random(count)?;
        string(heap, &hex(&bytes))
    });
    isolate.define_native_in(MODULE, "random-below", 1, Some(1), |heap, arguments| {
        let bound = counted(heap, "crypto/random-below", arguments[0])?;
        if bound == 0 {
            return Err(Error::new("crypto/random-below needs a bound above 0"));
        }
        let limit = u64::MAX - u64::MAX % bound;
        loop {
            let bytes = random(8)?;
            let drawn = u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]));
            if drawn < limit {
                return Ok(Value::Int(i64::try_from(drawn % bound).unwrap_or(0)));
            }
        }
    });
    isolate.define_native_in(MODULE, "derive-key", 2, None, derive_key);
    isolate.define_native_in(MODULE, "seal", 2, Some(2), seal);
    isolate.define_native_in(MODULE, "open", 2, Some(2), open);
    isolate.define_native_in(MODULE, "totp", 1, None, one_time);
    isolate.define_native_in(MODULE, "sha256", 1, Some(1), |heap, arguments| {
        let given = text(heap, "crypto/sha256", arguments[0])?;
        let digest = digest::digest(&digest::SHA256, given.as_bytes());
        string(heap, &hex(digest.as_ref()))
    });
    isolate.define_native_in(MODULE, "to-base64", 1, Some(1), |heap, arguments| {
        let encoded = to_base64(text(heap, "crypto/to-base64", arguments[0])?.as_bytes());
        string(heap, &encoded)
    });
    isolate.define_native_in(MODULE, "from-base64", 1, Some(1), |heap, arguments| {
        let decoded = heap
            .text(arguments[0])
            .and_then(from_base64)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| Error::new("crypto/from-base64 expects base64 of text"))?;
        string(heap, &decoded)
    });
}
