use super::{App, MouseButton, Window};

pub(crate) type EventListener<E> = Box<dyn Fn(&E, &mut Window, &mut App) + 'static>;

/// A mouse-button listener: for `button`, or for any button.
pub(super) struct ButtonBinding<E> {
    pub(super) button: Option<MouseButton>,
    pub(super) listener: EventListener<E>,
}
