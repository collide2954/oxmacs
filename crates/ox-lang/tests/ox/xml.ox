;; ox xml reads elements into maps of tag, attrs and children
(module (import (ox xml))) (xml/parse "<a href=\"x\">hi <b>there</b></a>") ;=> {tag: "a" attrs: {href: "x"} children: ["hi " {tag: "b" attrs: {} children: ["there"]}]}
(module (import (ox xml))) (xml/parse "<list>\n  <item/>\n  <!-- note -->\n  <item>&amp;&lt;<![CDATA[<raw>]]></item>\n</list>") ;=> {tag: "list" attrs: {} children: [{tag: "item" attrs: {} children: []} {tag: "item" attrs: {} children: ["&<<raw>"]}]}
(module (import (ox xml))) (xml/parse "<?xml version=\"1.0\"?><feed xmlns=\"http://www.w3.org/2005/Atom\" xmlns:m=\"urn:m\"><m:thumb m:url=\"u\"/></feed>") ;=> {tag: "feed" attrs: {} children: [{tag: "thumb" attrs: {url: "u"} children: []}]}
(module (import (ox xml))) (xml/parse "<a><b></a>") ;! xml/parse can't read this: expected 'b' tag, not 'a' at 1:7
(module (import (ox xml))) (xml/parse 5) ;! xml/parse expects a string, got 5
(module (import (ox xml))) (xml/parse "<p><i>a</i> <b>b</b>&nbsp;&mdash;&eacute;&nope;</p>" :html true) ;=> {tag: "p" attrs: {} children: [{tag: "i" attrs: {} children: ["a"]} " " {tag: "b" attrs: {} children: ["b"]} " —é�"]}
(module (import (ox xml))) (xml/parse "<p>a&amp;b</p>" :html false) ;=> {tag: "p" attrs: {} children: ["a&b"]}
(module (import (ox xml))) (xml/parse "<p/>" :strict true) ;! xml/parse has no keyword parameter :strict

;; finds elements by a path of child tag names
(module (import (ox xml))) (def rss (xml/parse "<rss><channel><title>News</title><item><title>One</title></item><item><title>Two</title><link>l2</link></item></channel></rss>")) (xml/text (xml/find rss "channel/title")) ;=> "News"
(module (import (ox xml))) (def rss (xml/parse "<rss><channel><item><title>One</title></item><item><title>Two</title></item></channel></rss>")) (map xml/text (xml/find-all rss "channel/item/title")) ;=> ["One" "Two"]
(module (import (ox xml))) (def rss (xml/parse "<rss><channel><item><title>One</title></item></channel></rss>")) [(xml/find rss "channel/missing") (xml/text (xml/find rss "channel/item/missing")) (xml/find-all rss "nothing")] ;=> [nil nil []]
(module (import (ox xml))) (xml/text (xml/parse "<p>Hello <b>big</b> world</p>")) ;=> "Hello big world"
(module (import (ox xml))) (xml/text "plain") ;=> "plain"
(module (import (ox xml))) (def link (xml/parse "<link href=\"https://x\" rel=\"alternate\"/>")) [(xml/attr link "href") (xml/attr link :rel) (xml/attr link "type")] ;=> ["https://x" "alternate" nil]
(module (import (ox xml))) (xml/find 5 "a") ;! xml/find expects an element, got 5
(module (import (ox xml))) (xml/find (xml/parse "<a/>") 5) ;! xml/find expects a path such as "channel/item", got 5

;; and writes elements back as XML
(module (import (ox xml))) (xml/write {tag: "a" attrs: {href: "x&y" title: "say \"hi\""} children: ["1 < 2 & 3 > 2 " {tag: "b" attrs: {} children: []}]}) ;=> "<a href=\"x&amp;y\" title=\"say &quot;hi&quot;\">1 &lt; 2 &amp; 3 &gt; 2 <b/></a>"
(module (import (ox xml))) (xml/write {tag: :p children: ["hi"]}) ;=> "<p>hi</p>"
(module (import (ox xml))) (xml/write (xml/parse "<a x=\"1\"><b>t</b></a>")) ;=> "<a x=\"1\"><b>t</b></a>"
(module (import (ox xml))) (xml/write {tag: "" children: []}) ;! xml/write needs a tag name such as "item", got ""
(module (import (ox xml))) (xml/write [{tag: "a"} 5]) ;! xml/write can't write [{tag: "a"} 5]
(module (import (ox xml))) (xml/write {tag: "n" children: [3 2.5 true]}) ;=> "<n>32.5true</n>"
