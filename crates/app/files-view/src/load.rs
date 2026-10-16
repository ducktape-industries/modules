//! The loaders: what the page on top of the stack shows, read with
//! `cx.load`, which keeps what is on screen while it reads again and draws
//! only what changed; and who reads, the name their home is called by.
use view::prelude::*;

use crate::state::{Name, Route, Sheet};
use crate::{Files, queries};

impl Files {
    /// What the page on top shows: on a move to it, and on every commit
    /// anyone lands. A page read before shows while it is read again.
    pub(crate) fn read_page(&mut self, cx: &mut Context<Self>) {
        match self.page().clone() {
            // the places name the newest snapshots
            Route::Places | Route::History => self.read_history(cx),
            Route::Folder(path) => self.read_folder(path, cx),
            Route::File(path) => self.read_file(path, cx),
            Route::Snapshot(seq) => {
                self.read_history(cx);
                self.read_changed(seq, cx);
            }
        }
    }

    fn read_folder(&mut self, path: String, cx: &mut Context<Self>) {
        // another folder's entries are not this one's to show meanwhile
        if self.folder_at(&path).is_none() {
            self.folder = Loadable::Idle;
        }
        let read = queries::folder(cx.host(), path);
        cx.load(self, read, |view| &mut view.folder);
    }

    fn read_file(&mut self, path: String, cx: &mut Context<Self>) {
        if self.file_at(&path).is_none() {
            self.file = Loadable::Idle;
        }
        let read = queries::file(cx.host(), path);
        cx.load(self, read, |view| &mut view.file);
    }

    fn read_history(&mut self, cx: &mut Context<Self>) {
        let read = queries::history(cx.host());
        cx.load(self, read, |view| &mut view.history);
        if self.names.is_idle() {
            self.read_names(cx);
        }
    }

    /// What one snapshot changed, asked once per snapshot (a snapshot
    /// never changes) and again after a refusal. It needs the snapshot's
    /// row, which the history brings: the root asks on every draw, so a
    /// page opened before the history landed asks when it does.
    pub(crate) fn read_changed(&mut self, seq: u64, cx: &mut Context<Self>) {
        let asked = self.changed_of == Some(seq) && self.changed.failed().is_none();
        if asked {
            return;
        }
        let Some(row) = self.snapshot(seq) else {
            return;
        };
        let read = queries::changed(cx.host(), seq, row.id, row.parent);
        self.changed_of = Some(seq);
        self.changed = Loadable::Idle;
        cx.load(self, read, |view| &mut view.changed);
    }

    /// Every author's name: once, and again when identity changes.
    pub(crate) fn read_names(&mut self, cx: &mut Context<Self>) {
        let read = queries::names(cx.host());
        cx.load(self, read, |view| &mut view.names);
    }

    /// The reader's name, which names their home; asked again when the
    /// session names another account, or identity changes.
    pub(crate) fn read_reader(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.reader.account else {
            return;
        };
        let read = queries::name(cx.host(), account);
        cx.land(read, move |view, name, cx| {
            // a session moved on while this was asked
            if view.reader.account != Some(account) {
                return;
            }
            let name = match name {
                Ok(Some(name)) => Name::Named(name),
                Ok(None) => Name::Unnamed,
                Err(refusal) => {
                    cx.log_refused("the reader's name", &refusal);
                    Name::Refused(refusal.message)
                }
            };
            if name != view.reader.name {
                view.reader.name = name;
                cx.notify();
            }
        })
        .detach();
    }

    /// A session: another account reads, or the same one again.
    pub(crate) fn session_changed(&mut self, session: Session, cx: &mut Context<Self>) {
        if session.account == self.reader.account {
            return;
        }
        self.reader.account = session.account;
        self.reader.name = Name::Asking;
        // another reader's half-filled write is not this one's
        if matches!(self.sheet, Some(Sheet::Write(_))) {
            self.sheet = None;
        }
        self.note = None;
        self.read_reader(cx);
        // what this reader may write in the page on screen
        self.read_page(cx);
        cx.notify();
    }
}
