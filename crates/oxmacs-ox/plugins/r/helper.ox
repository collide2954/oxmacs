(module (import (ox file) (ox path) (ox crypto)) (export script write))

(def script #r#"local({
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
  flat <- function(text) gsub("[\t\n\r]", " ", text)
  short <- function(value) {
    text <- tryCatch({
      if (is.function(value)) {
        paste(utils::head(deparse(args(value)), 1), collapse = "")
      } else if (is.data.frame(value)) {
        sprintf("%d obs. of %d variables", nrow(value), ncol(value))
      } else if (is.list(value)) {
        sprintf("List of %d", length(value))
      } else if (is.character(value)) {
        paste(encodeString(utils::head(value, 10), quote = "\""), collapse = " ")
      } else {
        paste(format(utils::head(value, 10), trim = TRUE), collapse = " ")
      }
    }, error = function(e) "")
    substr(flat(text), 1, 80)
  }
  oxmacs$environment <- function() {
    names <- sort(ls(globalenv()))
    rows <- vapply(names, function(name) {
      value <- get(name, envir = globalenv())
      group <- if (is.function(value)) {
        "functions"
      } else if (is.data.frame(value) || is.matrix(value) || is.array(value) || is.list(value)) {
        "data"
      } else {
        "values"
      }
      size <- if (is.null(dim(value))) length(value) else paste(dim(value), collapse = " x ")
      paste(group, name, class(value)[1], size, short(value), sep = "\t")
    }, character(1))
    marker("environment", paste(rows, collapse = "\n"))
    last()
  }
  oxmacs$view <- function(name, rows, offset = 0L, sort = "", decreasing = FALSE, filter = "") {
    value <- tryCatch(get(name, envir = globalenv()), error = function(e) NULL)
    if (!is.data.frame(value) && !is.matrix(value)) {
      marker("message", paste(name, "is not a table"))
      return(last())
    }
    value <- as.data.frame(value)
    total <- nrow(value)
    if (nzchar(filter)) {
      kept <- tryCatch(eval(str2lang(filter), value, globalenv()), error = function(e) e)
      if (inherits(kept, "error")) {
        marker("message", paste0("Cannot keep the rows where ", filter, ": ", conditionMessage(kept)))
        return(last())
      }
      if (!is.logical(kept)) {
        marker("message", paste("Cannot keep the rows where", filter, "because it is not TRUE or FALSE"))
        return(last())
      }
      value <- value[!is.na(kept) & kept, , drop = FALSE]
    }
    if (nzchar(sort) && sort %in% names(value)) {
      value <- value[order(value[[sort]], decreasing = isTRUE(decreasing)), , drop = FALSE]
    }
    offset <- max(0L, min(as.integer(offset), nrow(value)))
    shown <- value[seq_len(max(0L, min(rows, nrow(value) - offset))) + offset, , drop = FALSE]
    types <- vapply(value, function(column) class(column)[1], character(1))
    cells <- lapply(shown, function(column) flat(format(column, trim = TRUE)))
    lines <- c(
      paste(name, nrow(value), ncol(value), offset, total, sort,
        if (isTRUE(decreasing)) "TRUE" else "FALSE", flat(filter), sep = "\t"),
      paste(c("", names(value)), collapse = "\t"),
      paste(c("", types), collapse = "\t"),
      if (nrow(shown) > 0) do.call(paste, c(list(flat(rownames(shown))), cells, sep = "\t"))
    )
    marker("table", paste(lines, collapse = "\n"))
    last()
  }
  oxmacs$stats <- function(name, column) {
    value <- tryCatch(as.data.frame(get(name, envir = globalenv()))[[column]], error = function(e) NULL)
    if (is.null(value)) {
      marker("message", paste("No column", column, "in", name))
      return(last())
    }
    missing <- sum(is.na(value))
    present <- value[!is.na(value)]
    text <- if (is.numeric(present) && length(present) > 0) {
      numbers <- signif(c(mean(present), stats::sd(present), min(present),
        stats::median(present), max(present)), 4)
      paste0("mean ", numbers[1], ", sd ", numbers[2], ", min ", numbers[3],
        ", median ", numbers[4], ", max ", numbers[5])
    } else {
      counts <- sort(table(present), decreasing = TRUE)
      paste0(length(counts), " distinct: ",
        paste(utils::head(paste0(names(counts), " (", counts, ")"), 5), collapse = ", "))
    }
    marker("stats", paste(column, class(value)[1], paste0(flat(text), ", ", missing, " missing"), sep = "\t"))
    last()
  }
  oxmacs$peek <- function(name) {
    text <- tryCatch(
      utils::capture.output(utils::str(eval(str2lang(name), globalenv()), max.level = 1, list.len = 10)),
      error = function(e) conditionMessage(e)
    )
    if (length(text) > 12) text <- c(utils::head(text, 12), "...")
    marker("peek", paste(c(name, text), collapse = "\n"))
    last()
  }
  oxmacs$inspect <- function(name) {
    text <- tryCatch(
      utils::capture.output(utils::str(get(name, envir = globalenv()))),
      error = function(e) conditionMessage(e)
    )
    marker("object", paste(c(name, text), collapse = "\n"))
    last()
  }
  oxmacs$definition <- function(name) {
    value <- tryCatch(eval(str2lang(name), globalenv()), error = function(e) NULL)
    file <- if (is.function(value)) utils::getSrcFilename(value, full.names = TRUE) else character()
    payload <- if (length(file) == 1 && nzchar(file) && file.exists(file)) {
      paste(name, "file", normalizePath(file), utils::getSrcLocation(value, "line")[1], sep = "\t")
    } else if (is.function(value)) {
      paste(name, "source", paste(deparse(value), collapse = "\n"), sep = "\t")
    } else {
      paste0(name, "\t")
    }
    marker("definition", payload)
    last()
  }
  oxmacs$signature <- function(name) {
    value <- tryCatch(eval(str2lang(name), globalenv()), error = function(e) NULL)
    shown <- if (is.function(value)) args(value) else NULL
    text <- ""
    if (!is.null(shown)) {
      lines <- utils::head(deparse(shown), -1)
      text <- paste0(name, sub("^function\\s*", "", gsub("\\s+", " ", paste(lines, collapse = " "))))
    }
    marker("signature", paste(name, trimws(flat(text)), sep = "\t"))
    last()
  }
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
"#)

(def (write)
  "Writes the helper R reads as its profile to a file of its own and returns a promise of its path."
  (let [folder (path/join (path/temp) "oxmacs")
        file (path/join folder "r-{(crypto/random 8)}.R")]
    (spawn (await (file/make-dir folder))
           (await (file/write file script))
           file)))
