(module
  (import (screensaver starfield :as stars)
          (screensaver matrix :as rain)))

(def frame {time: 4000 width: 1920 height: 1040 font-size: 18.67})

{starfield: (fn [] (stars/draw frame))
 matrix: (fn [] (rain/draw frame))}
