use std::time::Duration;

const LONGEST_FRAME: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
pub struct Clock {
    step: Duration,
    accumulated: Duration,
    ticks: u64,
    scale: f64,
    paused: bool,
    queued: u32,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new(60)
    }
}

impl Clock {
    pub fn new(rate: u32) -> Self {
        Self {
            step: step_for(rate),
            accumulated: Duration::ZERO,
            ticks: 0,
            scale: 1.0,
            paused: false,
            queued: 0,
        }
    }

    pub fn step(&self) -> Duration {
        self.step
    }

    pub fn set_rate(&mut self, rate: u32) {
        self.step = step_for(rate);
        self.accumulated = self.accumulated.min(self.step);
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn set_ticks(&mut self, ticks: u64) {
        self.ticks = ticks;
        self.accumulated = Duration::ZERO;
    }

    pub fn seconds(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "games run far fewer than 2^52 nanoseconds"
        )]
        let nanoseconds = (self.step.as_nanos() * u128::from(self.ticks)) as f64;
        nanoseconds / 1e9
    }

    pub fn alpha(&self) -> f64 {
        self.accumulated.as_secs_f64() / self.step.as_secs_f64()
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = if scale.is_finite() {
            scale.max(0.0)
        } else {
            1.0
        };
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.queued = 0;
    }

    pub fn step_once(&mut self) {
        self.queued = self.queued.saturating_add(1);
    }

    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        let ran = if self.paused {
            std::mem::take(&mut self.queued)
        } else {
            self.accumulated += elapsed.min(LONGEST_FRAME).mul_f64(self.scale);
            let mut ran = 0;
            while self.accumulated >= self.step {
                self.accumulated -= self.step;
                ran += 1;
            }
            ran
        };
        self.ticks += u64::from(ran);
        ran
    }
}

fn step_for(rate: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(rate.max(1)))
}
