(module
  (import (ox string) (screensaver noise :as noise))
  (export drop draw))

(def glyphs (vec (map str (string/chars "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ=+*<>:;|#$%&@"))))

(def scramble
  (vec (map (fn [i] (nth glyphs (int (* (count glyphs) (noise/of [i :glyph])))))
            (range 997))))

(def columns
  (vec (map (fn [column]
              {length: (+ 6 (int (* 12 (noise/of [column :length]))))
               gap: (noise/of [column :gap])
               speed: (+ 6 (* 14 (noise/of [column :speed])))
               start: (noise/of [column :start])
               flicker: (+ 0.08 (* 0.3 (noise/of [column :flicker])))})
            (range 400))))

(def (drop column seconds rows)
  (let [c (nth columns (mod column (count columns)))
        cycle (+ rows c.length (int (* rows c.gap)))]
    {head: (int (mod (+ (* cycle c.start) (* seconds c.speed)) cycle))
     length: c.length
     flicker: (int (/ seconds c.flicker))}))

(def (glyph column row flicker)
  (nth scramble (mod (+ (* column 31) (* row 17) (* flicker 7)) 997)))

(def (fall column seconds rows width height size)
  (let [falling (drop column seconds rows)
        x (* column width)]
    (keep (fn [k]
            (let [row (- falling.head k)]
              (when (and (>= row 0) (< row rows))
                [:text x (* row height)
                 (glyph column row falling.flicker)
                 (if (= k 0) :foreground [:success (- 1 (/ k falling.length))])
                 size])))
          (range falling.length))))

(def (draw frame)
  (let [size frame.font-size
        width size
        height (* size 1.2)
        rows (+ 1 (int (/ frame.height height)))
        seconds (/ frame.time 1000.0)]
    (mapcat (fn [column] (fall column seconds rows width height size))
            (range (int (/ frame.width width))))))