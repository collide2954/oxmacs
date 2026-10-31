(module
  (export library))

(def library
  """
  const TAU: f32 = 6.28318530718;
  const PI: f32 = 3.14159265359;

  fn hash12(p: vec2f) -> f32 {
      var q = fract(p * vec2f(123.34, 456.21));
      q += dot(q, q + 45.32);
      return fract(q.x * q.y);
  }

  fn hash22(p: vec2f) -> vec2f {
      let n = hash12(p);
      return vec2f(n, hash12(p + n * 17.17));
  }

  fn hash13(p: vec3f) -> f32 {
      var q = fract(p * 0.1031);
      q += dot(q, q.zyx + 31.32);
      return fract((q.x + q.y) * q.z);
  }

  fn hash33(p: vec3f) -> vec3f {
      var q = fract(p * vec3f(0.1031, 0.1030, 0.0973));
      q += dot(q, q.yxz + 33.33);
      return fract((q.xxy + q.yxx) * q.zyx);
  }

  fn pcg(v: u32) -> u32 {
      let state = v * 747796405u + 2891336453u;
      let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
      return (word >> 22u) ^ word;
  }

  fn cell2(i: vec2f) -> f32 {
      let c = bitcast<vec2u>(vec2i(i));
      return f32(pcg(c.x + pcg(c.y)) >> 8u) / 16777215.0;
  }

  fn cell3(i: vec3f) -> f32 {
      let c = bitcast<vec3u>(vec3i(i));
      return f32(pcg(c.x + pcg(c.y + pcg(c.z))) >> 8u) / 16777215.0;
  }

  fn cell22(i: vec2f) -> vec2f {
      return vec2f(cell2(i), cell2(i + vec2f(7919.0, 104729.0)));
  }

  fn noise2(p: vec2f) -> f32 {
      let i = floor(p);
      let f = fract(p);
      let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
      let a = cell2(i);
      let b = cell2(i + vec2f(1.0, 0.0));
      let c = cell2(i + vec2f(0.0, 1.0));
      let d = cell2(i + vec2f(1.0, 1.0));
      return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
  }

  fn noise3(p: vec3f) -> f32 {
      let i = floor(p);
      let f = fract(p);
      let u = f * f * (3.0 - 2.0 * f);
      let n000 = cell3(i);
      let n100 = cell3(i + vec3f(1.0, 0.0, 0.0));
      let n010 = cell3(i + vec3f(0.0, 1.0, 0.0));
      let n110 = cell3(i + vec3f(1.0, 1.0, 0.0));
      let n001 = cell3(i + vec3f(0.0, 0.0, 1.0));
      let n101 = cell3(i + vec3f(1.0, 0.0, 1.0));
      let n011 = cell3(i + vec3f(0.0, 1.0, 1.0));
      let n111 = cell3(i + vec3f(1.0, 1.0, 1.0));
      return mix(
          mix(mix(n000, n100, u.x), mix(n010, n110, u.x), u.y),
          mix(mix(n001, n101, u.x), mix(n011, n111, u.x), u.y),
          u.z);
  }

  fn rotation(angle: f32) -> mat2x2f {
      let c = cos(angle);
      let s = sin(angle);
      return mat2x2f(c, s, -s, c);
  }

  fn fbm2(start: vec2f, octaves: i32) -> f32 {
      var p = start;
      var sum = 0.0;
      var amplitude = 0.5;
      let turn = rotation(0.62);
      for (var i = 0; i < octaves; i++) {
          sum += amplitude * noise2(p);
          p = turn * p * 2.03 + vec2f(1.7, 9.2);
          amplitude *= 0.5;
      }
      return sum;
  }

  fn fbm3(start: vec3f, octaves: i32) -> f32 {
      var p = start;
      var sum = 0.0;
      var amplitude = 0.5;
      for (var i = 0; i < octaves; i++) {
          sum += amplitude * noise3(p);
          p = p * 2.02 + vec3f(1.7, 9.2, 4.1);
          amplitude *= 0.5;
      }
      return sum;
  }

  fn palette(t: f32, a: vec3f, b: vec3f, c: vec3f, d: vec3f) -> vec3f {
      return a + b * cos(TAU * (c * t + d));
  }

  fn filmic(color: vec3f) -> vec3f {
      let x = max(color, vec3f(0.0));
      return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
  }

  fn shown(linear: vec3f, at: vec2f) -> vec4f {
      let display = pow(filmic(linear), vec3f(1.0 / 2.2));
      let grain = (hash12(at + fract(frame.time) * 61.0) - 0.5) / 255.0;
      return vec4f(display + grain, 1.0);
  }
  """)
