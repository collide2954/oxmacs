(module
  (import (ox string) (ox regex) (ox path) (ox file)
          (r session :as session)
          (r jobs :as jobs)
          (markdown grammar :as grammar)
          (markdown style :as style)
          (markdown preview :as preview))
  (export parse highlight render-with script-file saved))

(def opening #"^\s*```+\s*\{\s*([A-Za-z][\w.-]*)(?:[\s,]+([^\s,}=]+)(?:[\s,}]|$))?")
(def label-option #"^\s*#\|\s*label:\s*(\S+)")
(def option-line #"^\s*#\|")
(def comment-line #"^\s*#")
(def continued #"(\|>|%|[-+*/^,=&|~]|<-)\s*$")

(def (front-matter-end lines)
  (when (and (> (count lines) 0) (= (string/trim-end (first lines)) "---"))
    (find (fn [at] (contains? #{"---" "..."} (string/trim-end (nth lines at))))
          (range 1 (count lines)))))

(def (unquoted text)
  (string/replace (string/replace text "\"" "") "'" ""))

(def (labelled chunk lines)
  (if chunk.name
    chunk
    (assoc chunk :name (some (fn [at] (when-let [hit (regex/find label-option (nth lines at))]
                                        (unquoted (first hit.groups))))
                             (range (+ chunk.start 1) chunk.end)))))

(def (parse text)
  "The chunks and headings of R Markdown text, as a map of :lines, :chunks with their :start and :end lines, :language and :name, and :headings with their :line, :level and :title."
  (let [lines (string/lines text)
        total (count lines)
        front (front-matter-end lines)
        step (fn [state at]
               (if (and front (<= at front))
                 state
                 (let [line (nth lines at)
                       read (grammar/step state.fence line)
                       state (assoc state :fence read.state)]
                   (cond ((= read.kind :fence-close)
                          (-> state
                              (update :blocks conj (assoc state.open :end at))
                              (assoc :open nil)))
                     ((= read.kind :fence-open)
                      (let [hit (regex/find opening line)]
                        (assoc state :open {start: at
                                            language: (when hit (first hit.groups))
                                            name: (when hit (second hit.groups))})))
                     ((and (= read.kind :heading) (not (string/blank? read.title)))
                      (update state :headings conj {line: at level: read.level title: read.title}))
                     (else state)))))
        done (reduce step {fence: nil open: nil blocks: [] headings: []} (range total))
        blocks (if done.open (conj done.blocks (assoc done.open :end total)) done.blocks)]
    {lines: lines
     chunks: (map (fn [chunk] (labelled chunk lines)) (filter (fn [block] block.language) blocks))
     headings: done.headings}))

(def (highlight line text state)
  (let [width (string/length text)
        trimmed (string/trim-end text)
        whole (fn [face] (if (= width 0) [] [[0 width face]]))]
    (cond ((and (= line 0) (= trimmed "---")) {spans: (whole :string) state: :front})
      ((= state :front)
       {spans: (whole :string) state: (if (contains? #{"---" "..."} trimmed) nil :front)})
      (else
       (let [read (grammar/step state text)]
         {spans: (cond ((contains? #{:fence-open :fence-close} read.kind) (whole :keyword))
                   ((= read.kind :code)
                    (cond ((regex/matches? option-line text) (whole :attribute))
                      ((regex/matches? comment-line text) (whole :comment))
                      (else [])))
                   (else (style/prose text)))
          state: read.state})))))

(def (here) (parse (buffer-text)))

(def (r-chunk? chunk) (contains? #{"r" "R"} chunk.language))

(def (chunk-at found line)
  (find (fn [chunk] (and (<= chunk.start line) (<= line chunk.end))) found.chunks))

(def (code-line? text)
  (and (not (string/blank? text)) (not (regex/matches? comment-line text))))

(def (body found chunk)
  (let [code (filter (fn [text] (not (regex/matches? option-line text)))
                     (map (fn [at] (nth found.lines at))
                          (range (+ chunk.start 1) (min chunk.end (count found.lines)))))]
    (if (some (fn [text] (not (string/blank? text))) code)
      (str (string/join "\n" code) "\n")
      "")))

(def (run code)
  (if (string/blank? code)
    (message "No R code to run")
    (session/send code)))

(def (runnable found line)
  (let [chunk (chunk-at found line)]
    (cond ((not chunk) (message "Not in an R chunk") nil)
      ((not (r-chunk? chunk)) (message "This is a {chunk.language} chunk, so R does not run it") nil)
      (else chunk))))

(def (leading text)
  (- (string/length text) (string/length (string/trim-start text))))

(def (in-code? found line)
  (when-let [chunk (chunk-at found line)]
    (and (< chunk.start line) (< line chunk.end))))

(def (chosen-lines)
  (if-let [[start end] (region)]
    (let [top (line-of start)
          bottom (line-of end)]
      [top (if (and (> bottom top) (= end (line-start bottom))) (- bottom 1) bottom)])
    [(current-line) (current-line)]))

(def (toggle-hashes top bottom)
  (let [filled (filter (fn [at] (not (string/blank? (line-text at)))) (range top (+ bottom 1)))
        commented (every? (fn [at] (string/starts-with? (string/trim-start (line-text at)) "#")) filled)]
    (if commented
      (each [at filled
             :let [text (line-text at)
                   start (+ (line-start at) (leading text))
                   width (if (string/starts-with? (string/trim-start text) "# ") 2 1)]]
        (delete start (+ start width)))
      (let [column (apply min (map (fn [at] (leading (line-text at))) filled))]
        (each [at filled]
          (set-point (+ (line-start at) column))
          (insert "# "))))))

(def (toggle-html top bottom)
  (let [head (string/trim-start (line-text top))
        tail (string/trim-end (line-text bottom))]
    (if (and (string/starts-with? head "<!--") (string/ends-with? tail "-->"))
      (let [end (+ (line-start bottom) (string/length tail))
            closing (if (string/ends-with? tail " -->") 4 3)
            start (+ (line-start top) (leading (line-text top)))
            opening (if (string/starts-with? head "<!-- ") 5 4)]
        (delete (- end closing) end)
        (delete start (+ start opening)))
      (let [end (+ (line-start bottom) (string/length tail))
            start (+ (line-start top) (leading (line-text top)))]
        (set-point end)
        (insert " -->")
        (set-point start)
        (insert "<!-- ")))))

(defcommand r-markdown-comment "Comment or uncomment with # in R chunks and <!-- --> in the prose"
  (let [found (here)
        [top bottom] (chosen-lines)
        code (in-code? found top)
        kept (marker (point))]
    (cond ((and (= top bottom) (not (region)) (string/blank? (line-text top)))
           (do (set-point (line-end top))
             (if code
               (insert "# ")
               (do (insert "<!--  -->") (set-point (- (point) 4))))))
      ((and code (not (region)))
       (let [text (line-text top)]
         (if (string/starts-with? (string/trim-start text) "#")
           (set-point (+ (line-start top) (leading text) (if (string/starts-with? (string/trim-start text) "# ") 2 1)))
           (do (set-point (+ (line-start top) (string/length (string/trim-end text))))
             (delete (point) (line-end top))
             (insert " # ")))))
      (code
       (do (toggle-hashes top bottom) (set-point (marker-position kept))))
      (else
       (do (toggle-html top bottom) (set-point (marker-position kept)))))))
(defcommand r-markdown-run-chunk "Run the R chunk at point"
  (let [found (here)]
    (when-let [chunk (runnable found (current-line))]
      (run (body found chunk)))))

(defcommand r-markdown-run-all "Run every R chunk of the document"
  (let [found (here)]
    (run (apply str (map (fn [chunk] (body found chunk)) (filter r-chunk? found.chunks))))))

(defcommand r-markdown-run-above "Run the R chunks above point"
  (let [found (here)
        line (current-line)]
    (run (apply str (map (fn [chunk] (body found chunk))
                         (filter (fn [chunk] (and (r-chunk? chunk) (< chunk.end line))) found.chunks))))))

(defcommand r-markdown-next-chunk "Move to the code of the next chunk"
  (let [line (current-line)]
    (if-let [chunk (find (fn [chunk] (> chunk.start line)) (get (here) :chunks))]
      (goto-line (+ chunk.start 1))
      (message "No more chunks"))))

(defcommand r-markdown-previous-chunk "Move to the code of the previous chunk"
  (let [line (current-line)]
    (if-let [chunk (last (filter (fn [chunk] (< chunk.end line)) (get (here) :chunks)))]
      (goto-line (+ chunk.start 1))
      (message "No chunk before this one"))))

(def (depth text)
  (get (reduce (fn [state character]
                 (cond (state.comment (if (= character "\n") (assoc state :comment false) state))
                   (state.escaped (assoc state :escaped false))
                   (state.quote (cond ((= character "\\") (assoc state :escaped true))
                                  ((= character state.quote) (assoc state :quote nil))
                                  (else state)))
                   ((contains? #{"\"" "'" "`"} character) (assoc state :quote character))
                   ((= character "#") (assoc state :comment true))
                   ((contains? #{"(" "[" "{{"} character) (update state :depth inc))
                   ((contains? #{")" "]" "}}"} character) (update state :depth dec))
                   (else state)))
               {depth: 0 quote: nil escaped: false comment: false}
               (map str (string/chars text)))
       :depth))

(def (uncommented text)
  (string/trim-end (first (string/split text "#" 2))))

(def (statement-end lines from end)
  ((fn grow [to]
     (let [text (string/join "\n" (map (fn [at] (nth lines at)) (range from (+ to 1))))]
       (if (and (< (+ to 1) end)
                (or (> (depth text) 0) (regex/matches? continued (uncommented (nth lines to)))))
         (grow (+ to 1))
         to)))
   from))

(defcommand r-markdown-send-statement "Send the R statement or selection to R and step"
  (if-let [[start end] (region)]
    (do (deactivate-mark)
      (run (text-between start end)))
    (let [found (here)
          line (current-line)
          lines found.lines]
      (when-let [chunk (runnable found line)]
        (if (= line chunk.start)
          (message "Not in an R chunk")
          (if-let [first-line (find (fn [at] (code-line? (nth lines at))) (range line chunk.end))]
            (let [last-line (statement-end lines first-line chunk.end)
                  code (str (string/join "\n" (map (fn [at] (nth lines at)) (range first-line (+ last-line 1)))) "\n")]
              (goto-line (or (find (fn [at] (code-line? (nth lines at))) (range (+ last-line 1) chunk.end))
                             chunk.end))
              (run code))
            (message "No R code here")))))))

(defcommand r-markdown-insert-chunk "Insert an R chunk and put point in it"
  (let [before (if (> (current-column) 0) "\n" "")
        at (point)]
    (try
      (insert (str before "```{{r}}\n\n```\n"))
      (set-point (+ at (string/length before) 7))
      (catch error e (message e.message)))))

(def (outline found)
  (let [chunks found.chunks
        headings (map (fn [heading] {line: heading.line label: heading.title note: "L{(+ heading.line 1)}"})
                      found.headings)
        blocks (map (fn [at] (let [chunk (nth chunks at)]
                               {line: chunk.start
                                label: "chunk {(or chunk.name (+ at 1))}"
                                note: "{chunk.language}  L{(+ chunk.start 1)}"}))
                    (range (count chunks)))]
    (reduce (fn [built item]
              (let [label (if (contains? built.places item.label) "{item.label} L{(+ item.line 1)}" item.label)]
                (-> built
                    (assoc-in [:notes label] item.note)
                    (assoc-in [:places label] item.line))))
            {notes: {} places: {}}
            (sort-by (fn [item] item.line) (concat headings blocks)))))

(defcommand r-markdown-outline "Jump to a heading or chunk of the document"
  (let [built (outline (here))]
    (if (empty? built.places)
      (message "No headings or chunks in this document")
      (let [answer (await (prompt "Index item: " :choices built.notes))]
        (when-let [line (get built.places answer)]
          (goto-line line))))))

(defonce rendering (atom #{}))

(def (renderable? file)
  (and file (contains? #{"rmd" "qmd"} (string/lower (or (path/extension file) "")))))

(def render-script #r#"path <- commandArgs(trailingOnly = TRUE)[1]
started <- Sys.time() - 1
quarto <- Sys.which("quarto")
knits <- requireNamespace("rmarkdown", quietly = TRUE) &&
  (rmarkdown::pandoc_available() || !nzchar(quarto))
by_quarto <- grepl("[.]qmd$", path, ignore.case = TRUE) || !knits
out <- tryCatch({
  if (!by_quarto) {
    rmarkdown::render(path, envir = new.env(parent = globalenv()))
  } else if (nzchar(quarto)) {
    status <- system2(quarto, c("render", shQuote(path)))
    if (!identical(as.integer(status), 0L)) stop("quarto render stopped with status ", status)
    stem <- paste0(tools::file_path_sans_ext(basename(path)), ".")
    made <- list.files(dirname(path), full.names = TRUE)
    made <- made[startsWith(basename(made), stem) &
      tolower(tools::file_ext(made)) %in% c("html", "pdf", "docx", "pptx", "odt", "epub") &
      file.mtime(made) >= started]
    if (length(made) == 0) stop("quarto made nothing beside the document")
    made[which.max(file.mtime(made))]
  } else {
    stop("rendering needs the rmarkdown package or Quarto")
  }
}, error = function(e) e)
if (inherits(out, "error")) {
  cat("oxmacs-render-failed\t", conditionMessage(out), "\n", sep = "")
  quit(save = "no", status = 1)
}
cat("oxmacs-rendered\t", normalizePath(out, winslash = "/"), "\n", sep = "")
"#)

(def (script-file)
  (let [file (path/join (path/temp) "oxmacs-render.R")]
    (await (file/write file render-script))
    file))

(def (heard file line)
  (let [[tag value] (string/split (string/trim-end line) "\t" 2)]
    (cond ((= tag "oxmacs-rendered")
           (do (swap! rendering disj file)
             (open-externally value)
             (message "Rendered {(path/name value)}")))
      ((= tag "oxmacs-render-failed")
       (do (swap! rendering disj file)
         (message "Rendering failed: {value}")))
      (else nil))))

(def (render-with file program args)
  "Renders file by running program with args as a background job, opening the file it reports it made."
  (swap! rendering conj file)
  (jobs/start "*R render: {(path/name file)}*" program args (path/parent file)
              :title "Rendering {(path/name file)}"
              :on-line (fn [line] (heard file line))
              :on-exit (fn [_] (swap! rendering disj file))))

(def (render file)
  (when-let [script (jobs/rscript)]
    (render-with file script [(script-file) file])
    (message "Rendering {(path/name file)}")))
(defcommand r-markdown-render "Save the document, render it with R Markdown or Quarto and open the result"
  (let [file (buffer-file)]
    (if (renderable? file)
      (do (run-command 'save-buffer)
        (render file))
      (message "Only .Rmd and .qmd files render"))))

(defcommand r-markdown-preview "Show the R Markdown document rendered beside it, as Markdown previews are"
  (preview/show))

(def (saved file)
  (when (and (setting :render-on-save) (renderable? file) (not (contains? @rendering file)))
    (render file)))

(defmode r-markdown "R Markdown"
  :highlight highlight)
