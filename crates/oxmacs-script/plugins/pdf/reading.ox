(module
  (import (ox string))
  (export label marks first-match step page-number outline-labels zoomed safe-link?))

(def (label doc)
  "The mode line's account of where doc is shown: page of pages and the scale."
  "{(inc (or doc.page 0))}/{doc.pages} · {(or doc.percent 100)}%")

(def (marks matches at page)
  "The marks for the matches found near page, the one numbered at drawn as current."
  (vec (mapcat (fn [index]
                 (let [found (nth matches index)]
                   (if (<= (abs (- found.page page)) 3)
                     (map (fn [rect] {page: found.page rect: rect current: (= index at)}) found.rects)
                     [])))
               (range (count matches)))))

(def (first-match matches page step)
  "The match to show first when searching from page: the first on it or after when step is 1, the last on it or before when step is -1."
  (let [indices (range (count matches))]
    (if (> step 0)
      (or (find (fn [index] (>= (get (nth matches index) :page) page)) indices) 0)
      (or (last (filter (fn [index] (<= (get (nth matches index) :page) page)) indices))
          (dec (count matches))))))

(def (step matches at page by)
  "The match after the one numbered at by by, wrapping around, or the first one from page when none is shown yet."
  (cond
    [(empty? matches) nil]
    [(nil? at) (first-match matches page by)]
    [else (mod (+ at by) (count matches))]))

(def (page-number text pages)
  "The page index that text names, counting from 1, or nil when it names none of pages."
  (let [number (string/to-number (string/trim text))]
    (when (and (int? number) (>= number 1) (<= number pages))
      (dec number))))

(def (outline-labels outline)
  "The outline's headings, indented by depth, with their page numbers."
  (vec (map (fn [heading] "{(string/repeat "  " heading.depth)}{heading.title} · p. {(inc heading.page)}") outline)))

(def (zoomed percent factor)
  "The scale after zooming the scale shown, percent, by factor, kept between a quarter and eight times."
  (/ (round (* 100 (max 0.25 (min 8 (* (/ percent 100) factor))))) 100))

(def (safe-link? address)
  "True for web and mail addresses, which open outside oxmacs."
  (let [lower (string/lower address)]
    (or (string/starts-with? lower "http://") (string/starts-with? lower "https://")
        (string/starts-with? lower "mailto:"))))

(test "the mode line counts pages from one"
  (expect (= (label {page: 2 pages: 10 percent: 125}) "3/10 · 125%"))
  (expect (= (label {pages: 4}) "1/4 · 100%")))

(test "marks cover matches near the page, the current one marked"
  (let [matches [{page: 0 rects: [[1 2 3 4]]} {page: 1 rects: [[5 6 7 8] [9 10 11 12]]} {page: 9 rects: [[0 0 1 1]]}]]
    (expect (= (marks matches 1 0)
               [{page: 0 rect: [1 2 3 4] current: false}
                {page: 1 rect: [5 6 7 8] current: true}
                {page: 1 rect: [9 10 11 12] current: true}]))))

(test "searching starts from the page in view and wraps around"
  (let [matches [{page: 0} {page: 3} {page: 3} {page: 7}]]
    (expect (= (first-match matches 2 1) 1))
    (expect (= (first-match matches 8 1) 0))
    (expect (= (first-match matches 5 -1) 2))
    (expect (= (first-match matches 0 -1) 0))
    (expect (= (step matches nil 4 1) 3))
    (expect (= (step matches 3 0 1) 0))
    (expect (= (step matches 0 0 -1) 3))
    (expect (nil? (step [] nil 0 1)))))

(test "page numbers count from one and stay inside the document"
  (expect (= (page-number " 12 " 20) 11))
  (expect (nil? (page-number "0" 20)))
  (expect (nil? (page-number "21" 20)))
  (expect (nil? (page-number "two" 20))))

(test "outline headings are indented and show their pages"
  (expect (= (outline-labels [{title: "Intro" page: 0 depth: 0} {title: "Data" page: 4 depth: 1}])
             ["Intro · p. 1" "  Data · p. 5"])))

(test "zooming steps from the scale shown and stays in bounds"
  (expect (= (zoomed 100 1.25) 1.25))
  (expect (= (zoomed 125 0.8) 1.0))
  (expect (= (zoomed 700 2) 8.0))
  (expect (= (zoomed 30 0.5) 0.25)))

(test "only web and mail links leave oxmacs"
  (expect (safe-link? "https://example.org"))
  (expect (safe-link? "MAILTO:someone@example.org"))
  (expect (not (safe-link? "file:///etc/passwd"))))
