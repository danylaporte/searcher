#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum Presence {
    #[default]
    Optional,

    Required,
    Denied,
}
