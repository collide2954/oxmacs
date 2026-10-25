use std::io::Write;
use std::path::{Path, PathBuf};

use ox::Isolate;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    let source = format!("(module (import (ox zip))) {source}");
    match isolate.eval(&source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn archive(folder: &Path) -> PathBuf {
    let path = folder.join("book.zip");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.add_directory("OEBPS/", stored).unwrap();
    zip.start_file("OEBPS/chapter.xhtml", deflated).unwrap();
    zip.write_all("<p>Héllo</p>".as_bytes()).unwrap();
    zip.start_file("OEBPS/bytes.bin", deflated).unwrap();
    zip.write_all(&[0, 1, 2, 255]).unwrap();
    zip.finish().unwrap();
    path
}

fn literal(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

#[test]
fn entries_are_listed_with_their_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let path = literal(&archive(dir.path()));
    let mut isolate = ox_library::isolate();

    assert_eq!(
        evaluated(
            &mut isolate,
            &format!(
                "(map (fn [entry] [entry.name entry.size entry.dir]) (await (zip/entries {path})))"
            )
        ),
        r#"[["mimetype" 20 false] ["OEBPS/" 0 true] ["OEBPS/chapter.xhtml" 13 false] ["OEBPS/bytes.bin" 4 false]]"#
    );
}

#[test]
fn entries_read_as_text_or_base64() {
    let dir = tempfile::tempdir().unwrap();
    let path = literal(&archive(dir.path()));
    let mut isolate = ox_library::isolate();

    assert_eq!(
        evaluated(
            &mut isolate,
            &format!(r#"(await (zip/read {path} "OEBPS/chapter.xhtml"))"#)
        ),
        r#""<p>Héllo</p>""#
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            &format!(r#"(await (zip/read-base64 {path} "OEBPS/bytes.bin"))"#)
        ),
        r#""AAEC/w==""#
    );
}

#[test]
fn missing_entries_and_archives_are_named() {
    let dir = tempfile::tempdir().unwrap();
    let path = archive(dir.path());
    let mut isolate = ox_library::isolate();

    let missing = evaluated(
        &mut isolate,
        &format!(
            r#"(try (await (zip/read {} "nope.txt")) (catch error e e.message))"#,
            literal(&path)
        ),
    );
    assert!(
        missing.starts_with(
            &format!("\"Couldn't read nope.txt from {}", path.display()).replace('\\', "\\\\")
        ),
        "{missing}"
    );
    let absent = evaluated(
        &mut isolate,
        &format!(
            "(try (await (zip/entries {})) (catch error e e.message))",
            literal(&dir.path().join("absent.zip"))
        ),
    );
    assert!(absent.starts_with("\"Couldn't open"), "{absent}");
}
