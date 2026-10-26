use std::fmt::Write;

pub(crate) fn pdf() -> Vec<u8> {
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
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.into_bytes()
}
