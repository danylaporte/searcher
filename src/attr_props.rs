use crate::Direction;

#[derive(Clone, Default)]
pub struct AttrProps {
    pub(crate) culture: Option<u8>,
    pub(crate) direction: Direction,
    pub(crate) priority: u8,
}

impl AttrProps {
    pub fn culture(mut self, culture: u8) -> Self {
        self.culture = Some(culture);
        self
    }

    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}
