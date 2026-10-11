//! What an entity tells the entities that hear it: `cx.notify()` its
//! observers, `cx.emit(event)` its subscribers. Raised during an update and
//! delivered once the outermost update is done, when no entity is borrowed,
//! so a parent that hears its child may update the child back.
use super::App;
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

/// `None` is a notify, `Some` an event of that type.
pub(crate) type Kind = Option<TypeId>;

type Hear = Rc<RefCell<dyn FnMut(&dyn Any, &mut App)>>;

/// A notify or an event, as raised: who raised it, its kind, its payload.
type Raised = (u64, Kind, Box<dyn Any>);

struct Hearer {
    id: u64,
    emitter: u64,
    kind: Kind,
    hear: Hear,
}

#[derive(Default)]
pub(crate) struct Events {
    next: Cell<u64>,
    hearers: RefCell<Vec<Hearer>>,
    raised: RefCell<VecDeque<Raised>>,
    /// How many updates are running, nested: the outermost delivers.
    depth: Cell<usize>,
}

impl Events {
    /// `hear` runs on each `kind` the entity `emitter` raises, until
    /// [`forget`](Self::forget) is called with the id this returns.
    pub(crate) fn listen(
        &self,
        emitter: u64,
        kind: Kind,
        hear: impl FnMut(&dyn Any, &mut App) + 'static,
    ) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.hearers.borrow_mut().push(Hearer {
            id,
            emitter,
            kind,
            hear: Rc::new(RefCell::new(hear)),
        });
        id
    }

    pub(crate) fn forget(&self, id: u64) {
        let gone = {
            let mut hearers = self.hearers.borrow_mut();
            let at = hearers.iter().position(|hearer| hearer.id == id);
            at.map(|at| hearers.remove(at))
        };
        // dropped once the table is free: what the closure held may forget
        // another hearer as it goes
        drop(gone);
    }

    /// Queues `kind` from `emitter` for delivery, if anything hears it.
    pub(crate) fn raise(&self, emitter: u64, kind: Kind, payload: impl FnOnce() -> Box<dyn Any>) {
        let heard = self
            .hearers
            .borrow()
            .iter()
            .any(|hearer| hearer.emitter == emitter && hearer.kind == kind);
        if heard {
            self.raised
                .borrow_mut()
                .push_back((emitter, kind, payload()));
        }
    }
}

impl App {
    /// Runs `f` as one update of the entity graph: what it raises is heard
    /// when the outermost update ends.
    pub(crate) fn update<R>(&mut self, f: impl FnOnce(&mut App) -> R) -> R {
        let depth = self.inner.events.depth.get();
        self.inner.events.depth.set(depth + 1);
        let result = f(self);
        if depth == 0 {
            self.deliver();
        }
        self.inner.events.depth.set(depth);
        result
    }

    /// Hands every raised notify and event to what hears it, in order,
    /// including what the hearers raise as they run.
    fn deliver(&mut self) {
        loop {
            let Some((emitter, kind, payload)) = self.inner.events.raised.borrow_mut().pop_front()
            else {
                return;
            };
            let hearers: Vec<(u64, Hear)> = self
                .inner
                .events
                .hearers
                .borrow()
                .iter()
                .filter(|hearer| hearer.emitter == emitter && hearer.kind == kind)
                .map(|hearer| (hearer.id, hearer.hear.clone()))
                .collect();
            for (id, hear) in hearers {
                // one an earlier hearer forgot hears nothing more
                let listening = self
                    .inner
                    .events
                    .hearers
                    .borrow()
                    .iter()
                    .any(|h| h.id == id);
                if listening {
                    (hear.borrow_mut())(&*payload, self);
                }
            }
        }
    }
}
