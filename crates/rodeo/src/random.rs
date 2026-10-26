#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Random {
    state: [u64; 4],
}

impl Random {
    pub fn new(seed: u64) -> Self {
        let mut mixer = seed;
        Self {
            state: [(); 4].map(|()| splitmix64(&mut mixer)),
        }
    }

    pub fn from_state(state: [u64; 4]) -> Self {
        if state == [0; 4] {
            return Self::new(0);
        }
        Self { state }
    }

    pub fn state(&self) -> [u64; 4] {
        self.state
    }

    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= shifted;
        *s3 = s3.rotate_left(45);
        result
    }

    pub fn float(&mut self) -> f64 {
        f64::from_bits((1023 << 52) | (self.next_u64() >> 12)) - 1.0
    }

    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        (high - low).mul_add(self.float(), low)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let limit = u64::MAX - u64::MAX % bound;
        loop {
            let drawn = self.next_u64();
            if drawn < limit {
                return drawn % bound;
            }
        }
    }

    pub fn int(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let span = u64::try_from(i128::from(high) - i128::from(low)).unwrap_or(u64::MAX);
        let drawn = i128::from(low) + i128::from(self.below(span));
        i64::try_from(drawn).unwrap_or(low)
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        self.float() < probability
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let count = u64::try_from(items.len()).ok()?;
        items.get(usize::try_from(self.below(count)).ok()?)
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for last in (1..items.len()).rev() {
            let bound = u64::try_from(last + 1).unwrap_or(u64::MAX);
            let other = usize::try_from(self.below(bound)).unwrap_or(last);
            items.swap(last, other);
        }
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = *state;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}
