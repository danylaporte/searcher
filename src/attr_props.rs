use crate::Direction;

#[derive(Clone, Default)]
pub struct AttrProps {
    pub(crate) direction: Option<Direction>,
    pub(crate) priority: Option<u8>,
}

impl AttrProps {
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = Some(direction);
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = Some(priority);
        self
    }
}
