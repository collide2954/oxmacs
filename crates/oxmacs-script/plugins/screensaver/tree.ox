(module
  (import (ox math) (screensaver noise :as noise))
  (export branches draw))

(def cycle 24.0)
(def deepest 9)
(def bark ["#2a1c14" "#33241a" "#3d2b1f" "#4a3424" "#55402c" "#5e4832" "#6a5238" "#76603f" "#7f6a45" "#8a7550"])

(def (branches season x y angle length depth grown seconds path)
  "The parts of a branch and everything growing from it, as far as grown lets them."
  (let [shown (max 0 (min 1 (- (* grown (inc deepest)) depth)))]
    (if (or (> depth deepest) (<= shown 0))
      []
      (let [sway (* 0.03 depth (math/sin (+ (* 1.1 seconds) (* 0.7 depth) (noise/of [season path :sway]))))
            heading (+ angle sway)
            reach (* length shown)
            tx (+ x (* reach (math/cos heading)))
            ty (+ y (* reach (math/sin heading)))
            spread (+ 0.30 (* 0.3 (noise/of [season path :spread])))
            shrink (+ 0.66 (* 0.12 (noise/of [season path :shrink])))
            base (* 0.55 (math/pow 0.72 depth) length 0.16)
            tip (* base 0.72)
            nx (- (math/sin heading))
            ny (math/cos heading)
            piece [:polygon [[(+ x (* nx base)) (+ y (* ny base))]
                             [(+ tx (* nx tip)) (+ ty (* ny tip))]
                             [(- tx (* nx tip)) (- ty (* ny tip))]
                             [(- x (* nx base)) (- y (* ny base))]]
                   (nth bark depth)]]
        (if (< shown 1)
          [piece]
          (vec (concat [piece]
                       (branches season tx ty (- heading spread) (* length shrink) (inc depth) grown seconds (* 2 path))
                       (branches season tx ty (+ heading spread) (* length shrink 0.94) (inc depth) grown seconds (inc (* 2 path)))
                       (if (= depth deepest) [{leaf: [tx ty] path: path}] []))))))))

(def blossoms ["#ffb3c6" "#ff8fab" "#ffc8dd" "#fb6f92" "#fff0f3"])

(def (leaf at path season along height)
  (let [[x y] at
        falls (+ 11 (* 6 (noise/of [season path :falls])))
        fallen (max 0 (- along falls))
        color (nth blossoms (mod path (count blossoms)))
        fade (max 0 (- 1 (/ (max 0 (- along 21)) 3)))
        bloom (min 1 (* 2 (- along 5.5)))
        px (+ x (* 26 fallen (math/sin (+ path (* 1.3 fallen)))) (* 10 fallen))
        py (min (- height 8) (+ y (* 30 fallen fallen)))
        size (* 3.4 bloom (+ 0.7 (* 0.6 (noise/of [path :size]))))]
    (if (<= bloom 0)
      []
      [[:glow px py (* 2.6 size) [color (* 0.3 fade)]]
       [:circle px py size [color (* 0.9 fade)]]])))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        season (int (/ seconds cycle))
        along (- seconds (* season cycle))
        grown (min 1 (/ along 5.0))
        ground (* 0.86 frame.height)
        parts (branches season (/ frame.width 2.0) ground (- (/ math/pi 2)) (* 0.22 frame.height) 0 grown seconds 1)
        pieces (filter vector? parts)
        leaves (filter map? parts)]
    (vec (concat [[:glow (/ frame.width 2.0) ground (* 0.35 frame.height) ["#000000" 0.35]]]
                 pieces
                 (if (>= grown 1)
                   (mapcat (fn [l] (leaf l.leaf l.path season along frame.height)) leaves)
                   [])))))

(test "the tree grows from its trunk to its tips"
  (let [young (branches 0 400 600 -1.57 100 0 0.2 0 1)
        grown (branches 0 400 600 -1.57 100 0 1 0 1)]
    (expect (< (count young) (count grown)))
    (expect (= (count (filter map? grown)) 512))))
