(module
  (import (ox string) (ox path) (ox file))
  (export sorted shown? rows filtered parse-status roll-up roll-up-problems line))

(def (sorted entries)
  "entries in the order file/list gives: folders first, then by name ignoring case, hidden ones last."
  (file/sorted entries))

(def (shown? entry hidden ignored)
  "Whether entry shows: never .git, and hidden or ignored files only when hidden is true."
  (and (not= entry.name ".git")
       (or hidden (and (not entry.hidden) (not (contains? ignored entry.path))))))

(def (walk model folder depth out)
  (reduce (fn [out entry]
            (let [open (and entry.dir (contains? model.open entry.path))
                  out (conj out {path: entry.path name: entry.name dir: entry.dir depth: depth open: open})]
              (if open (walk model entry.path (inc depth) out) out)))
          out
          (filter (fn [entry] (shown? entry model.hidden model.ignored))
                  (sorted (get model.entries folder [])))))

(def (rows model)
  "The rows of the tree under model.root, each {{path name dir depth open}}, folders first, walking into the open folders whose entries are known."
  (walk model model.root 0 []))

(def (prefixes parts)
  (map (fn [n] (string/join "/" (take n parts))) (range 1 (count parts))))

(def (order-key [rel dir])
  (let [parts (string/split rel "/")
        last-at (dec (count parts))]
    (vec (map (fn [at] [(if (or dir (< at last-at)) 0 1) (string/lower (nth parts at))])
              (range (count parts))))))

(def (filtered files text join)
  "Rows for the files, given as /-separated paths, whose names hold text ignoring case, under open rows for the folders holding them; join turns a vector of path parts into a path."
  (let [needle (string/lower text)
        hits (filter (fn [rel] (string/includes? (string/lower (last (string/split rel "/"))) needle)) files)
        folders (distinct (mapcat (fn [rel] (prefixes (string/split rel "/"))) hits))
        all (concat (map (fn [rel] [rel true]) folders) (map (fn [rel] [rel false]) hits))]
    (map (fn [[rel dir]]
           (let [parts (string/split rel "/")]
             {path: (join parts) name: (last parts) dir: dir depth: (dec (count parts)) open: dir match: (not dir)}))
         (sort-by order-key all))))

(def (state-of code)
  (cond
    [(= code "??") :untracked]
    [(= code "!!") :ignored]
    [(or (string/includes? code "U") (= code "AA") (= code "DD")) :conflict]
    [(string/includes? code "D") :deleted]
    [(string/starts-with? code "A") :added]
    [else :modified]))

(def (parse-status text)
  "Reads the output of git status --porcelain=v1 -z --ignored into a map of /-separated paths to :modified, :added, :untracked, :deleted, :conflict or :ignored, folders without their trailing slash."
  (loop [items (string/split text "\u{0}")
         out {}]
    (if (empty? items)
      out
      (let [item (first items)
            more (rest items)]
        (if (< (count item) 4)
          (again more out)
          (let [code (string/slice item 0 2)
                rel (string/slice item 3 (count item))
                rel (if (string/ends-with? rel "/") (string/slice rel 0 (dec (count rel))) rel)
                more (if (or (string/starts-with? code "R") (string/starts-with? code "C")) (rest more) more)]
            (again more (assoc out rel (state-of code)))))))))

(def rank {conflict: 5 deleted: 4 modified: 3 added: 2 untracked: 1})

(def (stronger a b)
  (if (> (get rank a 0) (get rank b 0)) a b))

(def (roll-up states)
  "Each folder holding changed files, as a /-separated path, mapped to the strongest change inside it; ignored files don't count."
  (reduce (fn [out [rel state]]
            (if (= state :ignored)
              out
              (reduce (fn [out folder] (assoc out folder (stronger state (get out folder))))
                      out
                      (prefixes (string/split rel "/")))))
          {}
          (entries states)))

(def (add-problems a b)
  {errors: (+ (get a :errors 0) b.errors) warnings: (+ (get a :warnings 0) b.warnings)})

(def (roll-up-problems problems root)
  "problems, a map of file paths to {{errors warnings}}, with each folder between a file and root added up too."
  (reduce (fn [out [file counts]]
            (loop [folder (path/parent file)
                   out out]
              (if (and folder (string/starts-with? folder root) (not= folder file))
                (again (if (= folder root) nil (path/parent folder))
                       (assoc out folder (add-problems (get out folder) counts)))
                out)))
          problems
          (entries problems)))

(def marks {modified: ["M" :warning] added: ["A" :success] untracked: ["U" :success] deleted: ["D" :error] conflict: ["!" :error]})

(def name-faces {modified: :warning added: :success untracked: :success deleted: :error conflict: :error ignored: :comment})

(def (badges row change problems)
  (let [errors (if problems problems.errors 0)
        warnings (if problems problems.warnings 0)
        mark (get marks change)]
    (vec (keep identity
               [(when (> errors 0) [(str errors) :error])
                (when (> warnings 0) [(str warnings) :warning])
                (when mark (if row.dir ["●" (second mark)] mark))]))))

(def (spaced parts)
  (vec (mapcat (fn [at] (if (= at 0) [(nth parts 0)] [" " (nth parts at)])) (range (count parts)))))

(def (named row change)
  (let [face (get name-faces change)]
    (if face [row.name face] row.name)))

(def (line row change problems width current)
  "The line showing row: indented by depth, a chevron on folders, its name in the color of its git change, and its error and warning counts and change at the right of width columns; current rows have the region's background."
  (let [lead (str (string/repeat "  " row.depth) (if row.dir (if row.open "▾ " "▸ ") "  "))
        right (spaced (badges row change problems))
        used (+ (count lead) (count row.name) (reduce + 0 (map (fn [part] (count (if (string? part) part (first part)))) right)))
        gap (string/repeat " " (max 1 (- width used 1)))]
    {line: (vec (concat [[lead :comment] (named row change) gap] right))
     background: (when current :region)}))

(test "folders come first, then names ignoring case"
  (expect (= (map :name (sorted [{name: "b" dir: false} {name: "Z" dir: true} {name: "A" dir: false}]))
             ["Z" "A" "b"])))

(test "hidden and ignored entries show only when asked, and .git never"
  (let [ignored #{"/r/target"}]
    (expect (shown? {name: "src" path: "/r/src" hidden: false} false ignored))
    (expect (not (shown? {name: ".env" path: "/r/.env" hidden: true} false ignored)))
    (expect (not (shown? {name: "target" path: "/r/target" hidden: false} false ignored)))
    (expect (shown? {name: "target" path: "/r/target" hidden: false} true ignored))
    (expect (not (shown? {name: ".git" path: "/r/.git" hidden: true} true ignored)))))

(test "rows walk into open folders"
  (let [model {root: "/r"
               open: #{"/r/src"}
               hidden: false
               ignored: #{}
               entries: {"/r" [{name: "README.md" path: "/r/README.md" dir: false hidden: false}
                               {name: "src" path: "/r/src" dir: true hidden: false}
                               {name: "docs" path: "/r/docs" dir: true hidden: false}]
                         "/r/src" [{name: "main.rs" path: "/r/src/main.rs" dir: false hidden: false}]}}]
    (expect (= (map (fn [row] [row.name row.depth row.open]) (rows model))
               [["docs" 0 false] ["src" 0 true] ["main.rs" 1 false] ["README.md" 0 false]]))))

(test "filtering keeps matching files under their open folders"
  (let [found (filtered ["src/main.rs" "src/lib.rs" "docs/Mainly.md" "README.md"] "main" (fn [parts] (string/join "/" parts)))]
    (expect (= (map (fn [row] [row.path row.depth row.dir]) found)
               [["docs" 0 true] ["docs/Mainly.md" 1 false] ["src" 0 true] ["src/main.rs" 1 false]]))))

(test "git status reads changes, renames and ignored folders"
  (expect (= (parse-status " M src/a.rs\u{0}A  b.rs\u{0}R  new.rs\u{0}old.rs\u{0}?? notes/\u{0}!! target/\u{0} D gone.rs\u{0}UU both.rs\u{0}")
             {"src/a.rs" :modified "b.rs" :added "new.rs" :modified "notes" :untracked "target" :ignored "gone.rs" :deleted "both.rs" :conflict})))

(test "folders show the strongest change inside them"
  (expect (= (roll-up {"src/a/x.rs" :untracked "src/b.rs" :modified "target" :ignored})
             {"src" :modified "src/a" :untracked})))

(test "problems add up in the folders above them"
  (let [root (path/join "r")
        file (path/join "r" "src" "main.rs")
        rolled (roll-up-problems {file {errors: 1 warnings: 2}} root)]
    (expect (= (get rolled (path/join "r" "src")) {errors: 1 warnings: 2}))
    (expect (= (get rolled file) {errors: 1 warnings: 2}))))

(test "lines put badges at the right edge"
  (let [shown (line {name: "main.rs" dir: false depth: 1 open: false} :modified {errors: 2 warnings: 0} 20 true)]
    (expect (= shown.line [["    " :comment] ["main.rs" :warning] "     " ["2" :error] " " ["M" :warning]]))
    (expect (= shown.background :region)))
  (expect (= (get (line {name: "src" dir: true depth: 0 open: true} :added nil 10 false) :line)
             [["▾ " :comment] ["src" :success] "   " ["●" :success]])))
