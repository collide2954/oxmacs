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
          (screensaver mandelbrot :as mandelbrot)
          (screensaver backdrops :as backdrops))
  (export savers notes start))

(def savers
  [{kind: :starfield note: "Stars rushing toward you" draw: stars/draw}
   {kind: :matrix note: "Glyphs raining down the screen" draw: rain/draw fps: 15}
   {kind: :pond note: "Rain falling on a lily pond at dusk, its rings spreading and crossing" shader: pond/shader}
   {kind: :fireworks note: "Fireworks bursting over a harbor, lighting the smoke and the water" draw: fireworks/draw shader: fireworks/shader}
   {kind: :harmonograph note: "A pen on swinging pendulums inking a curve that dies away" draw: harmonograph/draw shader: backdrops/paper}
   {kind: :aurora note: "Northern lights folding over mountains and a still lake" shader: aurora/shader}
   {kind: :clock note: "The time and date drifting slowly over soft light" draw: clock/draw shader: clock/shader}
   {kind: :flow note: "Light swept along slowly turning currents" draw: flow/draw shader: backdrops/deep fps: 20}
   {kind: :orbits note: "Planets and moons going round a glowing star" draw: orbits/draw shader: backdrops/space}
   {kind: :snowfall note: "Snow falling on a quiet forest and a lit cabin at dusk" shader: snowfall/shader}
   {kind: :tunnel note: "Flying through a winding lit tunnel" shader: tunnel/shader}
   {kind: :lava note: "Wax rising, merging and splitting in a glowing lava lamp" shader: lava/shader}
   {kind: :constellations note: "Constellations traced one by one across the Milky Way" draw: constellations/draw shader: constellations/shader}
   {kind: :plasma note: "Glossy liquid color folding slowly into itself" shader: plasma/shader}
   {kind: :bubbles note: "Soap bubbles drifting up past a sunny garden, their films swirling with color" shader: bubbles/shader}
   {kind: :kaleidoscope note: "Colored glass turning in a six-fold mirror" shader: kaleidoscope/shader}
   {kind: :wireframes note: "Glowing wireframe solids tumbling and turning into one another" draw: wireframes/draw shader: backdrops/studio}
   {kind: :city note: "A city across the water at night, its windows lighting and going dark" shader: city/shader}
   {kind: :pendulums note: "A row of pendulums swinging in and out of step" draw: pendulums/draw shader: backdrops/deep}
   {kind: :tree note: "A tree growing at dusk, swaying, dropping its blossom and growing again" draw: tree/draw shader: backdrops/dusk fps: 20}
   {kind: :ocean note: "Open sea rolling under a low golden sun" shader: ocean/shader}
   {kind: :glass note: "Rain beading and running down a window over blurred city lights" shader: glass/shader}
   {kind: :galaxy note: "A spiral galaxy turning, its inner stars fastest" shader: galaxy/shader}
   {kind: :ants note: "Langton's ants building chaos and then highways" draw: ants/draw shader: backdrops/deep fps: 15}
   {kind: :campfire note: "A campfire in a dark forest, sparks rising into the night" shader: campfire/shader}
   {kind: :sorting note: "Bars sorting themselves by one algorithm after another" draw: sorting/draw shader: backdrops/studio fps: 20}
   {kind: :storm note: "A night thunderstorm, forked lightning lighting the clouds and rain" shader: storm/shader}
   {kind: :fireflies note: "Fireflies over a meadow at dusk, blinking into step" shader: fireflies/shader}
   {kind: :mandelbrot note: "A slow dive into the edge of the Mandelbrot set" shader: mandelbrot/shader}])

(def notes
  (reduce (fn [found saver] (assoc found saver.kind saver.note)) {} savers))

(def (start kind)
  "Takes over the windows with the screensaver called kind, or the first one."
  (let [saver (or (find (fn [saver] (= saver.kind kind)) savers) (first savers))]
    (take-over saver.draw :fps saver.fps :background saver.background :shader saver.shader)))
