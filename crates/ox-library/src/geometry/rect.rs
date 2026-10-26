use super::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[must_use]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn at(position: Vec2, size: Vec2) -> Self {
        Self::new(position.x, position.y, size.x, size.y)
    }

    pub fn centered(center: Vec2, size: Vec2) -> Self {
        Self::at(center - size / 2.0, size)
    }

    pub fn left(self) -> f64 {
        self.x
    }

    pub fn top(self) -> f64 {
        self.y
    }

    pub fn right(self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(self) -> f64 {
        self.y + self.h
    }

    pub fn position(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    pub fn size(self) -> Vec2 {
        Vec2::new(self.w, self.h)
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(self, point: Vec2) -> bool {
        (self.left()..self.right()).contains(&point.x)
            && (self.top()..self.bottom()).contains(&point.y)
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.overlaps(other) {
            return None;
        }
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Some(Self::new(left, top, right - left, bottom - top))
    }

    pub fn union(self, other: Self) -> Self {
        let left = self.left().min(other.left());
        let top = self.top().min(other.top());
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(left, top, right - left, bottom - top)
    }

    pub fn translated(self, by: Vec2) -> Self {
        Self::at(self.position() + by, self.size())
    }

    pub fn expanded(self, by: f64) -> Self {
        Self::new(
            self.x - by,
            self.y - by,
            by.mul_add(2.0, self.w),
            by.mul_add(2.0, self.h),
        )
    }

    pub fn clamp(self, point: Vec2) -> Vec2 {
        Vec2::new(
            point.x.max(self.left()).min(self.right()),
            point.y.max(self.top()).min(self.bottom()),
        )
    }
}
