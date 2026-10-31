(module
  (import (ox math) (screensaver noise :as noise))
  (export background branches draw))

(def background "#0d0b12")
(def cycle 24.0)
(def deepest 9)

(def (branches season x y angle length depth grown seconds path)
  "The lines of a branch and everything growing from it, as far as grown lets them."
  (let [shown (max 0 (min 1 (- (* grown (inc deepest)) depth)))]
    (if (or (> depth deepest) (<= shown 0))
      []
      (let [sway (* 0.035 depth (math/sin (+ (* 1.3 seconds) (* 0.7 depth) (noise/of [season path :sway]))))
            heading (+ angle sway)
            reach (* length shown)
            tx (+ x (* reach (math/cos heading)))
            ty (+ y (* reach (math/sin heading)))
            spread (+ 0.32 (* 0.3 (noise/of [season path :spread])))
            shrink (+ 0.66 (* 0.12 (noise/of [season path :shrink])))
            width (max 0.6 (* 0.9 (- deepest depth)))
            line [:line x y tx ty [(if (< depth 6) "#6b4f3a" "#8a6b4a") 0.95] width]]
        (if (< shown 1)
          [line]
          (vec (concat [line]
                       (branches season tx ty (- heading spread) (* length shrink) (inc depth) grown seconds (* 2 path))
                       (branches season tx ty (+ heading spread) (* length shrink 0.94) (inc depth) grown seconds (inc (* 2 path)))
                       (if (= depth deepest) [{leaf: [tx ty] path: path}] []))))))))

(def (leaf at path season along height)
  (let [[x y] at
        falls (+ 11 (* 5 (noise/of [season path :falls])))
        fallen (max 0 (- along falls))
        color (nth ["#ff8fab" "#ffb3c6" "#fb6f92" "#c9f2c7"] (mod path 4))
        fade (max 0 (- 1 (/ (max 0 (- along 21)) 3)))]
    [:circle (+ x (* 30 fallen (math/sin (+ path fallen))) (* 8 fallen))
     (min (- height 6) (+ y (* 40 fallen fallen)))
     3.2 [color (* 0.85 fade)]]))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        season (int (/ seconds cycle))
        along (- seconds (* season cycle))
        grown (min 1 (/ along 5.0))
        parts (branches season (/ frame.width 2.0) frame.height (- (/ math/pi 2)) (* 0.24 frame.height) 0 grown seconds 1)
        lines (filter vector? parts)
        leaves (filter map? parts)]
    (vec (concat lines
                 (if (>= grown 1)
                   (map (fn [l] (leaf l.leaf l.path season along frame.height)) leaves)
                   [])))))

(test "the tree grows from its trunk to its tips"
  (let [young (branches 0 400 600 -1.57 100 0 0.2 0 1)
        grown (branches 0 400 600 -1.57 100 0 1 0 1)]
    (expect (< (count young) (count grown)))
    (expect (= (count (filter map? grown)) 512))))
