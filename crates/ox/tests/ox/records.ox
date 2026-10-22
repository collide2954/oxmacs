;; records are defined built and read
(defrecord Point [x y]) ;=> Point
(defrecord Point [x y]) Point ;=> #<record Point>
(defrecord Point [x y]) (Point 1 2) ;=> #Point{x: 1 y: 2}
(defrecord Point [x y (z 0) {label nil}]) (Point 1 2) ;=> #Point{x: 1 y: 2 z: 0 label: nil}
(defrecord Point [x y (z 0) {label nil}]) (Point 1 2 3 label: "a") ;=> #Point{x: 1 y: 2 z: 3 label: "a"}
(defrecord Point [x (y (* x 2))]) (Point 3) ;=> #Point{x: 3 y: 6}
(defrecord Point [x y]) (def p (Point 1 2)) (+ p.x p.y) ;=> 3
(defrecord Point [x y]) (:y (Point 1 2)) ;=> 2
(defrecord Point [x y]) (get (Point 1 2) :x) ;=> 1
(defrecord Point [x y]) (get (Point 1 2) :w :none) ;=> :none
(defrecord Point [x y]) (record? (Point 1 2)) ;=> true
(record? {x: 1}) ;=> false
(defrecord Point [x y]) (Point 1) ;! Point takes 2 arguments, got 1
(defrecord Point [x y]) (def p (Point 1 2)) p.z ;! Point has no field z
(defrecord Point [x & more]) ;! Records have no rest field
(defrecord Point x) ;! defrecord takes a name and a field vector

;; records change by copying
(defrecord Point [x y]) (with (Point 1 2) :x 10) ;=> #Point{x: 10 y: 2}
(defrecord Point [x y]) (with (Point 1 2) :x 10 :y 20) ;=> #Point{x: 10 y: 20}
(defrecord Point [x y]) (assoc (Point 1 2) :y 5) ;=> #Point{x: 1 y: 5}
(defrecord Point [x y]) (def p (Point 1 2)) (with p :x 10) p ;=> #Point{x: 1 y: 2}
(defrecord Point [x y]) (with (Point 1 2) :color :red) ;! Point has no field color
(defrecord Point [x y]) (assoc (Point 1 2) :color :red) ;! Point has no field color
(defrecord Point [x y]) (with (Point 1 2) :x) ;! with takes a record and field value pairs
(with {a: 1} :a 2) ;! with expects a record, got {a: 1}

;; record literals read back what records print
(defrecord Point [x y]) #Point{x: 1 y: 2} ;=> #Point{x: 1 y: 2}
(defrecord Point [x y]) (= #Point{y: 2 x: 1} (Point 1 2)) ;=> true
(defrecord Point [x y]) (let [a 5] #Point{x: a y: (* a 2)}) ;=> #Point{x: 5 y: 10}
(defrecord Point [x y]) #Point{x: 1} ;! This Point literal is missing field y
(defrecord Point [x y]) #Point{x: 1 y: 2 z: 3} ;! Point has no field z
(defrecord Point [x y]) #Point{x: 1 x: 2 y: 3} ;! Duplicate field x in a Point literal
#Nothing{} ;! Nothing is not defined
(def Nothing 5) #Nothing{} ;! 5 is not a record type

;; records are equal by type and fields
(defrecord Point [x y]) (= (Point 1 2) (Point 1 2)) ;=> true
(defrecord Point [x y]) (= (Point 1 2) (Point 1 3)) ;=> false
(defrecord Point [x y]) (= (Point 1 2) {x: 1 y: 2}) ;=> false
(defrecord Point [x y]) (defrecord Pair [x y]) (= (Point 1 2) (Pair 1 2)) ;=> false
(defrecord Point [x y]) (= (Point [1] 2) (Point (list 1) 2)) ;=> true
(defrecord Point [x y]) (= (hash (Point 1 2)) (hash (Point 1 2))) ;=> true
(defrecord Point [x y]) (= (hash (Point 1 2)) (hash [1 2])) ;=> false
(defrecord Point [x y]) (get {(Point 1 2) :found} (Point 1 2)) ;=> :found

;; fields are read with dots
(def city {name: "Lyon" size: 3}) city.name ;=> "Lyon"
(def city {name: "Lyon"}) city.missing ;=> nil
(def a {b: {c: {d: 4}}}) a.b.c.d ;=> 4
(def a {b: nil}) a.b.c.d ;=> nil
(def a nil) a.b ;=> nil
(def city {name: "Lyon"}) "Hi {city.name}!" ;=> "Hi Lyon!"
(let [p {x: 1 y: 2}] (+ p.x p.y)) ;=> 3
(def (f m) m.k) (f {k: :v}) ;=> :v
(def n 5) n.x ;! 5 has no field x
(def s "abc") s.x ;! "abc" has no field x
(. {a: 1}) ;! . takes a value and field names
(. {a: 1} "a") ;! Field names must be names

;; field sites follow a change of record type
(defrecord A [x y]) (defrecord B [y x]) (def (gety r) r.y) [(gety (A 1 2)) (gety (B 3 4)) (gety (A 5 6)) (gety {y: 7})] ;=> [2 3 6 7]
