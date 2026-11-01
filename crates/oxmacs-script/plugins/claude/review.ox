(module
  (import (ox string) (ox file) (ox path)
          (git diff :as diff)
          (git pane :as pane)
          (claude where :only [in-main]))
  (export review-buffer located hunks-of edits! write! decide! decide-all! finish! closed! reviewing?))

(def review-buffer "*claude review*")
(def context 3)
(def widest-match 40000)
(def review-keys {"y" 'claude-accept-hunk "n" 'claude-reject-hunk "a" 'claude-accept-all
                  "r" 'claude-reject-all "C-c C-c" 'claude-finish-review "q" 'claude-finish-review})

(defonce current (atom nil))

(def (reviewing?) (some? @current))

(def (occurrences text part)
  (dec (count (string/split text part))))

(def (located text edits)
  "Each edit with where its old_text is in text, in order; raises when one isn't there exactly once or two overlap."
  (let [found (vec (map (fn [i]
                          (let [edit (nth edits i)
                                old (get edit :old_text)
                                new (get edit :new_text)
                                n (inc i)]
                            (unless (and (string? old) (string? new))
                              (raise (error $"Edit {n} needs old_text and new_text")))
                            (when (= old "")
                              (raise (error $"Edit {n} has an empty old_text")))
                            (let [seen (occurrences text old)]
                              (when (= seen 0)
                                (raise (error $"Edit {n}'s old_text isn't in the text; read it again")))
                              (when (> seen 1)
                                (raise (error $"Edit {n}'s old_text appears {seen} times; give more of the text around it"))))
                            (let [start (string/index-of text old)]
                              {index: i start: start end: (+ start (string/length old)) old: old new: new})))
                        (range (count edits))))
        sorted (vec (sort-by :start found))]
    (each [k (range 1 (count sorted))]
      (when (< (get (nth sorted k) :start) (get (nth sorted (dec k)) :end))
        (raise (error "Two edits overlap; make them one"))))
    sorted))

(def (line-at text offset)
  (dec (count (string/split (string/slice text 0 offset) "\n"))))

(def (start-of rows line)
  (reduce + 0 (map (fn [row] (inc (string/length row))) (take line rows))))

(def (between rows from to)
  (vec (take (max 0 (- to from)) (drop from rows))))

(def (line-diff old new)
  (if (> (* (count old) (count new)) widest-match)
    (vec (concat (map (fn [text] {kind: :removed text: text}) old)
                 (map (fn [text] {kind: :added text: text}) new)))
    (let [[left right] (diff/matches old new)]
      (loop [i 0 j 0 out []]
        (cond
          [(and (>= i (count old)) (>= j (count new))) out]
          [(and (< i (count old)) (not (get left i))) (again (inc i) j (conj out {kind: :removed text: (nth old i)}))]
          [(and (< j (count new)) (not (get right j))) (again i (inc j) (conj out {kind: :added text: (nth new j)}))]
          [else (again (inc i) (inc j) (conj out {kind: :context text: (nth old i)}))])))))

(def (hunk-of text rows change shift)
  (let [first (line-at text change.start)
        last (line-at text change.end)
        region-start (start-of rows first)
        region-end (+ (start-of rows last) (string/length (nth rows last)))
        old (between rows first (inc last))
        new (string/split (str (string/slice text region-start change.start)
                               change.new
                               (string/slice text change.end region-end))
                          "\n")
        before (between rows (max 0 (- first context)) first)
        after (between rows (inc last) (+ last 1 context))
        start (inc (- first (count before)))
        around (fn [texts] (map (fn [text] {kind: :context text: text}) texts))]
    {old-start: start old-count: (+ (count before) (count old) (count after))
     new-start: (+ start shift) new-count: (+ (count before) (count new) (count after))
     lines: (vec (concat (around before) (line-diff old new) (around after)))
     grows: (- (count new) (count old))}))

(def (hunks-of text changes)
  "The hunks showing changes, as located gives them, made to text."
  (let [rows (string/split text "\n")]
    (loop [i 0 shift 0 out []]
      (if (>= i (count changes))
        out
        (let [hunk (hunk-of text rows (nth changes i) shift)]
          (again (inc i) (+ shift hunk.grows) (conj out (dissoc hunk :grows))))))))

(def (status-piece decision)
  (cond
    [(= decision :accepted) ["  ✓ accepted" :success]]
    [(= decision :rejected) ["  ✗ rejected" :error]]
    [else ["  y accepts, n rejects" :comment]]))

(def (shown-lines state)
  (let [out (pane/render {section: :claude path: state.title label: "Claude's edits"} state.hunks :unified 120)]
    (vec (map (fn [row]
                (if (and (map? row) (= row.kind :hunk))
                  (update row :line (fn [pieces] (conj (vec pieces) (status-piece (nth state.decisions row.hunk)))))
                  row))
              out.lines))))

(def (show!)
  (let [state @current
        lines (shown-lines state)]
    (if (buffer-exists? review-buffer)
      (set-lines review-buffer lines)
      (in-main (fn []
                 (let [made (open-buffer review-buffer lines :keys review-keys :label "Claude review" :line 2)]
                   (swap! current assoc :handle made))
                 (set-header review-buffer [[["y" :keyword] " accept  " ["n" :keyword] " reject  "
                                             ["a" :keyword] " or " ["r" :keyword] " all  "
                                             ["C-c C-c" :keyword] " make the accepted edits"]]))))))

(def (decide! decision)
  "Accepts or rejects the hunk at point in the review."
  (let [item (line-item)]
    (when (and @current item (some? (get item :hunk)))
      (swap! current update :decisions assoc item.hunk decision)
      (show!))))

(def (decide-all! decision)
  (when @current
    (swap! current update :decisions (fn [decisions] (vec (map (fn [_] decision) decisions))))
    (show!)))

(def (finish!)
  "Ends the review, making the accepted edits."
  (when-let [state @current]
    (unless (done? state.answer)
      (deliver state.answer state.decisions))))

(def (closed! buffer)
  "Ends the review as it stands when its buffer was closed."
  (when-let [state @current]
    (when (= buffer state.handle)
      (finish!))))

(def (reviewed! title hunks)
  "Shows hunks for review and waits for the decisions, one per hunk."
  (when @current
    (raise (error "Another review of Claude's edits is open; finish it first")))
  (let [answer (promise)
        from-chat (get (side-window) :selected)]
    (reset! current {title: title hunks: hunks answer: answer
                     decisions: (vec (map (fn [_] :pending) hunks))})
    (show!)
    (when from-chat (select-main-window))
    (try (await answer)
      (finally
        (reset! current nil)
        (close-buffer review-buffer)
        (when (and from-chat (side-window)) (select-side-window))))))

(def (buffer-for target)
  (if (buffer-exists? target)
    target
    (in-main (fn []
               (unless (open-file target)
                 (raise (error $"Couldn't open {target}")))
               (buffer-name)))))

(def (apply! buffer change)
  (with-buffer buffer
    (let [text (buffer-text)
          here (if (= (string/slice text change.start (min (string/length text) change.end)) change.old)
                 change.start
                 (when (= (occurrences text change.old) 1) (string/index-of text change.old)))]
      (when here
        (delete here (+ here (string/length change.old)))
        (set-point here)
        (insert change.new)
        true))))

(def (told made skipped rejected)
  (string/join " " (keep identity
                         [(if (empty? made) "No edits were made." $"Made edits {(string/join ", " made)}.")
                          (when (not (empty? rejected)) $"The person rejected edits {(string/join ", " rejected)}.")
                          (when (not (empty? skipped)) $"Edits {(string/join ", " skipped)} no longer matched the text and were left out.")
                          (when (not (empty? made)) "They are in the buffer, not yet saved.")])))

(def (edits! target edits)
  "Shows Claude's edits to target, a buffer or a file, for review and makes the accepted ones."
  (let [buffer (buffer-for target)
        changes (located (buffer-contents buffer) edits)
        decisions (reviewed! buffer (hunks-of (buffer-contents buffer) changes))
        numbered (fn [change] (str (inc change.index)))
        accepted (vec (filter (fn [i] (= (nth decisions i) :accepted)) (range (count changes))))
        made (vec (filter (fn [i] (apply! buffer (nth changes i))) (reverse accepted)))]
    (told (vec (map (fn [i] (numbered (nth changes i))) (reverse made)))
          (vec (map (fn [i] (numbered (nth changes i))) (remove (fn [i] (contains? (set made) i)) accepted)))
          (vec (map (fn [i] (numbered (nth changes i))) (remove (fn [i] (= (nth decisions i) :accepted)) (range (count changes))))))))

(def (write! path content)
  "Shows the file Claude wants written at path for review and writes it when accepted."
  (if (some (fn [buffer] (= (with-buffer buffer (buffer-file)) path)) (buffer-list))
    (edits! path [{old_text: (with-buffer (buffer-for path) (buffer-text)) new_text: content}])
    (let [old (if (file/exists? path) (file/read path) nil)
          hunks (if old
                  (hunks-of old [{index: 0 start: 0 end: (string/length old) old: old new: content}])
                  (diff/whole-file content))
          decisions (reviewed! path hunks)]
      (if (every? (fn [decision] (= decision :accepted)) decisions)
        (do (file/make-dir (path/parent path))
          (file/write path content)
          (in-main (fn [] (open-file path)))
          $"Wrote {path} and opened it.")
        $"The person didn't want {path} written."))))

(test "edits are found once each, in order"
  (let [found (located "one two three" [{old_text: "three" new_text: "3"} {old_text: "one" new_text: "1"}])]
    (expect (= (map (fn [change] [change.index change.start change.end]) found) [[1 0 3] [0 8 13]])))
  (expect (= (try (located "a a" [{old_text: "a" new_text: "b"}]) (catch error e e.message))
             "Edit 1's old_text appears 2 times; give more of the text around it"))
  (expect (= (try (located "abc" [{old_text: "x" new_text: "y"}]) (catch error e e.message))
             "Edit 1's old_text isn't in the text; read it again"))
  (expect (= (try (located "abcd" [{old_text: "abc" new_text: ""} {old_text: "cd" new_text: ""}]) (catch error e e.message))
             "Two edits overlap; make them one")))

(test "a hunk shows whole lines with the lines around them"
  (let [text "a\nb\nc\nd\ne\nf\ng\nh\ni"
        [hunk] (hunks-of text (located text [{old_text: "e\nf" new_text: "E\nf\nF"}]))]
    (expect (= [hunk.old-start hunk.old-count hunk.new-start hunk.new-count] [2 8 2 9]))
    (expect (= (map (fn [line] [line.kind line.text]) hunk.lines)
               [[:context "b"] [:context "c"] [:context "d"]
                [:removed "e"] [:added "E"] [:context "f"] [:added "F"]
                [:context "g"] [:context "h"] [:context "i"]]))))

(test "later hunks start where the earlier edits moved them"
  (let [text "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk"
        hunks (hunks-of text (located text [{old_text: "b" new_text: "b\nb2"} {old_text: "j" new_text: "J"}]))]
    (expect (= (map (fn [hunk] [hunk.old-start hunk.new-start]) hunks) [[1 1] [7 8]]))))
