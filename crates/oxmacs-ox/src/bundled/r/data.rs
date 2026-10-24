use oxmacs_core::editor::JobId;
use oxmacs_core::theme::Highlight;

use super::{PROMPT, R, script};

const LISTING: &str = "functions\tadd\tfunction\t1\tfunction (a, b = 2) \ndata\tcars\tdata.frame\t32 x 11\t32 obs. of 11 variables\nvalues\ts\tcharacter\t1\t\"hello world\"\nvalues\tx\tinteger\t10\t1 2 3 4 5 6 7 8 9 10";

fn opened() -> (R, JobId) {
    let mut r = script("x <- 1:10\n", 0);
    let job = r.running();
    r.press("C-c C-e");
    (r, job)
}

fn listing(r: &mut R, job: JobId) {
    r.output(job, &format!("\u{1}environment\u{2}{LISTING}\u{3}{PROMPT}"));
}

#[test]
fn control_c_control_e_asks_r_and_lists_objects_by_group() {
    let (mut r, job) = opened();
    assert_eq!(r.editor.buffer().name(), "*R:analysis environment*");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$environment()\n"]);

    listing(&mut r, job);

    let shown = r.lines();
    assert_eq!(shown[0], "RET opens   g refreshes   q quits");
    assert_eq!(shown[2], "Data");
    assert!(
        shown[3].starts_with("  cars  data.frame  32 x 11  32 obs. of 11 variables"),
        "{shown:?}"
    );
    assert_eq!(shown[5], "Values");
    assert_eq!(shown[6], "  s     character         1  \"hello world\"");
    assert_eq!(shown[9], "Functions");
    assert_eq!(r.editor.line_style(2).spans[0].1, Highlight::Heading);
    assert_eq!(r.editor.tiling().windows().len(), 3);
    assert_eq!(r.logged(), "");
}

#[test]
fn an_empty_environment_says_so() {
    let (mut r, job) = opened();

    r.output(job, &format!("\u{1}environment\u{2}\u{3}{PROMPT}"));

    assert_eq!(
        r.lines(),
        [
            "RET opens   g refreshes   q quits",
            "",
            "The environment is empty"
        ]
    );
}

#[test]
fn return_opens_tables_as_tables_and_other_objects_with_str() {
    let (mut r, job) = opened();
    listing(&mut r, job);
    r.editor.take_effects();

    r.press("M-< C-n C-n C-n RET");
    assert_eq!(r.sent(), [view_call("0L, \"\", FALSE, \"\"")]);
    r.press("C-n C-n C-n RET");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$inspect(\"s\")\n"]);
    r.press("M-< RET");
    assert_eq!(r.editor.message(), Some("No object on this line"));
}

#[test]
fn the_listing_follows_commands_while_it_is_shown() {
    let (mut r, job) = opened();
    listing(&mut r, job);
    r.editor.take_effects();
    r.press("C-x o C-x o");

    r.press("C-c C-z");
    r.typed("y <- 2");
    r.press("RET");
    r.editor.take_effects();
    r.output(job, PROMPT);

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$environment()\n"]);
    r.output(job, PROMPT);
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn g_refreshes_the_listing() {
    let (mut r, job) = opened();
    listing(&mut r, job);
    r.editor.take_effects();

    r.press("g");

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$environment()\n"]);
}

const CARS: &str = "cars\t32\t11\n\tmpg\tcyl\tdisp\tname\n\tnumeric\tnumeric\tnumeric\tcharacter\nMazda RX4\t21\t6\t160\tfast\nDatsun 710\t22.8\t4\t108\tsmall";

fn viewed(payload: &str) -> (R, JobId) {
    let mut r = script("x\n", 0);
    let job = r.running();
    r.output(job, &format!("\u{1}table\u{2}{payload}\u{3}{PROMPT}"));
    (r, job)
}

fn header(r: &R) -> Vec<String> {
    r.editor
        .pane()
        .header_lines()
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

#[test]
fn tables_show_rows_under_a_header_of_names_and_types() {
    let (r, _) = viewed(CARS);

    assert_eq!(r.editor.buffer().name(), "*R:analysis cars*");
    let header = header(&r);
    assert!(
        header[0].starts_with("cars  32 × 11 (rows 1–2)  columns 1–4"),
        "{header:?}"
    );
    assert_eq!(header[1], "              mpg    cyl   disp  name");
    assert_eq!(header[2], "            <dbl>  <dbl>  <dbl>  <chr>");
    assert_eq!(
        r.lines(),
        [
            "Mazda RX4      21      6    160  fast",
            "Datsun 710   22.8      4    108  small"
        ]
    );
    assert_eq!(r.editor.message(), Some("cars: 32 rows, 11 columns"));
    assert_eq!(r.editor.buffer().point(), 0);
}

#[test]
fn narrow_windows_scroll_through_columns_with_the_arrows() {
    let (mut r, _) = viewed(CARS);
    let window = r.editor.selected_window();
    r.editor.resize_pane(window, 20, 26);
    r.press("C-g");

    assert_eq!(header(&r)[1], "              mpg    cyl");
    r.press("<right>");
    assert_eq!(header(&r)[1], "              cyl   disp");
    r.press("<left> <left>");
    assert_eq!(r.editor.message(), Some("Already at the first column"));
    assert_eq!(r.logged(), "");
}

#[test]
fn g_asks_r_for_the_table_again() {
    let (mut r, _) = viewed(CARS);
    r.editor.take_effects();

    r.press("g");

    assert_eq!(r.sent(), [view_call("0L, \"\", FALSE, \"\"")]);
}

#[test]
fn other_objects_show_their_structure() {
    let mut r = script("x\n", 0);
    let job = r.running();

    r.output(
        job,
        &format!("\u{1}object\u{2}add\nfunction (a, b = 2)  \u{3}{PROMPT}"),
    );

    assert_eq!(r.editor.buffer().name(), "*R:analysis add*");
    assert_eq!(header(&r), ["str(add)"]);
    assert_eq!(r.lines(), ["function (a, b = 2)  "]);
    r.editor.take_effects();
    r.press("g");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$inspect(\"add\")\n"]);
}

const SORTED: &str = "cars\t32\t11\t0\t32\tmpg\tTRUE\t\n\tmpg\tcyl\tdisp\tname\n\tnumeric\tnumeric\tnumeric\tcharacter\nDatsun 710\t22.8\t4\t108\tsmall\nMazda RX4\t21\t6\t160\tfast";

fn view_call(rest: &str) -> String {
    format!("getOption(\"oxmacs\")$view(\"cars\", 1000L, {rest})\n")
}

#[test]
fn s_sorts_by_the_column_at_point_up_then_down_then_not() {
    let (mut r, job) = viewed(CARS);
    r.editor.take_effects();
    r.editor.go_to_line(1, 15);

    r.press("s");
    assert_eq!(r.sent(), [view_call("0L, \"mpg\", FALSE, \"\"")]);
    r.output(job, &format!("\u{1}table\u{2}{SORTED}\u{3}{PROMPT}"));
    assert!(
        header(&r)[0].contains("sorted by mpg ↓"),
        "{:?}",
        header(&r)
    );
    assert_eq!(r.lines()[0], "Datsun 710   22.8      4    108  small");

    r.editor.go_to_line(1, 15);
    r.press("s");
    assert_eq!(r.sent(), [view_call("0L, \"\", FALSE, \"\"")]);
    r.editor.go_to_line(1, 23);
    r.press("s");
    assert_eq!(r.sent(), [view_call("0L, \"cyl\", FALSE, \"\"")]);
    r.press("M-< C-a s");
    assert_eq!(r.editor.message(), Some("No column here"));
}

#[test]
fn slash_filters_the_rows_with_an_r_expression() {
    let (mut r, job) = viewed(CARS);
    r.editor.take_effects();

    r.press("/");
    assert_eq!(
        r.editor.minibuffer().unwrap().prompt(),
        "Keep the rows of cars where (R): "
    );
    r.typed("mpg > 20");
    r.press("RET");
    assert_eq!(r.sent(), [view_call("0L, \"\", FALSE, \"mpg > 20\"")]);

    r.output(
        job,
        &format!(
            "\u{1}table\u{2}{}\u{3}{PROMPT}",
            CARS.replace("cars\t32\t11", "cars\t14\t11\t0\t32\t\tFALSE\tmpg > 20")
        ),
    );
    assert!(
        header(&r)[0].contains("14 of 32 rows where mpg > 20"),
        "{:?}",
        header(&r)
    );
}

#[test]
fn brackets_page_through_long_tables() {
    let (mut r, job) = viewed(&CARS.replace("cars\t32\t11", "cars\t2500\t11\t0\t2500\t\tFALSE\t"));
    r.editor.take_effects();
    assert!(header(&r)[0].contains("rows 1–2)"), "{:?}", header(&r));

    r.press("[");
    assert_eq!(r.editor.message(), Some("This is the first page"));
    r.press("]");
    assert_eq!(r.sent(), [view_call("1000L, \"\", FALSE, \"\"")]);
    r.output(
        job,
        &format!(
            "\u{1}table\u{2}{}\u{3}{PROMPT}",
            CARS.replace("cars\t32\t11", "cars\t2500\t11\t1000\t2500\t\tFALSE\t")
        ),
    );
    assert!(header(&r)[0].contains("rows 1001–1002"), "{:?}", header(&r));
    r.press("[");
    assert_eq!(r.sent(), [view_call("0L, \"\", FALSE, \"\"")]);
}

#[test]
fn i_describes_the_column_at_point() {
    let (mut r, job) = viewed(CARS);
    r.editor.take_effects();
    r.editor.go_to_line(1, 15);

    r.press("i");
    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$stats(\"cars\", \"mpg\")\n"]
    );
    r.output(
        job,
        &format!("\u{1}stats\u{2}mpg\tnumeric\tmean 20.09, max 33.9, 0 missing\u{3}{PROMPT}"),
    );

    assert_eq!(
        r.editor.message(),
        Some("mpg <dbl>: mean 20.09, max 33.9, 0 missing")
    );
}
