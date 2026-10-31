(module
  (import (screensaver starfield :as stars)
          (screensaver matrix :as rain)
          (screensaver pond :as pond)
          (screensaver fireworks :as fireworks)
          (screensaver harmonograph :as harmonograph)
          (screensaver aurora :as aurora)
          (screensaver clock :as clock)
          (screensaver flow :as flow)
          (screensaver orbits :as orbits)
          (screensaver snowfall :as snowfall)
          (screensaver tunnel :as tunnel)
          (screensaver lava :as lava)
          (screensaver constellations :as constellations)
          (screensaver plasma :as plasma)
          (screensaver bubbles :as bubbles)
          (screensaver kaleidoscope :as kaleidoscope)
          (screensaver wireframes :as wireframes)
          (screensaver city :as city)
          (screensaver pendulums :as pendulums)
          (screensaver tree :as tree)
          (screensaver ocean :as ocean)
          (screensaver glass :as glass)
          (screensaver galaxy :as galaxy)
          (screensaver ants :as ants)
          (screensaver campfire :as campfire)
          (screensaver sorting :as sorting)
          (screensaver storm :as storm)
          (screensaver fireflies :as fireflies)
          (screensaver mandelbrot :as mandelbrot))
  (export savers notes start))

(def savers
  [{kind: :starfield note: "Stars rushing toward you" draw: stars/draw}
   {kind: :matrix note: "Glyphs raining down the screen" draw: rain/draw fps: 15}
   {kind: :pond note: "Rain on a dark pond, its rings spreading and crossing" draw: pond/draw background: pond/background}
   {kind: :fireworks note: "Rockets rising and bursting into falling sparks" draw: fireworks/draw background: fireworks/background}
   {kind: :harmonograph note: "A pen on swinging pendulums drawing a curve that dies away" draw: harmonograph/draw background: harmonograph/background}
   {kind: :aurora note: "Curtains of light waving over a starry sky" draw: aurora/draw background: aurora/background}
   {kind: :clock note: "The time and date drifting slowly around the screen" draw: clock/draw fps: 2}
   {kind: :flow note: "Light swept along slowly turning currents" draw: flow/draw background: flow/background}
   {kind: :orbits note: "Planets and moons going round a glowing star" draw: orbits/draw background: orbits/background}
   {kind: :snowfall note: "Snow drifting down on a breeze and piling up" draw: snowfall/draw background: snowfall/background}
   {kind: :tunnel note: "Rings of light rushing at you through a twisting tunnel" draw: tunnel/draw background: tunnel/background}
   {kind: :lava note: "Warm blobs rising, merging and splitting in a lava lamp" draw: lava/draw background: lava/background fps: 20}
   {kind: :constellations note: "Drifting stars joined by lines whenever they come near" draw: constellations/draw background: constellations/background}
   {kind: :plasma note: "Bands of color rippling across a grid of tiles" draw: plasma/draw background: plasma/background fps: 20}
   {kind: :bubbles note: "Bubbles wobbling up from the deep and popping" draw: bubbles/draw background: bubbles/background}
   {kind: :kaleidoscope note: "Bright shapes turning in a six-fold mirror" draw: kaleidoscope/draw background: kaleidoscope/background}
   {kind: :wireframes note: "Wireframe solids tumbling and turning into one another" draw: wireframes/draw background: wireframes/background}
   {kind: :city note: "A skyline whose windows light up and go dark through the night" draw: city/draw background: city/background fps: 10}
   {kind: :pendulums note: "A row of pendulums swinging in and out of step" draw: pendulums/draw background: pendulums/background}
   {kind: :tree note: "A tree growing, swaying, dropping its blossom and growing again" draw: tree/draw background: tree/background}
   {kind: :ocean note: "Swells rolling in under a low sun" draw: ocean/draw background: ocean/background}
   {kind: :glass note: "Rain beading and running down a window over city lights" draw: glass/draw background: glass/background}
   {kind: :galaxy note: "A spiral galaxy turning, its inner stars fastest" draw: galaxy/draw background: galaxy/background}
   {kind: :ants note: "Langton's ants building chaos and then highways" draw: ants/draw background: ants/background fps: 15}
   {kind: :campfire note: "Flames flickering up from the logs, sparks drifting off" draw: campfire/draw background: campfire/background}
   {kind: :sorting note: "Bars sorting themselves by one algorithm after another" draw: sorting/draw background: sorting/background fps: 20}
   {kind: :storm note: "Rain and forked lightning lighting up the clouds" draw: storm/draw background: storm/background}
   {kind: :fireflies note: "Fireflies over the grass, blinking into step" draw: fireflies/draw background: fireflies/background}
   {kind: :mandelbrot note: "A slow dive into the edge of the Mandelbrot set" draw: mandelbrot/draw background: mandelbrot/background fps: 12}])

(def notes
  (reduce (fn [found saver] (assoc found saver.kind saver.note)) {} savers))

(def (start kind)
  "Takes over the windows with the screensaver called kind, or the first one."
  (let [saver (or (find (fn [saver] (= saver.kind kind)) savers) (first savers))]
    (take-over saver.draw :fps saver.fps :background saver.background)))
