//! The style table of the tree being generated: a generator hands a node
//! the id of a hostile style and the entry waits here for the frame, or
//! for the patch frame after it, to carry.
use super::*;
use std::cell::RefCell;

struct Table {
    next: u32,
    waiting: Vec<Style>,
}

thread_local! {
    static TABLE: RefCell<Table> = const {
        RefCell::new(Table {
            next: 0,
            waiting: Vec::new(),
        })
    };
}

/// Starts a tree's table over: its first entry is the plain style, which
/// [`PLAIN`] names.
pub(super) fn start_table() {
    TABLE.with_borrow_mut(|table| {
        *table = Table {
            next: 1,
            waiting: plain(),
        }
    });
}

/// A hostile style as a new entry of the table, by the id a node names it
/// with: one draw of [`gen_native_style`].
pub(super) fn gen_style(rng: &mut Rng) -> StyleId {
    let style = gen_native_style(rng);
    TABLE.with_borrow_mut(|table| {
        table.waiting.push(Style::new(&style));
        table.next += 1;
        StyleId(table.next - 1)
    })
}

/// The entries generated since the last take: what the next frame carries.
pub(super) fn take_styles() -> Vec<Style> {
    TABLE.with_borrow_mut(|table| std::mem::take(&mut table.waiting))
}

/// `sanitize`, and the table it left the host holding.
pub(super) fn sanitized(frame: &mut Frame) -> Result<Styles, Refused> {
    let mut styles = Styles::default();
    view_wire::sanitize(frame, &mut styles)?;
    Ok(styles)
}
