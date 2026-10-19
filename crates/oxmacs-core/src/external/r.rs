use std::fmt::Write;

const START: char = '\u{1}';
const SEPARATOR: char = '\u{2}';
const END: char = '\u{3}';

pub const PROFILE_VARIABLE: &str = "R_PROFILE_USER";
pub const ORIGINAL_PROFILE_VARIABLE: &str = "OXMACS_R_PROFILE_USER";
pub const PLOTS_VARIABLE: &str = "OXMACS_R_PLOTS";

pub const HELPER: &str = r#"local({
  original <- Sys.getenv("OXMACS_R_PROFILE_USER")
  if (nzchar(original)) {
    Sys.setenv(R_PROFILE_USER = original)
  } else {
    Sys.unsetenv("R_PROFILE_USER")
  }
  Sys.unsetenv("OXMACS_R_PROFILE_USER")
  candidates <- c(
    original,
    ".Rprofile",
    path.expand("~/.Rprofile")
  )
  for (candidate in candidates) {
    if (nzchar(candidate) && file.exists(candidate)) {
      source(candidate)
      break
    }
  }
})
local({
  marker <- function(kind, payload) {
    cat("\001", kind, "\002", payload, "\003", sep = "")
  }
  last <- function() invisible(get(".Last.value", envir = baseenv()))
  oxmacs <- new.env()
  oxmacs$help <- function(topic) {
    found <- tryCatch(
      utils::help(topic, help_type = "text"),
      error = function(e) character()
    )
    if (length(found) == 0) {
      marker("message", paste0("No R help for ", topic))
    } else {
      print(found)
    }
    last()
  }
  oxmacs$complete <- function(line, cursor) {
    found <- tryCatch({
      utils:::.assignLinebuffer(line)
      utils:::.assignEnd(cursor)
      utils:::.guessTokenFromLine()
      utils:::.completeToken()
      c(utils:::.CompletionEnv[["start"]], utils:::.retrieveCompletions())
    }, error = function(e) character())
    marker("complete", paste(found, collapse = "\t"))
    last()
  }
  oxmacs$width <- function(width) {
    options(width = width)
    tools::Rd2txt_options(width = width)
    last()
  }
  plots <- new.env()
  plots$history <- list()
  plots$ids <- integer()
  plots$next_id <- 1L
  plots$fresh <- FALSE
  plots$device <- 0L
  plots$count <- 0L
  plots$size <- c(672L, 480L)
  plots$directory <- Sys.getenv("OXMACS_R_PLOTS", file.path(tempdir(), "oxmacs-plots"))
  Sys.unsetenv("OXMACS_R_PLOTS")
  dir.create(plots$directory, showWarnings = FALSE, recursive = TRUE)
  options(device = function(...) {
    grDevices::pdf(NULL, width = plots$size[1] / 96, height = plots$size[2] / 96, bg = "white")
    grDevices::dev.control("enable")
    plots$device <- grDevices::dev.cur()
  })
  setHook("plot.new", function() plots$fresh <- TRUE)
  setHook("grid.newpage", function() plots$fresh <- TRUE)
  bitmap <- function(file, width, height, scale) {
    arguments <- list(file, width = width * scale, height = height * scale,
      res = 96 * scale, bg = "white")
    if (capabilities("cairo")) arguments$type <- "cairo"
    do.call(grDevices::png, arguments)
  }
  draw <- function(open, index) {
    current <- grDevices::dev.cur()
    open()
    on.exit({
      grDevices::dev.off()
      if (current > 1) grDevices::dev.set(current)
      plots$fresh <- FALSE
    })
    grDevices::replayPlot(plots$history[[index]])
  }
  numbered <- function(prefix) {
    plots$count <- plots$count + 1L
    file.path(plots$directory, sprintf("%s-%d.png", prefix, plots$count))
  }
  render <- function(why, index) {
    file <- numbered("plot")
    tryCatch(
      draw(function() bitmap(file, plots$size[1], plots$size[2], 2), index),
      error = function(e) NULL
    )
    marker("plot", paste(why, index, length(plots$history), plots$ids[index],
      plots$size[1], plots$size[2], normalizePath(file, winslash = "/"), sep = "\t"))
  }
  find <- function(id) match(id, plots$ids)
  gone <- function() marker("message", "That plot is no longer in the history")
  oxmacs$plot <- function(index, width, height) {
    plots$size <- c(width, height)
    if (index >= 1 && index <= length(plots$history)) render("asked", index)
    last()
  }
  oxmacs$plot_pinned <- function(id, width, height) {
    index <- find(id)
    if (is.na(index)) {
      marker("plot", paste("gone", id, sep = "\t"))
    } else {
      size <- plots$size
      plots$size <- c(width, height)
      render("pinned", index)
      plots$size <- size
    }
    last()
  }
  oxmacs$plot_save <- function(id, file, width, height) {
    index <- find(id)
    if (is.na(index)) {
      gone()
      return(last())
    }
    open <- switch(tolower(tools::file_ext(file)),
      pdf = function() grDevices::pdf(file, width = width / 96, height = height / 96),
      svg = function() grDevices::svg(file, width = width / 96, height = height / 96),
      function() bitmap(file, width, height, 2)
    )
    saved <- tryCatch({
      draw(open, index)
      TRUE
    }, error = function(e) conditionMessage(e))
    marker("message", if (isTRUE(saved)) {
      paste("Saved plot to", file)
    } else {
      paste("Cannot save the plot:", saved)
    })
    last()
  }
  oxmacs$plot_delete <- function(id, width, height) {
    index <- find(id)
    count <- length(plots$history)
    if (is.na(index)) {
      index <- count
    } else {
      plots$history[[index]] <- NULL
      plots$ids <- plots$ids[-index]
      if (index == count) plots$detached <- TRUE
    }
    plots$size <- c(width, height)
    count <- length(plots$history)
    if (count == 0L) marker("plot", "none") else render("deleted", min(index, count))
    last()
  }
  oxmacs$plot_gallery <- function(width, height) {
    files <- vapply(seq_along(plots$history), function(index) {
      file <- numbered("thumb")
      tryCatch(draw(function() bitmap(file, width, height, 1), index), error = function(e) NULL)
      normalizePath(file, winslash = "/")
    }, character(1))
    marker("gallery", paste(files, collapse = "\t"))
    last()
  }
  oxmacs$plot_zoom <- function(id, width, height) {
    index <- find(id)
    if (is.na(index)) {
      gone()
      return(last())
    }
    file <- numbered("zoom")
    tryCatch(draw(function() bitmap(file, width, height, 1), index), error = function(e) NULL)
    marker("zoom", normalizePath(file, winslash = "/"))
    last()
  }
  invisible(addTaskCallback(function(...) {
    if (grDevices::dev.cur() == plots$device) {
      shown <- tryCatch(grDevices::recordPlot(), error = function(e) NULL)
      if (!is.null(shown) && !identical(shown, plots$last)) {
        index <- length(plots$history)
        if (plots$fresh || isTRUE(plots$detached) || index == 0L) index <- index + 1L
        plots$detached <- FALSE
        if (index > length(plots$ids)) {
          plots$ids[index] <- plots$next_id
          plots$next_id <- plots$next_id + 1L
        }
        plots$history[[index]] <- shown
        plots$last <- shown
        render("new", index)
      }
    }
    TRUE
  }, name = "oxmacs-plots"))
  utils::rc.settings(ipck = TRUE)
  options(
    oxmacs = oxmacs,
    help_type = "text",
    pager = function(files, header, title, delete.file) {
      text <- unlist(lapply(files, readLines, warn = FALSE))
      if (isTRUE(delete.file)) unlink(files)
      marker("help", paste(text, collapse = "\n"))
    }
  )
})
options(prompt = "\001>\002\003", continue = "\001+\002\003")
"#;

pub fn string(text: &str) -> String {
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for c in text.chars() {
        match c {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(literal, "\\u{{{:x}}}", u32::from(c));
            }
            c => literal.push(c),
        }
    }
    literal.push('"');
    literal
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Prompt(char),
    Marker { kind: String, payload: String },
}

#[derive(Debug, Default)]
pub struct Stream {
    pending: String,
}

impl Stream {
    pub fn feed(&mut self, chunk: &str) -> Vec<Piece> {
        self.pending.push_str(chunk);
        let text = std::mem::take(&mut self.pending);
        let mut pieces = Vec::new();
        let mut rest = text.as_str();
        while let Some(at) = rest.find(START) {
            push_text(&mut pieces, &rest[..at]);
            let body = &rest[at + START.len_utf8()..];
            let Some(end) = body.find(END) else {
                rest[at..].clone_into(&mut self.pending);
                return pieces;
            };
            let (kind, payload) = body[..end]
                .split_once(SEPARATOR)
                .unwrap_or((&body[..end], ""));
            pieces.push(match kind {
                ">" => Piece::Prompt('>'),
                "+" => Piece::Prompt('+'),
                _ => Piece::Marker {
                    kind: kind.to_owned(),
                    payload: payload.to_owned(),
                },
            });
            rest = &body[end + END.len_utf8()..];
        }
        push_text(&mut pieces, rest);
        pieces
    }
}

fn push_text(pieces: &mut Vec<Piece>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Piece::Text(last)) = pieces.last_mut() {
        last.push_str(text);
    } else {
        pieces.push(Piece::Text(text.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(text: &str) -> Piece {
        Piece::Text(text.to_owned())
    }

    #[test]
    fn plain_output_is_text() {
        assert_eq!(Stream::default().feed("[1] 2\n"), [text("[1] 2\n")]);
    }

    #[test]
    fn prompts_are_recognised_anywhere() {
        let mut stream = Stream::default();

        assert_eq!(
            stream.feed("done\u{1}>\u{2}\u{3}x\u{1}+\u{2}\u{3}"),
            [
                text("done"),
                Piece::Prompt('>'),
                text("x"),
                Piece::Prompt('+')
            ]
        );
    }

    #[test]
    fn markers_carry_a_payload_and_may_arrive_in_pieces() {
        let mut stream = Stream::default();

        assert_eq!(stream.feed("a\u{1}plot\u{2}C:/tmp"), [text("a")]);
        assert_eq!(
            stream.feed("/p.png\u{3}b"),
            [
                Piece::Marker {
                    kind: "plot".to_owned(),
                    payload: "C:/tmp/p.png".to_owned()
                },
                text("b")
            ]
        );
    }

    #[test]
    fn strings_are_quoted_for_r() {
        assert_eq!(string("mean"), "\"mean\"");
        assert_eq!(string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(string("x\ny\u{1}"), "\"x\\ny\\u{1}\"");
        assert_eq!(string("é"), "\"é\"");
    }

    #[test]
    fn the_helper_answers_help_through_markers() {
        assert!(HELPER.contains("oxmacs$help <- function(topic)"));
        assert!(HELPER.contains("pager = function(files, header, title, delete.file)"));
        assert!(HELPER.contains("help_type = \"text\""));
    }

    #[test]
    fn the_helper_completes_through_markers() {
        assert!(HELPER.contains("oxmacs$complete <- function(line, cursor)"));
        assert!(HELPER.contains("marker(\"complete\""));
    }

    #[test]
    fn the_helper_sets_the_width_of_output_and_help() {
        assert!(HELPER.contains("options(width = width)"));
        assert!(HELPER.contains("tools::Rd2txt_options(width = width)"));
    }

    #[test]
    fn the_helper_sets_marked_prompts() {
        assert!(
            HELPER.contains(r#"options(prompt = "\001>\002\003", continue = "\001+\002\003")"#)
        );
        assert!(HELPER.contains(ORIGINAL_PROFILE_VARIABLE));
    }
}
