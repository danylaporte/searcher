#[derive(Clone, Copy, Default)]
pub(crate) enum Presence {
    #[default]
    Optional,

    Required,
    Denied,
}
