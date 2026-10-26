use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[derive(Debug, Clone, Copy)]
pub struct Chapter<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub anchor: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct Epub<'a> {
    pub title: &'a str,
    pub author: &'a str,
    pub chapters: &'a [Chapter<'a>],
    pub cover: &'a [u8],
}

fn chapter(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body><section><h1>{title}</h1>{body}</section></body></html>"
    )
}

pub fn write_epub(path: &Path, book: &Epub<'_>) {
    let mut zip = ZipWriter::new(std::fs::File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8], options| {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    };
    add("mimetype", b"application/epub+zip", stored);
    add(
        "META-INF/container.xml",
        br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        deflated,
    );
    let (mut items, mut spine, mut toc) = (String::new(), String::new(), String::new());
    for (at, part) in book.chapters.iter().enumerate() {
        let at = at + 1;
        let _ = write!(
            items,
            "<item id=\"c{at}\" href=\"text/{at}.xhtml\" media-type=\"application/xhtml+xml\"/>"
        );
        let _ = write!(spine, "<itemref idref=\"c{at}\"/>");
        let anchor = part
            .anchor
            .map(|anchor| format!("#{anchor}"))
            .unwrap_or_default();
        let _ = write!(
            toc,
            "<li><a href=\"text/{at}.xhtml{anchor}\">{}</a></li>",
            part.title
        );
    }
    add(
        "OEBPS/content.opf",
        format!(
            "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>en</dc:language></metadata><manifest><item id=\"nav\" href=\"nav.xhtml\" properties=\"nav\" media-type=\"application/xhtml+xml\"/>{items}<item id=\"cover\" href=\"images/cover.png\" properties=\"cover-image\" media-type=\"image/png\"/></manifest><spine>{spine}</spine></package>",
            book.title, book.author
        )
        .as_bytes(),
        deflated,
    );
    add(
        "OEBPS/nav.xhtml",
        format!("<html xmlns:epub=\"http://www.idpf.org/2007/ops\"><body><nav epub:type=\"toc\"><ol>{toc}</ol></nav></body></html>").as_bytes(),
        deflated,
    );
    for (at, part) in book.chapters.iter().enumerate() {
        add(
            &format!("OEBPS/text/{}.xhtml", at + 1),
            chapter(part.title, part.body).as_bytes(),
            deflated,
        );
    }
    add("OEBPS/images/cover.png", book.cover, stored);
    zip.finish().unwrap();
}

pub fn pdf_file(objects: &[String], info: Option<usize>) -> Vec<u8> {
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{object}\nendobj\n", index + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let info = info
        .map(|object| format!(" /Info {object} 0 R"))
        .unwrap_or_default();
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R{info} >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.into_bytes()
}

pub fn sample_pdf() -> Vec<u8> {
    let pages = 3;
    let first_page = 8;
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            (0..pages)
                .map(|page| format!("{} 0 R", first_page + 2 * page))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Outlines /First 4 0 R /Last 6 0 R /Count 3 >>".to_owned(),
        format!(
            "<< /Title (Chapter 1) /Parent 3 0 R /Next 6 0 R /First 5 0 R /Last 5 0 R /Count 1 /Dest [{first_page} 0 R /Fit] >>"
        ),
        format!(
            "<< /Title (Section 1.1) /Parent 4 0 R /Dest [{} 0 R /Fit] >>",
            first_page + 2
        ),
        format!(
            "<< /Title (Chapter 2) /Parent 3 0 R /Prev 4 0 R /Dest [{} 0 R /Fit] >>",
            first_page + 4
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    for page in 0..pages {
        let contents = first_page + 2 * page + 1;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 7 0 R >> >> /Contents {contents} 0 R >>"
        ));
        let stream = format!(
            "BT /F1 36 Tf 72 700 Td (Page {}) Tj ET\nBT /F1 14 Tf 72 640 Td (The quick brown fox jumps over the lazy dog.) Tj ET",
            page + 1
        );
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    pdf_file(&objects, None)
}

pub fn timed(label: &str, runs: usize, mut work: impl FnMut()) -> Duration {
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        work();
        times.push(started.elapsed());
    }
    times.sort_unstable();
    let median = times[times.len() / 2];
    let worst = times[times.len() - 1];
    println!("{label:<44} median {median:>10.2?}   worst {worst:>10.2?}   ({runs} runs)");
    median
}

pub fn rust_sample(lines: usize) -> String {
    let mut text = String::new();
    for index in 0..lines / 4 {
        let _ = writeln!(
            text,
            "fn item_{index}(value: u64) -> u64 {{\n    let doubled = value * 2; // {index}\n    doubled + {index}\n}}"
        );
    }
    text
}
