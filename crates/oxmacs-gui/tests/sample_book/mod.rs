use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const TITLE: &str = "The Patient Ox";
const AUTHOR: &str = "Mira Hollis";

pub struct Book {
    pub title: &'static str,
    pub author: &'static str,
    pub sky: [f64; 3],
}

const PATIENT_OX: Book = Book {
    title: TITLE,
    author: AUTHOR,
    sky: [30.0, 52.0, 96.0],
};

const CHAPTERS: [(&str, &str); 3] = [
    (
        "The Field",
        "<p>Before the sun had cleared the hedges, the ox was already standing at the edge of the long field, breathing slow clouds into the cold. He did not hurry. He had learned, over many seasons, that the field would wait for him, and that a furrow cut in haste must always be cut again.</p>\
<p>The farmer's daughter came down the lane with a lantern and an apple. She was eleven that spring, and she had decided that the ox was the wisest creature on the farm. <em>He never argues,</em> she told her brother, <em>and he is never wrong about the weather.</em></p>\
<p>Her brother said that the ox was simply large and quiet, and that large quiet things are easy to mistake for wise ones. She gave him the look her mother gave the radio when it was wrong about the weather, and went to fetch the yoke.</p>\
<p>The field was <strong>seventeen furrows</strong> wide. The ox knew this the way he knew the smell of rain: not as a number, but as a shape the morning would take. Seventeen times down, seventeen times back, and somewhere in the middle the sun would warm his shoulders and the crows would follow the plough as if it were a parade.</p>\
<p>They began. The blade bit, the earth turned over dark and shining, and the first furrow opened behind them like a sentence that had been waiting to be written. The girl walked beside him with one hand on his flank, and neither of them said anything at all, because nothing needed saying.</p>\
<p>By the fourth furrow the mist had lifted. By the ninth, the girl was humming. By the twelfth, her brother had wandered down from the barn to watch, pretending he had come for some other reason, and stayed.</p>",
    ),
    (
        "The Furrow",
        "<p>It was at the thirteenth furrow that the plough struck the stone.</p>\
<p>It was not a large stone, as stones go, but it was lodged deep and it was stubborn, and the blade rang against it with a sound that made the crows rise all at once. The ox stopped. He did not pull harder, as a younger animal might have done. He stood, and he waited, and he let the girl find out what was wrong.</p>\
<blockquote><p>A furrow is only as straight as the patience of whoever cuts it. Pull against a stone and you will break the blade; wait, and someone will bring a lever.</p></blockquote>\
<p>That was what her grandmother had written on the inside cover of the farm ledger, in a hand so careful it looked printed. The girl had read it a hundred times without understanding it. Now, kneeling in the cold soil with her fingers around the edge of the stone, she thought she was beginning to.</p>\
<p>Her brother brought the iron bar from the barn. Together they worked it under the stone, and together they leaned, and on the third try the stone came up out of the earth with a wet, satisfied sound, like a word that had finally been remembered.</p>\
<p>They set it at the edge of the field, beside the others. There was a long low wall of them there, built over many years from every stone that had ever stopped a plough:</p>\
<ul><li>the round grey stone her grandfather found the year of the flood,</li><li>the flat red stone that had cracked a blade in two,</li><li>and now this one, small and stubborn, the first that was hers.</li></ul>\
<p>The ox had watched the whole operation with mild interest. When the girl took up the yoke again, he leaned into it without being asked, and the thirteenth furrow ran on, straight and dark, to the hedge.</p>",
    ),
    (
        "The Harvest",
        "<p>The wheat came up green in April and gold in August. The girl, who was twelve by then, counted the rows from the top of the stone wall and found that there were still seventeen of them, and that every one was straight.</p>\
<p>On the last evening of the harvest the family ate supper in the field, at a table carried down from the kitchen. Her brother made a speech, which nobody had asked for, about the virtue of hard work and the dignity of the soil. Her mother laughed and passed the bread. Her father, who rarely spoke, raised his cup toward the edge of the field where the ox stood grazing in the last of the light.</p>\
<p>Later, when the others had gone up to the house, the girl stayed behind and wrote in the back of the ledger, under her grandmother's careful lines, in a hand that was not yet careful but was trying to be:</p>\
<p class=\"verse\">Seventeen furrows, seventeen turns,<br/>one stone lifted, one lesson learned;<br/>the field will wait, the sun will climb,<br/>and every row is cut in time.</p>\
<p>She closed the book. Across the field the ox lifted his great head, as if he had heard, and then lowered it again to the sweet stubble, unhurried, as he did everything.</p>\
<hr/>\
<p>The next spring the stone wall was one stone longer. The ox was one year older. And the girl, walking beside him on the first cold morning, with one hand on his flank and the lantern swinging, found that she was not in a hurry either.</p>",
    ),
];

fn chapter(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body><section><h1>{title}</h1>{body}</section></body></html>"
    )
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "channel values stay between 0 and 255"
)]
fn cover(sky: [f64; 3]) -> Vec<u8> {
    let (width, height) = (600, 900);
    let image = image::RgbaImage::from_fn(width, height, |x, y| {
        let fy = f64::from(y) / f64::from(height);
        let fx = f64::from(x) / f64::from(width);
        let sun = ((fx - 0.5).powi(2) + (fy - 0.38).powi(2)).sqrt();
        let field = fy > 0.62 + 0.03 * (fx * 9.0).sin();
        let [r, g, b] = if sun < 0.12 {
            [244, 196, 92]
        } else if field {
            let furrow = ((fx * 40.0 + fy * 8.0).sin() * 0.5 + 0.5) * 22.0;
            [(92.0 + furrow) as u8, (64.0 + furrow * 0.6) as u8, 38]
        } else {
            [
                (sky[0] + 60.0 * fy) as u8,
                (sky[1] + 70.0 * fy) as u8,
                (sky[2] + 40.0 * fy) as u8,
            ]
        };
        image::Rgba([r, g, b, 255])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

pub fn write(folder: &Path, name: &str) -> PathBuf {
    write_book(folder, name, &PATIENT_OX)
}

pub fn write_book(folder: &Path, name: &str, book: &Book) -> PathBuf {
    let path = folder.join(name);
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
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
    let mut items = String::new();
    let mut spine = String::new();
    for at in 1..=CHAPTERS.len() {
        let _ = write!(
            items,
            "<item id=\"c{at}\" href=\"text/{at}.xhtml\" media-type=\"application/xhtml+xml\"/>"
        );
        let _ = write!(spine, "<itemref idref=\"c{at}\"/>");
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
    let mut toc = String::new();
    for (at, (title, _)) in CHAPTERS.iter().enumerate() {
        let _ = write!(
            toc,
            "<li><a href=\"text/{}.xhtml\">{title}</a></li>",
            at + 1
        );
    }
    add(
        "OEBPS/nav.xhtml",
        format!("<html xmlns:epub=\"http://www.idpf.org/2007/ops\"><body><nav epub:type=\"toc\"><ol>{toc}</ol></nav></body></html>").as_bytes(),
        deflated,
    );
    for (at, (title, body)) in CHAPTERS.iter().enumerate() {
        add(
            &format!("OEBPS/text/{}.xhtml", at + 1),
            chapter(title, body).as_bytes(),
            deflated,
        );
    }
    add("OEBPS/images/cover.png", &cover(book.sky), stored);
    zip.finish().unwrap();
    path
}
