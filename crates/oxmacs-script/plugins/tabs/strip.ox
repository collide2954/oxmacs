(module
  (export insert-at remove-at move-item follow after-close label))

(def (insert-at items at item)
  "items with item put in before the one at index at."
  (vec (concat (take at items) [item] (drop at items))))

(def (remove-at items at)
  "items without the one at index at."
  (vec (concat (take at items) (drop (inc at) items))))

(def (move-item items from to)
  "items with the one at index from moved to index to."
  (insert-at (remove-at items from) to (nth items from)))

(def (follow current from to)
  "Where the tab at index current ends up after the tab at from moves to to."
  (cond
    [(= current from) to]
    [(and (< from current) (<= current to)) (dec current)]
    [(and (< to from) (<= to current) (< current from)) (inc current)]
    [else current]))

(def (after-close current closed n)
  "The index of the tab to show once the tab at closed of n tabs closes, while the one at current shows: its right neighbor, or its left one when it was last."
  (cond
    [(< closed current) (dec current)]
    [(> closed current) current]
    [(= closed (dec n)) (dec closed)]
    [else closed]))

(def (label tab shown)
  "What tab is called: the name it was given, else shown, the buffer of its selected window."
  (or tab.name shown tab.seen "*scratch*"))

(test "items go in and come out at an index"
  (expect (= (insert-at [:a :c] 1 :b) [:a :b :c]))
  (expect (= (insert-at [:a] 1 :b) [:a :b]))
  (expect (= (remove-at [:a :b :c] 1) [:a :c]))
  (expect (= (move-item [:a :b :c] 0 2) [:b :c :a]))
  (expect (= (move-item [:a :b :c] 2 0) [:c :a :b])))

(test "the current tab follows moves"
  (expect (= (follow 0 0 2) 2))
  (expect (= (follow 1 0 2) 0))
  (expect (= (follow 2 0 1) 2))
  (expect (= (follow 1 2 0) 2))
  (expect (= (follow 0 2 1) 0)))

(test "closing a tab shows its right neighbor, or its left one at the end"
  (expect (= (after-close 1 1 3) 1))
  (expect (= (after-close 2 2 3) 1))
  (expect (= (after-close 2 0 3) 1))
  (expect (= (after-close 0 2 3) 0)))

(test "tabs are called by their name, else the buffer they show"
  (expect (= (label {name: "work" seen: "a.rs"} "b.rs") "work"))
  (expect (= (label {name: nil seen: "a.rs"} "b.rs") "b.rs"))
  (expect (= (label {name: nil seen: "a.rs"} nil) "a.rs")))
