;; str joins what its arguments display as
(str "a" 1 :k nil 2.5 \c) ;=> "a1:k2.5c"
(str) ;=> ""
(def name "Ana") $"Hi {name}!" ;=> "Hi Ana!"
$"{(+ 1 2)} items" ;=> "3 items"
(let [a 1 b 2] $"{a}+{b}={(+ a b)}") ;=> "1+2=3"

;; strings count and index by character
(count "héllo") ;=> 5
(count "") ;=> 0
(count nil) ;=> 0
(def (f & xs) (count xs)) (f 1 2 3) ;=> 3
(nth "héllo" 1) ;=> \é
(nth "日本語" 2) ;=> \語
(nth "abc" 5) ;! Index 5 is out of range for a string of 3 characters
(count 5) ;! count expects a collection, got 5
