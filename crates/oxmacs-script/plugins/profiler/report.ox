(module
  (import (ox path) (ox string))
  (export rows first-row))

(def (fixed number places)
  (let [scale (if (= places 1) 10 100)
        whole (round (* number scale))
        integer (int (// whole scale))
        fraction (- whole (* integer scale))]
    (str integer "." (string/pad-start (str fraction) places "0"))))

(def (shown ms)
  (if (>= ms 1000)
    (str (fixed (/ ms 1000.0) 2) " s")
    (str (fixed ms 2) " ms")))

(def (percent share)
  (str (fixed share 1) "%"))

(def (placed row)
  (if row.file (str (path/name row.file) ":" row.line) ""))

(def (face kind)
  (cond ((= kind :plugin) :heading)
    ((= kind :native) :constant)
    (else :function)))

(def (columns label kind first second calls row)
  [[(string/pad-end label 44) (face kind)]
   (string/pad-start first 8)
   (string/pad-start second 8)
   (string/pad-start calls 10)
   "  "
   [(placed row) :comment]])

(def (titles title first second)
  {line: [[(string/pad-end title 44) :heading]
          (string/pad-start first 8)
          (string/pad-start second 8)
          (string/pad-start "calls" 10)]})

(def (header found flat)
  [{line: [[(str "Ox profile  " (shown found.busy) " of Ox in " (shown found.wall) ", " found.samples " samples") :heading]]}
   {line: [[(str "TAB folds   f shows " (if flat "the call tree" "functions by self time") "   RET visits   g refreshes   q quits") :comment]]}
   {line: []}])

(def (ticks found)
  (if (empty? found.ticks)
    []
    (concat
     [{line: [["Slowest ticks" :heading]]}]
     (map (fn [tick] {line: [(str "  " (string/pad-start (shown tick.took) 10) "  " tick.kind " at " (shown tick.at))]})
          found.ticks)
     [{line: []}])))

(def (branch-rows branches depth path folded)
  (mapcat
   (fn [branch]
     (let [here (conj path branch.name)
           open (not (contains? folded here))
           marker (cond ((empty? branch.children) "  ") (open "▾ ") (else "▸ "))
           calls (if (= branch.kind :plugin) "" (str branch.calls))
           line (columns (str (string/repeat "  " depth) marker branch.name) branch.kind
                         (percent branch.total-share) (percent branch.self-share) calls branch)]
       (concat [{line: line path: here file: branch.file line-number: branch.line}]
               (if open (branch-rows branch.children (+ depth 1) here folded) []))))
   branches))

(def (function-rows found)
  (map (fn [function]
         {line: (columns (str "  " function.name) function.kind
                         (percent function.self-share) (percent function.total-share)
                         (str function.calls) function)
          file: function.file line-number: function.line})
       found.functions))

(def (rows found folded flat)
  (vec (concat (header found flat)
               (ticks found)
               (if flat
                 (concat [(titles "Functions by self time" "self" "total")] (function-rows found))
                 (concat [(titles "Call tree" "total" "self")] (branch-rows found.tree 0 [] folded))))))

(def (first-row rows)
  (or (some (fn [i] (when (or (get (nth rows i) :path) (get (nth rows i) :file)) i))
            (range (count rows)))
      0))
