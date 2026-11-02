(module
  (import (ox process) (ox string) (ox path) (ox file) (ox time)
          (git diff))
  (export run git git-with act act-with history locate read-status read-head files-of find-change
          located diffs read-log read-commit read-branches))

(def base ["--no-pager" "--no-optional-locks" "-c" "color.ui=false" "-c" "core.quotepath=false"])

(def field "\u{1f}")

(def kept 200)

(defonce history (atom []))

(def (record! root args ran)
  (swap! history
         (fn [entries]
           (let [all (conj entries {root: root args: args status: ran.status at: (time/now)
                                    output: (string/trim-end (str ran.stdout ran.stderr))})]
             (if (> (count all) kept) (drop (- (count all) kept) all) all)))))

(def (native path)
  (if (= (path/separator) "\\") (string/replace path "/" "\\") path))

(def (run root args)
  (process/run "git" (concat base args) :cwd root))

(def (reason ran)
  (let [said (filter (fn [line] (not (string/blank? line)))
                     (string/lines (if (string/blank? ran.stderr) ran.stdout ran.stderr)))]
    (if (empty? said) $"git exited with {ran.status}" (string/trim (last said)))))

(def (output ran)
  (if (= ran.status 0) ran.stdout (raise (error $"Git: {(reason ran)}"))))

(def (git root args)
  (output (await (run root args))))

(def (git-with root args input)
  (output (process/run "git" (concat base args) :cwd root :input input)))

(def (act root args)
  (let [ran (await (run root args))]
    (record! root args ran)
    (output ran)))

(def (act-with root args input)
  (let [ran (process/run "git" (concat base args) :cwd root :input input)]
    (record! root args ran)
    (output ran)))

(def (climb directory cdup)
  (reduce (fn [dir part] (if (= part "..") (path/parent dir) dir))
          directory (string/split cdup "/")))

(def (locate directory)
  (let [ran (await (run directory ["rev-parse" "--show-cdup" "--git-dir"]))]
    (when (= ran.status 0)
      (let [found (string/lines ran.stdout)
            root (climb directory (nth found 0))
            metadata (nth found 1)]
        {root: root
         metadata: (cond
                     [(not (path/absolute? metadata)) (path/join directory metadata)]
                     [(= (path/name metadata) ".git") (path/join root ".git")]
                     [else (native metadata)])}))))

(def (records text)
  (filter (fn [record] (not (= record ""))) (string/split text "\0")))

(def kinds {"M" :modified "T" :type-change "A" :added "D" :deleted
            "R" :renamed "C" :copied "U" :conflict})

(def (kind-of letter)
  (get kinds letter :modified))

(def (header found record)
  (let [parts (string/split record " " 3)
        key (nth parts 1)
        value (if (> (count parts) 2) (nth parts 2) "")]
    (cond
      [(= key "branch.oid") (assoc found :born (not (= value "(initial)")) :oid value)]
      [(= key "branch.head") (assoc found :branch (if (= value "(detached)") nil value))]
      [(= key "branch.upstream") (assoc found :upstream {name: value ahead: 0 behind: 0})]
      [(and (= key "branch.ab") found.upstream)
       (let [counts (string/split value " ")]
         (assoc found :upstream
                (assoc found.upstream
                       :ahead (string/to-number (nth counts 0))
                       :behind (- (string/to-number (nth counts 1))))))]
      [else found])))

(def (changed found fields from)
  (let [xy (nth fields 1)
        x (string/slice xy 0 1)
        y (string/slice xy 1 2)
        path (last fields)
        found (if (= x ".")
                found
                (update found :staged conj {path: path kind: (kind-of x) from: from}))]
    (if (= y ".")
      found
      (update found :unstaged conj {path: path kind: (kind-of y) from: nil}))))

(def (parse-status text)
  (let [items (records text)
        n (count items)]
    (loop [i 0 found {branch: nil born: true upstream: nil untracked: [] unstaged: [] staged: []}]
      (if (>= i n)
        found
        (let [record (nth items i)
              tag (string/slice record 0 1)]
          (cond
            [(= tag "#") (again (+ i 1) (header found record))]
            [(= tag "1") (again (+ i 1) (changed found (string/split record " " 9) nil))]
            [(= tag "2") (again (+ i 2) (changed found (string/split record " " 10) (nth items (+ i 1))))]
            [(= tag "u")
             (again (+ i 1) (update found :unstaged conj
                                    {path: (last (string/split record " " 11)) kind: :conflict from: nil}))]
            [(= tag "?") (again (+ i 1) (update found :untracked conj (string/slice record 2)))]
            [else (again (+ i 1) found)]))))))

(def (parse-numstat section text)
  (let [items (string/split text "\0")
        n (count items)]
    (loop [i 0 counts {}]
      (if (>= i n)
        counts
        (let [parts (string/split (nth items i) "\t" 3)]
          (if (< (count parts) 3)
            (again (+ i 1) counts)
            (let [added (nth parts 0)
                  counted (if (= added "-")
                            :binary
                            [(string/to-number added) (string/to-number (nth parts 1))])
                  path (nth parts 2)]
              (if (= path "")
                (again (+ i 3) (assoc counts [section (nth items (+ i 2))] counted))
                (again (+ i 1) (assoc counts [section path] counted))))))))))

(def (parse-stashes text)
  (map (fn [record]
         (let [parts (string/split record field 2)]
           {name: (nth parts 0) summary: (if (> (count parts) 1) (nth parts 1) "")}))
       (records text)))

(def (parse-log text)
  (map (fn [record]
         (let [parts (string/split record field 4)]
           {id: (nth parts 0) summary: (nth parts 1) author: (nth parts 2)
            time: (string/to-number (nth parts 3))}))
       (records text)))

(def counted-limit 200)
(def large 1048576)

(def (untracked-count root path)
  (if (string/ends-with? path "/")
    nil
    (try
      (let [full (path/join root path)
            found (file/metadata full)]
        (if (> (or found.size 0) large)
          :large
          [(count (string/lines (file/read full))) 0]))
      (catch error _e :binary))))

(def (untracked-counts root paths)
  (let [counted (if (> (count paths) counted-limit)
                  (map (fn [i] (nth paths i)) (range counted-limit))
                  paths)]
    (reduce (fn [counts path]
              (let [found (untracked-count root path)]
                (if found (assoc counts [:untracked path] found) counts)))
            {} counted)))

(def (merged & maps)
  (reduce (fn [all more] (reduce (fn [all key] (assoc all key (get more key))) all (keys more)))
          {} maps))

(def (quietly-read ran)
  (if (= ran.status 0) ran.stdout ""))

(def (read-status root)
  (let [status (spawn (run root ["status" "--porcelain=v2" "--branch" "-z"]))
        unstaged (spawn (run root ["diff" "--numstat" "-z" "--no-ext-diff"]))
        staged (spawn (run root ["diff" "--cached" "--numstat" "-z" "--no-ext-diff"]))
        stashes (spawn (run root ["stash" "list" "-z" "--format=%gd%x1f%gs"]))
        recent (spawn (run root ["log" "-n" "10" "-z" "--format=%h%x1f%s%x1f%an%x1f%at"]))
        found (parse-status (output (await status)))]
    (assoc found
           :stashes (parse-stashes (quietly-read (await stashes)))
           :recent (parse-log (quietly-read (await recent)))
           :counts (merged (parse-numstat :unstaged (quietly-read (await unstaged)))
                           (parse-numstat :staged (quietly-read (await staged)))
                           (untracked-counts root found.untracked)))))

(def (read-head root)
  (let [ran (await (run root ["status" "--porcelain=v2" "--branch" "-z"]))]
    (when (= ran.status 0)
      (let [found (parse-status ran.stdout)]
        {branch: found.branch upstream: found.upstream
         id: (when (and found.born found.oid) (string/slice found.oid 0 7))
         dirty: (not (and (empty? found.staged) (empty? found.unstaged) (empty? found.untracked)))}))))

(def (ref-of text)
  (let [given (string/trim text)
        head (string/starts-with? given "HEAD -> ")
        part (if head (string/slice given 8) given)]
    (cond
      [(= part "") nil]
      [(= part "HEAD") {name: "HEAD" kind: :detached head: true}]
      [(string/starts-with? part "tag: refs/tags/") {name: (string/slice part 15) kind: :tag head: false}]
      [(string/starts-with? part "refs/heads/") {name: (string/slice part 11) kind: :local head: head}]
      [(and (string/starts-with? part "refs/remotes/") (not (string/ends-with? part "/HEAD")))
       {name: (string/slice part 13) kind: :remote head: false}]
      [else nil])))

(def (parse-refs text)
  (keep ref-of (string/split text ", ")))

(def (numbers-in text)
  (filter (fn [part] (not (= part ""))) (string/split text " ")))

(def log-format "--format=%H%x1f%h%x1f%P%x1f%an%x1f%at%x1f%D%x1f%s")

(def (parse-commits text)
  (map (fn [record]
         (let [parts (string/split record field 7)]
           {id: (nth parts 0) short: (nth parts 1) parents: (numbers-in (nth parts 2))
            author: (nth parts 3) time: (string/to-number (nth parts 4))
            refs: (parse-refs (nth parts 5)) summary: (nth parts 6)}))
       (records text)))

(def (read-log root limit all)
  (let [ran (await (run root (concat ["log" "-z" "--topo-order" "--decorate=full" log-format
                                      "-n" (str limit)]
                                     (if all ["--all"] []))))]
    (if (= ran.status 0) (parse-commits ran.stdout) [])))

(def show-format "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%D%x1f%B")

(def (read-commit root id)
  (let [head (spawn (run root ["show" "-s" "--decorate=full" show-format id]))
        patch (spawn (run root ["show" "--format=" "-M" "--no-ext-diff" "--diff-merges=first-parent" id]))
        parts (string/split (string/trim-end (output (await head))) field 9)]
    {id: (nth parts 0) parents: (numbers-in (nth parts 1))
     author: (nth parts 2) email: (nth parts 3) time: (string/to-number (nth parts 4))
     committer: (nth parts 5) committer-email: (nth parts 6)
     refs: (parse-refs (nth parts 7)) message: (string/trim (nth parts 8))
     files: (diff/files (quietly-read (await patch)))}))

(def branch-format
  "--format=%(refname)%1f%(refname:short)%1f%(objectname:short)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(HEAD)%1f%(committerdate:unix)%1f%(contents:subject)")

(def (tracked text)
  (reduce (fn [found part]
            (let [words (string/split (string/trim part) " ")]
              (cond
                [(= (nth words 0) "ahead") (assoc found :ahead (string/to-number (nth words 1)))]
                [(= (nth words 0) "behind") (assoc found :behind (string/to-number (nth words 1)))]
                [(= (nth words 0) "gone") (assoc found :gone true)]
                [else found])))
          {ahead: 0 behind: 0 gone: false}
          (filter (fn [part] (not (string/blank? part))) (string/split text ","))))

(def (branch-of line)
  (let [parts (string/split line field 8)
        ref (nth parts 0)
        kind (cond
               [(string/starts-with? ref "refs/heads/") :local]
               [(and (string/starts-with? ref "refs/remotes/") (not (string/ends-with? ref "/HEAD"))) :remote]
               [(string/starts-with? ref "refs/tags/") :tag]
               [else nil])]
    (when (and kind (= (count parts) 8))
      (merge {kind: kind ref: ref
              name: (string/slice ref (cond [(= kind :local) 11] [(= kind :remote) 13] [else 10]))
              id: (nth parts 2) upstream: (when (not (= (nth parts 3) "")) (nth parts 3))
              head: (= (nth parts 5) "*") time: (or (string/to-number (nth parts 6)) 0)
              summary: (nth parts 7)}
             (tracked (nth parts 4))))))

(def (read-branches root)
  (let [ran (await (run root ["for-each-ref" "--sort=-committerdate" branch-format
                              "refs/heads" "refs/remotes" "refs/tags"]))]
    (if (= ran.status 0)
      (keep branch-of (filter (fn [line] (not (string/blank? line))) (string/lines ran.stdout)))
      [])))

(def (files-of snapshot section)
  (if (= section :untracked)
    (map (fn [path] {path: path kind: :added from: nil}) snapshot.untracked)
    (get snapshot section)))

(def (find-change snapshot section path)
  (first (filter (fn [change] (= change.path path)) (files-of snapshot section))))

(def (located snapshot key)
  (let [path (nth key 1)]
    (first (keep (fn [section] (when (find-change snapshot section path) [section path]))
                 [(nth key 0) :unstaged :staged :untracked]))))

(def (file-diff root section change)
  (cond
    [(= section :untracked)
     (if (string/ends-with? change.path "/")
       []
       (diff/whole-file (file/read (path/join root change.path))))]
    [(= section :unstaged)
     (diff/parse (git root ["diff" "--no-ext-diff" "--" change.path]))]
    [else
     (diff/parse (git root (concat ["diff" "--cached" "--no-ext-diff" "-M" "--" change.path]
                                   (if change.from [change.from] []))))]))

(def (diffs root snapshot opened)
  (reduce (fn [found key]
            (let [change (find-change snapshot (nth key 0) (nth key 1))]
              (if change
                (assoc found key (try (file-diff root (nth key 0) change) (catch error _e [])))
                found)))
          {} (keys opened)))
