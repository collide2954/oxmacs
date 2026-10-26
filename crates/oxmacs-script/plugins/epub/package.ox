(module
  (import (ox string) (ox xml))
  (export rootfile folder-of resolve fragment package toc-from-nav toc-from-ncx tidy))

(def hex "0123456789abcdef")
(def printable " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{{|}}~")

(def (decoded text)
  "text with %20-style escapes of ASCII characters read back."
  (loop [rest text
         out ""]
    (let [at (string/index-of rest "%")]
      (if (or (nil? at) (> (+ at 3) (count rest)))
        (str out rest)
        (let [high (string/index-of hex (string/lower (string/slice rest (+ at 1) (+ at 2))))
              low (string/index-of hex (string/lower (string/slice rest (+ at 2) (+ at 3))))
              code (when (and high low) (+ (* high 16) low))]
          (if (and code (>= code 32) (< code 127))
            (again (string/slice rest (+ at 3) (count rest)) (str out (string/slice rest 0 at) (string/slice printable (- code 32) (- code 31))))
            (again (string/slice rest (+ at 1) (count rest)) (str out (string/slice rest 0 (inc at))))))))))

(def (tidy text)
  "text with its whitespace run together into single spaces and trimmed."
  (string/join " " (string/words (or text ""))))

(def (folder-of entry)
  "The folder of a zip entry, such as \"OEBPS\" for \"OEBPS/content.opf\", or \"\" at the top."
  (string/join "/" (pop (string/split entry "/"))))

(def (resolve base href)
  "The zip entry href points to from the folder base, with . and .. followed, escapes read and any #fragment dropped."
  (let [place (first (string/split href "#"))
        joined (if (string/blank? base) place (str base "/" place))]
    (string/join "/" (reduce (fn [out part]
                               (cond
                                 [(or (= part ".") (= part "")) out]
                                 [(= part "..") (if (empty? out) out (pop out))]
                                 [else (conj out part)]))
                             []
                             (string/split (decoded joined) "/")))))

(def (fragment href)
  "The part of href after #, or nil."
  (let [parts (string/split href "#" 2)]
    (when (> (count parts) 1) (second parts))))

(def (rootfile text)
  "The package document META-INF/container.xml names."
  (xml/attr (xml/find (xml/parse text :html true) "rootfiles/rootfile") "full-path"))

(def (properties item)
  (string/words (or (xml/attr item "properties") "")))

(def (package text entry)
  "What the package document at entry says: :title, :creator and :language, the entries of the :chapters in reading order, and the entries of the :cover image, the :nav document and the :ncx table of contents, or nil."
  (let [opf (xml/parse text :html true)
        base (folder-of entry)
        metadata (xml/find opf "metadata")
        named (fn [tag] (when-let [found (xml/find metadata tag)] (tidy (xml/text found))))
        items (xml/find-all opf "manifest/item")
        by-id (reduce (fn [found item] (assoc found (xml/attr item "id") item)) {} items)
        href-of (fn [item] (when item (resolve base (xml/attr item "href"))))
        spine (xml/find opf "spine")
        cover-meta (find (fn [meta] (= (xml/attr meta "name") "cover")) (xml/find-all metadata "meta"))
        cover (or (find (fn [item] (some (fn [word] (= word "cover-image")) (properties item))) items)
                  (when cover-meta (get by-id (xml/attr cover-meta "content"))))]
    {title: (named "title")
     creator: (named "creator")
     language: (named "language")
     chapters: (vec (keep (fn [reference] (href-of (get by-id (xml/attr reference "idref"))))
                          (xml/find-all opf "spine/itemref")))
     cover: (href-of cover)
     nav: (href-of (find (fn [item] (some (fn [word] (= word "nav")) (properties item))) items))
     ncx: (when spine (href-of (get by-id (xml/attr spine "toc"))))}))

(def (named-below element tag)
  (if (string? element)
    []
    (concat (if (= element.tag tag) [element] [])
            (mapcat (fn [child] (named-below child tag)) element.children))))

(def (listed ol base depth)
  (mapcat (fn [li]
            (let [label (or (xml/find li "a") (xml/find li "span"))
                  href (xml/attr label "href")
                  nested (xml/find li "ol")
                  here (when href
                         [{title: (tidy (xml/text label)) entry: (resolve base href) fragment: (fragment href) depth: depth}])]
              (concat (or here []) (if nested (listed nested base (inc depth)) []))))
          (xml/find-all ol "li")))

(def (toc-from-nav text entry)
  "The table of contents of the EPUB 3 navigation document at entry, as maps of :title, :entry, :fragment and :depth."
  (let [navs (named-below (xml/parse text :html true) "nav")
        toc (or (find (fn [nav] (= (xml/attr nav "type") "toc")) navs) (first navs))]
    (if toc (vec (listed (xml/find toc "ol") (folder-of entry) 0)) [])))

(def (points parent base depth)
  (mapcat (fn [point]
            (let [source (xml/attr (xml/find point "content") "src")]
              (concat [{title: (tidy (xml/text (xml/find point "navLabel/text")))
                        entry: (resolve base source)
                        fragment: (fragment source)
                        depth: depth}]
                      (points point base (inc depth)))))
          (xml/find-all parent "navPoint")))

(def (toc-from-ncx text entry)
  "The table of contents of the EPUB 2 NCX document at entry, in the same maps toc-from-nav gives."
  (vec (points (xml/find (xml/parse text :html true) "navMap") (folder-of entry) 0)))

(test "entries resolve against their folder"
  (expect (= (resolve "OEBPS/text" "../images/a%20b.jpg#x") "OEBPS/images/a b.jpg"))
  (expect (= (resolve "" "./chapter.xhtml") "chapter.xhtml"))
  (expect (= (fragment "c.xhtml#part-2") "part-2"))
  (expect (= (fragment "c.xhtml") nil))
  (expect (= (folder-of "OEBPS/content.opf") "OEBPS"))
  (expect (= (folder-of "content.opf") ""))
  (expect (= (tidy "  A \n  Tale ") "A Tale")))

(test "the container names the package"
  (expect (= (rootfile "<?xml version=\"1.0\"?><container xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" version=\"1.0\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>")
             "OEBPS/content.opf")))

(def opf3 "<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>The   Ox</dc:title><dc:creator>Ada</dc:creator><dc:language>en</dc:language></metadata><manifest><item id=\"nav\" href=\"nav.xhtml\" properties=\"nav\" media-type=\"application/xhtml+xml\"/><item id=\"c1\" href=\"text/one.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"c2\" href=\"text/two.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"pic\" href=\"images/cover.png\" properties=\"cover-image\" media-type=\"image/png\"/></manifest><spine><itemref idref=\"c1\"/><itemref idref=\"c2\"/></spine></package>")

(test "EPUB 3 packages give their metadata, chapters, cover and navigation"
  (expect (= (package opf3 "OEBPS/content.opf")
             {title: "The Ox" creator: "Ada" language: "en"
              chapters: ["OEBPS/text/one.xhtml" "OEBPS/text/two.xhtml"]
              cover: "OEBPS/images/cover.png" nav: "OEBPS/nav.xhtml" ncx: nil})))

(test "EPUB 2 packages name their cover in a meta and their NCX in the spine"
  (let [found (package "<package><metadata><meta name=\"cover\" content=\"pic\"/></metadata><manifest><item id=\"pic\" href=\"cover.jpg\"/><item id=\"ncx\" href=\"toc.ncx\"/><item id=\"c\" href=\"c.html\"/></manifest><spine toc=\"ncx\"><itemref idref=\"c\"/></spine></package>" "content.opf")]
    (expect (= found.cover "cover.jpg"))
    (expect (= found.ncx "toc.ncx"))
    (expect (= found.chapters ["c.html"]))
    (expect (= found.title nil))))

(test "navigation documents list nested contents"
  (expect (= (toc-from-nav "<html xmlns:epub=\"http://www.idpf.org/2007/ops\"><body><nav epub:type=\"landmarks\"><ol><li><a href=\"x.xhtml\">X</a></li></ol></nav><nav epub:type=\"toc\"><ol><li><a href=\"text/one.xhtml\">One</a><ol><li><a href=\"text/one.xhtml#s2\">  Two\n</a></li></ol></li><li><span>Part</span></li></ol></nav></body></html>" "OEBPS/nav.xhtml")
             [{title: "One" entry: "OEBPS/text/one.xhtml" fragment: nil depth: 0}
              {title: "Two" entry: "OEBPS/text/one.xhtml" fragment: "s2" depth: 1}])))

(test "NCX documents list nested navigation points"
  (expect (= (toc-from-ncx "<ncx><navMap><navPoint><navLabel><text>One</text></navLabel><content src=\"one.html\"/><navPoint><navLabel><text>Inner</text></navLabel><content src=\"one.html#i\"/></navPoint></navPoint></navMap></ncx>" "toc.ncx")
             [{title: "One" entry: "one.html" fragment: nil depth: 0}
              {title: "Inner" entry: "one.html" fragment: "i" depth: 1}])))
