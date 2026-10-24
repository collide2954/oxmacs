(module
  (import (ox string)
          (r session :as session))
  (export listed tabled described described-column environment-due))

(def help "RET opens   g refreshes   q quits")
(def value-width 60)
(def least-value 8)
(def room 100)
(def cell-width 24)
(def gap 2)
(def table-room 80)
(def view-rows 1000)
(def groups [["data" "Data"] ["values" "Values"] ["functions" "Functions"]])
(def tabular #{"data.frame" "matrix" "tbl_df" "data.table"})
(def short-types {"numeric" "dbl" "integer" "int" "character" "chr" "factor" "fct" "logical" "lgl"
                  "Date" "date" "POSIXct" "dttm" "complex" "cpl"})

(defonce environments (atom {}))
(defonce tables (atom {}))

(def (stem session)
  (if (string/ends-with? session "*") (string/slice session 0 (- (count session) 1)) session))

(def (environment-name session) "{(stem session)} environment*")

(def (entries payload)
  (keep (fn [line]
          (let [fields (string/split line "\t" 5)]
            (when (>= (count fields) 4)
              {group: (nth fields 0) name: (nth fields 1) class: (nth fields 2) size: (nth fields 3)
               value: (string/trim-end (or (get fields 4) ""))})))
        (string/lines payload)))

(def (widest found pick) (apply max 0 (map (fn [entry] (count (pick entry))) found)))

(def (page found width)
  (if (empty? found)
    {lines: [[[help :comment]] [] [["The environment is empty" :comment]]] rows: []}
    (let [names (widest found (fn [entry] entry.name))
          classes (widest found (fn [entry] entry.class))
          sizes (widest found (fn [entry] entry.size))
          fixed (+ 2 names 2 classes 2 sizes 2)
          shown-width (min (max 0 (- width fixed)) value-width)
          line (fn [entry]
                 [["  {(string/pad-end entry.name names)}  " :link]
                  ["{(string/pad-end entry.class classes)}  " :comment]
                  "{(string/pad-start entry.size sizes)}  "
                  [(if (>= shown-width least-value) (string/truncate entry.value shown-width) "") :comment]])]
      (reduce (fn [built [group title]]
                (let [members (filter (fn [entry] (= entry.group group)) found)]
                  (if (empty? members)
                    built
                    (let [lines (conj built.lines [] [[title :heading]])]
                      (reduce (fn [done entry]
                                {lines: (conj done.lines (line entry))
                                 rows: (conj done.rows [(count done.lines) entry])})
                              {lines: lines rows: built.rows}
                              members)))))
              {lines: [[[help :comment]]] rows: []}
              groups))))

(def (draw-environment name width)
  (when-let [listing (get @environments name)]
    (let [drawn (page listing.entries (if (and width (> width 0)) width room))]
      (swap! environments assoc-in [name :rows] drawn.rows)
      (set-lines name drawn.lines))))

(def environment-keys
  {"RET" 'r-environment-visit "n" 'next-line "p" 'previous-line
   "g" 'r-environment-refresh "q" 'quit-window})

(def (show-environment session)
  (let [name (environment-name session)]
    (unless (buffer-exists? name)
      (swap! environments assoc name {session: session entries: [] rows: []})
      (open-buffer name [help] :keys environment-keys :show false
                   :on-resize (fn [width] (draw-environment name width)))
      (session/own! name session))
    (unless (buffer-shown? name)
      (if (buffer-shown? session)
        (display-buffer name :below session)
        (display-buffer name)))
    name))

(def (ask-environment session)
  (session/request session "getOption(\"oxmacs\")$environment()"))

(def (listed session payload)
  (let [name (environment-name session)]
    (when (get @environments name)
      (swap! environments assoc-in [name :entries] (entries payload))
      (draw-environment name (buffer-width name)))))

(def (environment-due session)
  (when (buffer-shown? (environment-name session))
    (ask-environment session)))

(def plain-view {offset: 0 sort: "" decreasing: false filter: ""})

(def (ask-table session object view)
  (session/request
   session
   "getOption(\"oxmacs\")$view({(session/r-string object)}, {view-rows}L, {view.offset}L, {(session/r-string view.sort)}, {(if view.decreasing "TRUE" "FALSE")}, {(session/r-string view.filter)})"))

(def (ask-object session object table)
  (if table
    (ask-table session object plain-view)
    (session/request session "getOption(\"oxmacs\")$inspect({(session/r-string object)})")))

(defcommand r-environment "List the objects in the R session"
  (when-let [running (session/ensure)]
    (session/link! running)
    (let [name (show-environment running)]
      (ask-environment running)
      (display-buffer name :select true))))

(defcommand r-environment-refresh "List the objects in the R session again"
  (when-let [listing (get @environments (buffer-name))]
    (ask-environment listing.session)))

(defcommand r-environment-visit "Open the object on this line"
  (let [listing (get @environments (buffer-name))
        line (current-line)
        found (when listing (find (fn [[row _]] (= row line)) listing.rows))]
    (if found
      (let [entry (second found)]
        (ask-object listing.session entry.name (contains? tabular entry.class)))
      (message "No object on this line"))))

(def (short-type name)
  "<{(or (get short-types name) (string/slice name 0 (min 4 (count name))))}>")

(def (table-of payload)
  (let [lines (string/lines payload)
        title (string/split (or (first lines) "") "\t")
        field (fn [at fallback] (let [given (get title at)] (if (or (nil? given) (= given "")) fallback given)))]
    (when (and (>= (count lines) 3) (>= (count title) 3))
      (let [rows (string/to-number (nth title 1))]
        {object: (first title)
         rows: rows
         columns: (string/to-number (nth title 2))
         offset: (or (string/to-number (field 3 "0")) 0)
         total: (or (string/to-number (field 4 "")) rows)
         sort: (field 5 "")
         decreasing: (= (field 6 "FALSE") "TRUE")
         filter: (field 7 "")
         names: (vec (rest (string/split (nth lines 1) "\t")))
         types: (vec (map short-type (rest (string/split (nth lines 2) "\t"))))
         cells: (vec (map (fn [line] (string/split line "\t")) (drop 3 lines)))}))))

(def (clipped text width) (string/truncate (or text "") width))

(def (info table first-column last-column)
  (let [shown (count table.cells)
        range (if (> table.rows shown) " (rows {(+ table.offset 1)}–{(+ table.offset shown)})" "")
        sorted (if (= table.sort "") "" "  sorted by {table.sort} {(if table.decreasing "↓" "↑")}")
        kept (if (= table.filter "") "" "  {table.rows} of {table.total} rows where {table.filter}")]
    "{table.object}  {table.rows} × {table.columns}{range}{sorted}{kept}  columns {(+ first-column 1)}–{(+ last-column 1)}  ← →"))

(def (lay-out table first-column width)
  (let [label-width (min cell-width (apply max 0 (map (fn [row] (count (or (first row) ""))) table.cells)))
        column-width (fn [column]
                       (min cell-width
                            (apply max (count (or (get table.names column) ""))
                                   (count (or (get table.types column) ""))
                                   (map (fn [row] (count (or (get row (+ column 1)) ""))) table.cells))))
        shown ((fn fit [column used found]
                 (if (>= column (count table.names))
                   found
                   (let [columns (column-width column)]
                     (if (and (not (empty? found)) (> (+ used gap columns) width))
                       found
                       (fit (+ column 1) (+ used gap columns) (conj found [column columns (+ used gap)]))))))
               first-column label-width [])
        numeric (fn [column] (contains? #{"<dbl>" "<int>"} (get table.types column)))
        render (fn [label cell]
                 (string/trim-end
                  (apply str (string/pad-end (clipped label label-width) label-width)
                         (map (fn [[column columns _]]
                                (let [text (clipped (cell column) columns)]
                                  (str "  " (if (numeric column)
                                              (string/pad-start text columns)
                                              (string/pad-end text columns)))))
                              shown))))
        last-column (if (empty? shown) first-column (first (last shown)))]
    {header: [[[(info table first-column last-column) :comment]]
              [[(render "" (fn [column] (get table.names column))) :heading]]
              [[(render "" (fn [column] (get table.types column))) :comment]]]
     spans: (map (fn [[column columns start]] [column start (+ start columns)]) shown)
     lines: (map (fn [row]
                   (let [line (render (or (first row) "") (fn [column] (get row (+ column 1))))
                         split (min label-width (count line))]
                     [[(string/slice line 0 split) :comment] (string/slice line split)]))
                 table.cells)}))

(def (draw-table name width)
  (when-let [view (get @tables name)]
    (when view.table
      (let [drawn (lay-out view.table view.first (if (and width (> width 0)) width table-room))]
        (swap! tables assoc-in [name :spans] drawn.spans)
        (set-header name drawn.header)
        (set-lines name drawn.lines)))))

(def table-keys
  {"<right>" 'r-table-next-column "f" 'r-table-next-column
   "<left>" 'r-table-previous-column "b" 'r-table-previous-column
   "n" 'next-line "p" 'previous-line "SPC" 'scroll-up-command "DEL" 'scroll-down-command
   "s" 'r-table-sort "/" 'r-table-filter "]" 'r-table-next-page "[" 'r-table-previous-page
   "i" 'r-table-describe-column "g" 'r-table-reload "q" 'quit-window})

(def (open-table session object)
  (let [name "{(stem session)} {object}*"]
    (unless (buffer-exists? name)
      (open-buffer name [] :keys table-keys :show false
                   :on-resize (fn [width] (draw-table name width)))
      (session/own! name session))
    (swap! tables update name (fn [view] (merge {session: session object: object first: 0} view)))
    (display-buffer name :select true)
    name))

(def (tabled session payload)
  (when-let [table (table-of payload)]
    (let [name (open-table session table.object)
          kept (if (= table.filter "") "" " where {table.filter}")]
      (message "{table.object}: {table.rows} rows{kept}, {table.columns} columns")
      (swap! tables update name (fn [view] (assoc view :table table :first (if (= (get-in view [:table :object]) table.object) (or view.first 0) 0))))
      (draw-table name (buffer-width name))
      (goto-line 0))))

(def (described session payload)
  (let [parts (string/split payload "\n" 2)
        object (first parts)
        name (open-table session object)]
    (swap! tables update name (fn [view] (assoc view :table nil)))
    (set-header name [[["str({object})" :comment]]])
    (set-lines name (string/lines (or (second parts) "")))
    (goto-line 0)))

(def (shift forward)
  (when-let [view (get @tables (buffer-name))]
    (when view.table
      (let [last-column (max 0 (- (count view.table.names) 1))
            next (if forward (min (+ view.first 1) last-column) (max (- view.first 1) 0))]
        (if (= next view.first)
          (message "Already at the {(if forward "last" "first")} column")
          (do (swap! tables assoc-in [(buffer-name) :first] next)
            (draw-table (buffer-name) (buffer-width (buffer-name)))))))))

(def (table-here)
  (when-let [view (get @tables (buffer-name))]
    (when view.table view)))

(def (column-here view)
  (let [at (current-column)]
    (when-let [[column _ _] (find (fn [[_ start end]] (and (<= start at) (< at end))) (or view.spans []))]
      column)))

(def (current-view view)
  {offset: view.table.offset sort: view.table.sort decreasing: view.table.decreasing filter: view.table.filter})

(defcommand r-table-next-column "Show the next columns of the table" (shift true))

(defcommand r-table-previous-column "Show the previous columns of the table" (shift false))

(defcommand r-table-sort "Sort the table by the column at point, up, then down, then not"
  (when-let [view (table-here)]
    (if-let [column (column-here view)]
      (let [name (get view.table.names column)
            now (current-view view)
            next (cond ((not= now.sort name) (assoc now :sort name :decreasing false))
                       ((not now.decreasing) (assoc now :decreasing true))
                       (else (assoc now :sort "" :decreasing false)))]
        (ask-table view.session view.object (assoc next :offset 0)))
      (message "No column here"))))

(defcommand r-table-filter "Keep the rows of the table where an R expression is true"
  (when-let [view (table-here)]
    (let [typed (await (prompt "Keep the rows of {view.object} where (R): " :initial view.table.filter))]
      (ask-table view.session view.object (assoc (current-view view) :filter (string/trim typed) :offset 0)))))

(def (turn-page forward)
  (when-let [view (table-here)]
    (let [offset view.table.offset
          next (if forward (+ offset view-rows) (- offset view-rows))]
      (cond ((< next 0) (message "This is the first page"))
            ((>= next view.table.rows) (message "This is the last page"))
            (else (ask-table view.session view.object (assoc (current-view view) :offset next)))))))

(defcommand r-table-next-page "Show the next thousand rows of the table" (turn-page true))

(defcommand r-table-previous-page "Show the previous thousand rows of the table" (turn-page false))

(defcommand r-table-describe-column "Describe the column at point: its spread or its commonest values"
  (when-let [view (table-here)]
    (if-let [column (column-here view)]
      (session/request view.session
                       "getOption(\"oxmacs\")$stats({(session/r-string view.object)}, {(session/r-string (get view.table.names column))})")
      (message "No column here"))))

(def (described-column _ payload)
  (let [[column kind text] (string/split payload "\t" 3)]
    (message "{column} {(short-type (or kind ""))}: {text}")))

(defcommand r-table-reload "Ask R for the table again"
  (when-let [view (get @tables (buffer-name))]
    (if view.table
      (ask-table view.session view.object (current-view view))
      (ask-object view.session view.object false))))
