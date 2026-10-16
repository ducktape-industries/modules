//! The writes `files` takes from this view, each one commit with a
//! message: a new folder, a new text file, a text file's new text, a rename
//! or move, a removal. Which of them the page on screen offers is
//! [`Files::offered`]; a form opens with [`Files::begin`], its change is
//! built by [`built`] (pure, unit tested), sent by [`Files::submit`] and
//! settled by `landed`. The app's confirm card is the yes for every op;
//! nothing here asks again.
use std::collections::BTreeMap;

use files::{BlobId, Change, Content, Node};
use view::methods::Submitted;
use view::prelude::*;

use crate::state::{Note, Route, Sheet, child_of};
use crate::{Files, words};

/// One write, and what it is made on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Write {
    /// a folder made in `folder`
    NewFolder { folder: String },
    /// a text file made in `folder`
    NewFile { folder: String },
    /// a text file's text replaced; it keeps its mode and metadata
    Edit {
        path: String,
        exec: bool,
        meta: BTreeMap<String, String>,
    },
    /// a file or folder moved to another path, its folder or its name
    Rename { path: String },
    /// a file or folder removed, a folder with all it holds
    Remove { path: String, folder: bool },
}

/// What a form's first field holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Naming {
    /// one name, in the folder the write is made in
    Name,
    /// a whole path: a rename's, which may move it to another folder
    Path,
}

impl Write {
    /// What its form's first field holds, if it has one.
    pub(crate) fn naming(&self) -> Option<Naming> {
        match self {
            Write::NewFolder { .. } | Write::NewFile { .. } => Some(Naming::Name),
            Write::Rename { .. } => Some(Naming::Path),
            Write::Edit { .. } | Write::Remove { .. } => None,
        }
    }

    /// Whether its form takes a file's text.
    pub(crate) fn takes_text(&self) -> bool {
        matches!(self, Write::NewFile { .. } | Write::Edit { .. })
    }

    /// What the control that begins it is keyed by: `files/begin/<slug>`.
    pub(crate) fn slug(&self) -> &'static str {
        match self {
            Write::NewFolder { .. } => "new-folder",
            Write::NewFile { .. } => "new-file",
            Write::Edit { .. } => "edit",
            Write::Rename { .. } => "rename",
            Write::Remove { .. } => "remove",
        }
    }

    /// The glyph (a Lucide name) its sheet row leads with.
    pub(crate) fn glyph(&self) -> &'static str {
        match self {
            Write::NewFolder { .. } => "folder-plus",
            Write::NewFile { .. } => "file-plus",
            Write::Edit { .. } => "pencil",
            Write::Rename { .. } => "folder-input",
            Write::Remove { .. } => "trash",
        }
    }
}

/// A write being filled in, over the page it was opened from.
pub(crate) struct Form {
    /// which form this is: a commit's answer lands on its own form only,
    /// never on one opened for the same write after it
    pub(crate) id: u64,
    pub(crate) write: Write,
    /// the snapshot the page was read at: the commit's base
    pub(crate) base: Option<BlobId>,
    /// a new folder's or file's name; a rename's new path
    pub(crate) name: TextField,
    /// a new file's or an edit's text
    pub(crate) text: TextField,
    /// the commit's message; left empty, [`words::message`] says it
    pub(crate) message: TextField,
    /// why the name or path is not one `files` takes: the field's own
    pub(crate) problem: Option<String>,
    /// why the last commit did not land, in the host's or the program's
    /// words: the form's
    pub(crate) refused: Option<String>,
    /// sent and not yet answered: no second press
    pub(crate) sending: bool,
}

impl Form {
    /// What the message field says while it is empty: the message the
    /// commit would carry.
    pub(crate) fn placeholder(&self) -> String {
        built(self).map_or_else(|_| words::MESSAGE.to_owned(), |(message, _)| message)
    }
}

/// A commit in flight: the form it came from, its change and message.
struct Sent {
    form: u64,
    change: Change,
    message: String,
}

impl Files {
    /// The writes the reader may make from the page on screen, in the
    /// order both designs offer them: in a folder, what is made in it,
    /// then the folder's own; on a file, its own. `files` decides again
    /// at the commit; this only leaves out what it would refuse.
    pub(crate) fn offered(&self) -> Vec<Write> {
        let mut offered = Vec::new();
        match self.page() {
            Route::Folder(path) => {
                let Some(folder) = self.folder_at(path) else {
                    return offered;
                };
                if self.reader.writes(&folder.children()) {
                    offered.push(Write::NewFolder {
                        folder: path.clone(),
                    });
                    offered.push(Write::NewFile {
                        folder: path.clone(),
                    });
                }
                let own = folder.ownership.as_ref();
                if own.is_some_and(|own| self.reader.writes(&own.writers)) {
                    offered.push(Write::Rename { path: path.clone() });
                    offered.push(Write::Remove {
                        path: path.clone(),
                        folder: true,
                    });
                }
            }
            Route::File(path) => {
                let Some(file) = self.file_at(path) else {
                    return offered;
                };
                let (Some(entry), Some(own)) = (file.file(), &file.ownership) else {
                    return offered;
                };
                if !self.reader.writes(&own.writers) {
                    return offered;
                }
                if let (Node::File { exec, .. }, true) = (&entry.node, file.editable()) {
                    offered.push(Write::Edit {
                        path: path.clone(),
                        exec: *exec,
                        meta: entry.meta.clone(),
                    });
                }
                offered.push(Write::Rename { path: path.clone() });
                offered.push(Write::Remove {
                    path: path.clone(),
                    folder: false,
                });
            }
            Route::Places | Route::History | Route::Snapshot(_) => {}
        }
        offered
    }

    /// Opens the form for `write`, based on the snapshot the page on
    /// screen was read at; a rename starts from the path as it is, an
    /// edit from the text as it is.
    pub(crate) fn begin(&mut self, write: Write, cx: &mut Context<Self>) {
        let (base, text) = match self.page() {
            Route::File(path) => {
                let file = self.file_at(path);
                let text = file.and_then(|file| file.text()).unwrap_or_default();
                (file.and_then(|file| file.snapshot), text.to_owned())
            }
            Route::Folder(path) => (
                self.folder_at(path).and_then(|folder| folder.snapshot),
                String::new(),
            ),
            Route::Places | Route::History | Route::Snapshot(_) => (None, String::new()),
        };
        let name = match &write {
            Write::Rename { path } => path.clone(),
            _ => String::new(),
        };
        let text = match &write {
            Write::Edit { .. } => text,
            _ => String::new(),
        };
        self.forms += 1;
        self.sheet = Some(Sheet::Write(Form {
            id: self.forms,
            write,
            base,
            name: TextField::new(name),
            text: TextField::new(text),
            message: TextField::default(),
            problem: None,
            refused: None,
            sending: false,
        }));
        cx.notify();
    }

    /// Sends the open form's commit; a form that builds none says why.
    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(Sheet::Write(form)) = &mut self.sheet else {
            return;
        };
        if form.sending {
            return;
        }
        let (message, change) = match built(form) {
            Ok(built) => built,
            Err(problem) => {
                form.problem = Some(problem);
                cx.notify();
                return;
            }
        };
        form.problem = None;
        form.refused = None;
        form.sending = true;
        let op = files::Op::Commit {
            base: form.base,
            message: message.clone(),
            changes: vec![change.clone()],
        };
        let sent = Sent {
            form: form.id,
            change,
            message,
        };
        let ask = cx.host().ask::<Submit<files::Files>>(op);
        cx.land(ask, move |view, answer, cx| view.landed(sent, answer, cx))
            .detach();
        cx.notify();
    }

    /// A commit landed: its form closes, the line says so, and the page
    /// follows what moved; else its form (or, closed, the line) says why
    /// not, in the program's words. Another form opened meanwhile is not
    /// this one's to close or to word.
    fn landed(&mut self, sent: Sent, answer: Result<Submitted, Error>, cx: &mut Context<Self>) {
        let problem = match answer {
            Ok(Submitted::Applied { .. }) => None,
            Ok(Submitted::Refused { refusal, .. }) | Err(refusal) => Some(refusal.message),
            Ok(Submitted::InDoubt { .. }) => Some(words::IN_DOUBT.into()),
        };
        let form = match &mut self.sheet {
            Some(Sheet::Write(form)) if form.id == sent.form => Some(form),
            _ => None,
        };
        match (problem, form) {
            (None, form) => {
                if form.is_some() {
                    self.sheet = None;
                }
                self.follow(&sent.change);
                self.note = Some(Note::Committed(sent.message));
                self.read_page(cx);
            }
            (Some(problem), Some(form)) => {
                form.sending = false;
                form.refused = Some(problem);
            }
            (Some(problem), None) => self.note = Some(Note::Refused(problem)),
        }
        cx.notify();
    }

    /// The page on screen after its own path moved or went: the new path
    /// in its place, or the page under it.
    fn follow(&mut self, change: &Change) {
        match (self.page().clone(), change) {
            (Route::File(shown), Change::Move { from, to }) if shown == *from => {
                self.pages.replace(Route::File(to.clone()))
            }
            (Route::Folder(shown), Change::Move { from, to }) if shown == *from => {
                self.pages.replace(Route::Folder(to.clone()))
            }
            (Route::File(shown) | Route::Folder(shown), Change::Remove { path })
                if shown == *path =>
            {
                self.pages.pop();
            }
            _ => {}
        }
    }
}

/// The commit a form builds: its message (the typed one, else the
/// change's own words) and its one change; or why it builds none.
pub(crate) fn built(form: &Form) -> Result<(String, Change), String> {
    let text = || Content::Inline(form.text.text().into_bytes());
    let change = match &form.write {
        Write::NewFolder { folder } => Change::Mkdir {
            path: path_in(folder, &form.name.text())?,
        },
        Write::NewFile { folder } => Change::Put {
            path: path_in(folder, &form.name.text())?,
            exec: false,
            meta: BTreeMap::new(),
            content: text(),
        },
        Write::Edit { path, exec, meta } => Change::Put {
            path: path.clone(),
            exec: *exec,
            meta: meta.clone(),
            content: text(),
        },
        Write::Rename { path } => {
            let to = form.name.text().trim().to_owned();
            checked(&to)?;
            if to == *path {
                return Err(words::SAME_PATH.into());
            }
            Change::Move {
                from: path.clone(),
                to,
            }
        }
        Write::Remove { path, .. } => Change::Remove { path: path.clone() },
    };
    let typed = form.message.text().trim().to_owned();
    let message = match typed.is_empty() {
        true => words::message(&change),
        false => typed,
    };
    Ok((message, change))
}

/// `name` inside `folder`, if `name` is one name.
fn path_in(folder: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(words::NAME_IT.into());
    }
    if name.contains('/') {
        return Err(words::ONE_NAME.into());
    }
    let path = child_of(folder, name);
    checked(&path)?;
    Ok(path)
}

/// A path `files` takes in a change, or its words for why not.
fn checked(path: &str) -> Result<(), String> {
    files::paths::change_path(path)
        .map(drop)
        .map_err(|refusal| refusal.message)
}
