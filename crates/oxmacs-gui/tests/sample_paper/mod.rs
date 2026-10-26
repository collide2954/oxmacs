use std::fmt::Write;
use std::path::{Path, PathBuf};

const WIDTH: f32 = 612.0;
const HEIGHT: f32 = 792.0;
const LEFT: f32 = 72.0;
const MEASURE: f32 = 468.0;

const FONTS: [&str; 5] = [
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Helvetica",
    "Helvetica-Bold",
];

#[derive(Clone, Copy)]
enum Face {
    Roman = 1,
    Bold = 2,
    Italic = 3,
    Sans = 4,
    SansBold = 5,
}

fn advance(character: char) -> f32 {
    match character {
        ' ' => 0.25,
        'i' | 'j' | 'l' | 't' | 'f' | 'r' | '.' | ',' | ';' | ':' | '\'' | '(' | ')' => 0.3,
        'm' | 'w' => 0.75,
        'A'..='Z' => 0.68,
        _ => 0.48,
    }
}

fn wrap(text: &str, size: f32, measure: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        let width: f32 = candidate.chars().map(advance).sum::<f32>() * size;
        if width > measure && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('·', "\\267")
        .replace('—', "\\227")
        .replace('’', "'")
}

struct Page {
    stream: String,
    y: f32,
    links: Vec<([f32; 4], Target)>,
}

enum Target {
    Page(usize),
    Uri(&'static str),
}

impl Page {
    fn new() -> Self {
        Self {
            stream: String::new(),
            y: HEIGHT - 72.0,
            links: Vec::new(),
        }
    }

    fn text(&mut self, x: f32, y: f32, face: Face, size: f32, text: &str) {
        let _ = writeln!(
            self.stream,
            "BT /F{} {size} Tf {x} {y} Td ({}) Tj ET",
            face as u8,
            escaped(text)
        );
    }

    fn heading(&mut self, text: &str) {
        self.y -= 14.0;
        self.text(LEFT, self.y, Face::Bold, 14.0, text);
        self.y -= 20.0;
    }

    fn paragraph(&mut self, text: &str) {
        for line in wrap(text, 11.0, MEASURE) {
            self.text(LEFT, self.y, Face::Roman, 11.0, &line);
            self.y -= 15.0;
        }
        self.y -= 7.0;
    }

    fn rule(&mut self, x: f32, y: f32, width: f32) {
        let _ = writeln!(self.stream, "0.4 w {x} {y} m {} {y} l S", x + width);
    }

    fn bar(&mut self, x: f32, y: f32, width: f32, height: f32, shade: f32) {
        let _ = writeln!(self.stream, "{shade} g {x} {y} {width} {height} re f 0 g");
    }

    fn footer(&mut self, number: usize) {
        self.text(
            WIDTH / 2.0 - 3.0,
            40.0,
            Face::Sans,
            9.0,
            &number.to_string(),
        );
    }
}

fn first_page() -> Page {
    let mut page = Page::new();
    page.text(LEFT, page.y, Face::Bold, 24.0, "Patient Furrows");
    page.y -= 26.0;
    page.text(
        LEFT,
        page.y,
        Face::Italic,
        14.0,
        "Notes on plowing with oxen in heavy clay",
    );
    page.y -= 22.0;
    page.text(
        LEFT,
        page.y,
        Face::Sans,
        10.0,
        "Mira Hollis \u{b7} Hollow Lane Farm Trust \u{b7} Spring 2026",
    );
    page.y -= 12.0;
    page.rule(LEFT, page.y, MEASURE);
    page.y -= 22.0;
    page.text(LEFT, page.y, Face::SansBold, 11.0, "Abstract");
    page.y -= 16.0;
    page.paragraph("We followed one ox team through a full plowing season on a seventeen-furrow field of heavy clay. Measured against a small tractor on the neighboring field, the team was slower by the hour but lost less topsoil, compacted the ground far less, and needed no fuel. The furrows were straighter than we expected, and the crows were more interested in them than in the tractor's.");
    page.heading("1  Introduction");
    page.paragraph("Draft animals still plow a great deal of the world's farmland, yet most of what is written about plowing assumes an engine. This note records what a season with oxen looks like when it is measured carefully: how long a furrow takes, how deep it runs, and what the soil looks like afterward.");
    page.paragraph("The field lies at the end of a long lane and has been worked by the same family for four generations. Its seventeen furrows are each about two hundred meters long. A stone wall along the hedge holds every stone that has ever stopped a plough there, and we added one of our own.");
    let figure_y = page.y;
    page.paragraph("Our working hours are summarized in Figure 1, and the full method follows in section 2. Readers who only want the numbers can skip to the results.");
    page.links.push((
        [LEFT + 178.0, figure_y - 3.0, LEFT + 228.0, figure_y + 10.0],
        Target::Page(1),
    ));
    page.footer(1);
    page
}

fn second_page() -> Page {
    let mut page = Page::new();
    page.heading("2  Method");
    page.paragraph("Each morning the team was yoked at first light and worked until the soil grew too warm and sticky to turn cleanly, usually a little before noon. We timed every furrow with a stopwatch, measured its depth at three points with a marked rod, and noted the weather in a ledger kept in the barn.");
    page.paragraph("The tractor on the neighboring field was timed in the same way. Both fields were sown with the same wheat from the same sack, so that the harvest could be compared as well as the plowing.");
    page.y -= 8.0;
    let base = page.y - 170.0;
    let months = [
        ("Mar", 46.0),
        ("Apr", 71.0),
        ("May", 88.0),
        ("Jun", 64.0),
        ("Jul", 39.0),
        ("Aug", 52.0),
    ];
    page.rule(LEFT + 40.0, base, 380.0);
    for (at, (month, value)) in months.iter().enumerate() {
        let x = LEFT + 60.0 + 60.0 * f32::from(u8::try_from(at).unwrap());
        page.bar(
            x,
            base,
            34.0,
            value * 1.6,
            0.35 + 0.08 * f32::from(u8::try_from(at % 2).unwrap()),
        );
        page.text(x + 7.0, base - 14.0, Face::Sans, 9.0, month);
        page.text(
            x + 8.0,
            base + value * 1.6 + 5.0,
            Face::Sans,
            8.0,
            &format!("{value}"),
        );
    }
    page.text(
        LEFT + 40.0,
        base + 160.0,
        Face::Sans,
        9.0,
        "Furrows plowed each week",
    );
    page.y = base - 34.0;
    page.text(
        LEFT,
        page.y,
        Face::Italic,
        10.0,
        "Figure 1. The team's weekly furrows over the season, highest in May.",
    );
    page.y -= 30.0;
    page.paragraph("Plowing stopped for two weeks in July while the ground was baked hard, and resumed after the first August rain.");
    page.footer(2);
    page
}

fn third_page() -> Page {
    let mut page = Page::new();
    page.heading("3  Results");
    page.paragraph("The team averaged eleven minutes a furrow over the season, against four for the tractor. The furrows were a hand's width shallower but noticeably straighter, and the clay behind the oxen broke into crumbs where the tractor left it in slabs.");
    page.paragraph("Soil compaction, measured with a penetrometer in September, was a third lower on the ox field. Its wheat came up a week later and was shorter, but yielded within five percent of the tractor field at harvest.");
    let rows = [
        ("", "Oxen", "Tractor"),
        ("Minutes per furrow", "11", "4"),
        ("Mean depth (cm)", "14", "19"),
        ("Fuel (liters)", "0", "212"),
        ("Yield (t/ha)", "6.1", "6.4"),
    ];
    for (at, (label, oxen, tractor)) in rows.iter().enumerate() {
        let face = if at == 0 { Face::SansBold } else { Face::Sans };
        page.text(LEFT + 20.0, page.y, face, 10.0, label);
        page.text(LEFT + 240.0, page.y, face, 10.0, oxen);
        page.text(LEFT + 330.0, page.y, face, 10.0, tractor);
        page.y -= 16.0;
    }
    page.y -= 10.0;
    page.heading("4  Discussion");
    page.paragraph("Oxen are slow, and nobody who has walked beside them would claim otherwise. But slowness is not the only thing that matters in a field. The ground they leave behind is easier to work the next year, they eat what the farm grows, and a stone that stops them is lifted rather than broken against.");
    page.paragraph("A furrow is only as straight as the patience of whoever cuts it. On this field, that patience belonged mostly to the ox.");
    page.footer(3);
    page
}

fn fourth_page() -> Page {
    let mut page = Page::new();
    page.heading("References");
    let references = [
        "Hollis, M. (2025). The seventeen furrows: a ledger. Hollow Lane Farm Trust.",
        "Hollis, R. (1979). Stones at the hedge. Privately printed.",
        "Field notes and the full timing data are kept at example.org/furrows.",
    ];
    for reference in references {
        for line in wrap(reference, 11.0, MEASURE - 20.0) {
            page.text(LEFT + 20.0, page.y, Face::Roman, 11.0, &line);
            page.y -= 15.0;
        }
        page.y -= 6.0;
    }
    page.links.push((
        [LEFT + 240.0, page.y + 18.0, LEFT + 360.0, page.y + 32.0],
        Target::Uri("https://example.org/furrows"),
    ));
    page.footer(4);
    page
}

pub fn write(folder: &Path, name: &str) -> PathBuf {
    let pages = [first_page(), second_page(), third_page(), fourth_page()];
    let fonts = FONTS.len();
    let first_page_object = 4 + fonts + 6;
    let page_object = |index: usize| first_page_object + 3 * index;
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R /PageMode /UseOutlines >>".to_owned(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (0..pages.len())
                .map(|index| format!("{} 0 R", page_object(index)))
                .collect::<Vec<_>>()
                .join(" "),
            pages.len()
        ),
    ];
    let outline_first = 4 + fonts;
    objects.push(format!(
        "<< /Type /Outlines /First {outline_first} 0 R /Last {} 0 R /Count 5 >>",
        outline_first + 5
    ));
    for font in FONTS {
        objects.push(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{font} /Encoding /WinAnsiEncoding >>"
        ));
    }
    objects.extend(outline(outline_first, first_page_object));
    let resources = format!(
        "<< /Font << {} >> >>",
        (1..=fonts)
            .map(|font| format!("/F{font} {} 0 R", 3 + font))
            .collect::<Vec<_>>()
            .join(" ")
    );
    for (index, page) in pages.iter().enumerate() {
        let contents = page_object(index) + 1;
        let annotations = page_object(index) + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] /Resources {resources} /Contents {contents} 0 R /Annots {annotations} 0 R >>"
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}endstream",
            page.stream.len(),
            page.stream
        ));
        let links: Vec<String> = page
            .links
            .iter()
            .map(|([x0, y0, x1, y1], target)| {
                let action = match target {
                    Target::Page(to) => format!("/Dest [{} 0 R /XYZ 0 {HEIGHT} 0]", page_object(*to)),
                    Target::Uri(uri) => format!("/A << /S /URI /URI ({uri}) >>"),
                };
                format!("<< /Type /Annot /Subtype /Link /Rect [{x0} {y0} {x1} {y1}] /Border [0 0 0] {action} >>")
            })
            .collect();
        objects.push(format!("[{}]", links.join(" ")));
    }
    objects.push("<< /Title (Patient Furrows) /Author (Mira Hollis) >>".to_owned());
    let out = serialized(&objects);
    let path = folder.join(name);
    std::fs::write(&path, out.into_bytes()).unwrap();
    path
}

fn outline(first: usize, first_page: usize) -> Vec<String> {
    let page_object = |index: usize| first_page + 3 * index;
    let headings = [
        ("Introduction", 0),
        ("Method", 1),
        ("Figure 1", 1),
        ("Results", 2),
        ("Discussion", 2),
        ("References", 3),
    ];
    let top_level = [0, 1, 3, 4, 5];
    headings
        .iter()
        .enumerate()
        .map(|(at, (title, page))| {
            let destination = format!("/Dest [{} 0 R /XYZ 0 {HEIGHT} 0]", page_object(*page));
            if at == 2 {
                return format!(
                    "<< /Title ({title}) /Parent {} 0 R {destination} >>",
                    first + 1
                );
            }
            let position = top_level.iter().position(|known| *known == at).unwrap();
            let mut links = String::from("/Parent 3 0 R");
            if let Some(previous) = position.checked_sub(1).map(|before| top_level[before]) {
                let _ = write!(links, " /Prev {} 0 R", first + previous);
            }
            if let Some(next) = top_level.get(position + 1) {
                let _ = write!(links, " /Next {} 0 R", first + next);
            }
            if at == 1 {
                let _ = write!(links, " /First {0} 0 R /Last {0} 0 R /Count 1", first + 2);
            }
            format!("<< /Title ({title}) {links} {destination} >>")
        })
        .collect()
}

fn serialized(objects: &[String]) -> String {
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
        "trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1,
        objects.len()
    );
    out
}
