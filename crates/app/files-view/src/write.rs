//! The writes `files` takes from this view, each one commit with a
//! message: a new folder, a new text file, files uploaded from the
//! reader's device (`upload.rs`), a text file's new text, a rename or
//! move, a removal. Which of them the page on screen offers is
//! [`Files::offered`]; a form opens with [`Files::begin`], its changes are
//! built by [`built`] (pure, unit tested), sent by [`Files::submit`] and
//! settled by `landed`. A file's bytes never ride in the commit: each file
//! it puts goes to the network first as `chunk` blobs (`op.put`, answered
//! once the network holds them), and the commit names its chunks. The
//! app's confirm card is the yes for every op; nothing here asks again.
use std::collections::BTreeMap;

use files::{BlobId, Change, Content, Node};
use view::methods::{BlobBody, NewBlob, OpPut, PutBlobs, Submitted};
use view::prelude::*;

use crate::state::{Note, Route, Sheet, child_of};
use crate::upload::{self, Upload};
use crate::{Files, words};

/// One write, and what it is made on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Write {
    /// a folder made in `folder`
    NewFolder { folder: String },
    /// a text file made in `folder`
    NewFile { folder: String },
    /// files picked on the reader's device, put in `folder` by their names
    Upload { folder: String },
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
            Write::Upload { .. } | Write::Edit { .. } | Write::Remove { .. } => None,
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
            Write::Upload { .. } => "upload",
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
            Write::Upload { .. } => "upload",
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
    /// an upload's files; empty for every other write
    pub(crate) upload: Vec<Upload>,
    /// the content each file the commit puts has on the network, in the
    /// order [`files_of`] lists them, as each is put
    pub(crate) contents: Vec<Content>,
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
    /// Whether Commit may be pressed: nothing is in flight, and an upload
    /// has a file to send.
    pub(crate) fn committable(&self) -> bool {
        let empty =
            matches!(self.write, Write::Upload { .. }) && upload::sent(&self.upload).is_empty();
        !self.sending && !empty
    }

    /// How far an upload's sending is, while it sends: files on the
    /// network, of all it sends.
    pub(crate) fn progress(&self) -> Option<(u64, u64)> {
        let Write::Upload { .. } = self.write else {
            return None;
        };
        let all = upload::going(&self.upload).count() as u64;
        Some((self.contents.len() as u64, all))
    }

    /// The content of the `index`th file the commit puts: its chunks once
    /// put; until then none, which only the checks and the message read
    /// before the commit is sent.
    fn content(&self, index: usize) -> Content {
        self.contents
            .get(index)
            .cloned()
            .unwrap_or(Content::Chunks {
                size: 0,
                chunks: Vec::new(),
            })
    }

    /// What the message field says while it is empty: the message the
    /// commit would carry.
    pub(crate) fn placeholder(&self) -> String {
        // an upload is worded by its names, not built for every frame
        if let Write::Upload { .. } = self.write {
            return words::uploads(&upload::sent(&self.upload));
        }
        built(self).map_or_else(|_| words::MESSAGE.to_owned(), |(message, _)| message)
    }
}

/// The blobs of each file a form's commit puts, in order: a new or edited
/// text file's text, an upload's files; none for any other write.
pub(crate) fn files_of(form: &Form) -> Vec<Vec<NewBlob>> {
    match &form.write {
        Write::NewFile { .. } | Write::Edit { .. } => {
            let text = form.text.text().into_bytes();
            let chunks = text
                .chunks(files::CHUNK_SIZE as usize)
                .map(|chunk| NewBlob {
                    kind: files::CHUNK.into(),
                    body: BlobBody::Bytes(chunk.to_vec()),
                });
            vec![chunks.collect()]
        }
        Write::Upload { .. } => upload::going(&form.upload)
            .map(|(upload, _)| upload.blobs())
            .collect(),
        Write::NewFolder { .. } | Write::Rename { .. } | Write::Remove { .. } => Vec::new(),
    }
}

/// How many bytes `blobs` hold.
fn size(blobs: &[NewBlob]) -> u64 {
    blobs
        .iter()
        .map(|blob| match &blob.body {
            BlobBody::Bytes(bytes) => bytes.len() as u64,
            BlobBody::Picked { len, .. } => *len,
        })
        .sum()
}

/// A commit in flight: the form it came from, its changes and message.
struct Sent {
    form: u64,
    changes: Vec<Change>,
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
                    offered.push(Write::Upload {
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
    /// edit from the text as it is. An upload asks for its files first.
    pub(crate) fn begin(&mut self, write: Write, cx: &mut Context<Self>) {
        if let Write::Upload { folder } = write {
            return self.pick(folder, cx);
        }
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
        self.sheet = Some(Sheet::Write(Box::new(Form {
            id: self.forms,
            write,
            base,
            name: TextField::new(name),
            text: TextField::new(text),
            upload: Vec::new(),
            contents: Vec::new(),
            message: TextField::default(),
            problem: None,
            refused: None,
            sending: false,
        })));
        cx.notify();
    }

    /// Opens an upload's form over its picked files.
    pub(crate) fn open_upload(
        &mut self,
        write: Write,
        base: Option<BlobId>,
        upload: Vec<Upload>,
        cx: &mut Context<Self>,
    ) {
        self.forms += 1;
        self.sheet = Some(Sheet::Write(Box::new(Form {
            id: self.forms,
            write,
            base,
            name: TextField::default(),
            text: TextField::default(),
            upload,
            contents: Vec::new(),
            message: TextField::default(),
            problem: None,
            refused: None,
            sending: false,
        })));
        cx.notify();
    }

    /// Commit pressed: the open form's files go to the network, then its
    /// commit; a form that builds none says why, before anything is put.
    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(Sheet::Write(form)) = &mut self.sheet else {
            return;
        };
        if form.sending {
            return;
        }
        form.problem = None;
        form.refused = None;
        // the text may have changed since it was put
        if form.write.takes_text() {
            form.contents.clear();
        }
        if let Err(problem) = built(form) {
            form.problem = Some(problem);
            cx.notify();
            return;
        }
        self.send(cx);
    }

    /// Puts the open form's next file not on the network yet; every one
    /// put, sends its commit.
    fn send(&mut self, cx: &mut Context<Self>) {
        let Some(Sheet::Write(form)) = &mut self.sheet else {
            return;
        };
        form.sending = true;
        if let Some(blobs) = files_of(form).into_iter().nth(form.contents.len()) {
            let size = size(&blobs);
            // an empty file has no chunk to put
            if blobs.is_empty() {
                form.contents.push(Content::Chunks {
                    size,
                    chunks: Vec::new(),
                });
                return self.send(cx);
            }
            let id = form.id;
            let ask = cx.host().ask::<OpPut>(PutBlobs { blobs });
            cx.land(ask, move |view, answer, cx| view.held(id, size, answer, cx))
                .detach();
            cx.notify();
            return;
        }
        let (message, changes) = match built(form) {
            Ok(built) => built,
            Err(problem) => {
                form.sending = false;
                form.problem = Some(problem);
                cx.notify();
                return;
            }
        };
        let op = files::Op::Commit {
            base: form.base,
            message: message.clone(),
            changes: changes.clone(),
        };
        let sent = Sent {
            form: form.id,
            changes,
            message,
        };
        let ask = cx.host().ask::<Submit<files::Files>>(op);
        cx.land(ask, move |view, answer, cx| view.landed(sent, answer, cx))
            .detach();
        cx.notify();
    }

    /// A file of `size` bytes is on the network: its chunks are its
    /// content, and the form goes on; else the form says why not. The form
    /// closed meanwhile, nothing was committed, and the line says so.
    fn held(
        &mut self,
        form: u64,
        size: u64,
        answer: Result<Vec<[u8; 32]>, Error>,
        cx: &mut Context<Self>,
    ) {
        let Some(Sheet::Write(open)) = &mut self.sheet else {
            return self.stopped(cx);
        };
        if open.id != form {
            return self.stopped(cx);
        }
        match answer {
            Ok(digests) => {
                let chunks = digests.into_iter().map(BlobId::Sha256).collect();
                open.contents.push(Content::Chunks { size, chunks });
                self.send(cx);
            }
            Err(refusal) => {
                open.sending = false;
                open.refused = Some(design::sentence(&refusal));
                cx.notify();
            }
        }
    }

    /// The form a put was for closed, or another opened: nothing of it
    /// was committed.
    fn stopped(&mut self, cx: &mut Context<Self>) {
        self.note = Some(Note::Refused(words::STOPPED.into()));
        cx.notify();
    }

    /// A commit landed: its form closes, the line says so, and the page
    /// follows what moved; else its form (or, closed, the line) says why
    /// not, in every view's words (`design::failure`). Another form opened
    /// meanwhile is not this one's to close or to word.
    fn landed(&mut self, sent: Sent, answer: Result<Submitted, Error>, cx: &mut Context<Self>) {
        let problem = design::failure(&answer);
        let form = match &mut self.sheet {
            Some(Sheet::Write(form)) if form.id == sent.form => Some(form),
            _ => None,
        };
        match (problem, form) {
            (None, form) => {
                if form.is_some() {
                    self.sheet = None;
                }
                sent.changes.iter().for_each(|change| self.follow(change));
                self.note = Some(Note::Committed(sent.message));
                self.read_all(cx);
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
/// changes' own words) and its changes, one but for an upload's; or why
/// it builds none.
pub(crate) fn built(form: &Form) -> Result<(String, Vec<Change>), String> {
    let text = || form.content(0);
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
        Write::Upload { .. } => {
            let puts = upload::puts(&form.upload, |index| form.content(index))?;
            let said = words::uploads(&upload::sent(&form.upload));
            return Ok((message(form, said), puts));
        }
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
    Ok((message(form, words::message(&change)), vec![change]))
}

/// The typed message, else `said`.
fn message(form: &Form, said: String) -> String {
    let typed = form.message.text().trim().to_owned();
    match typed.is_empty() {
        true => said,
        false => typed,
    }
}

/// `name` inside `folder`, if `name` is one name.
pub(crate) fn path_in(folder: &str, name: &str) -> Result<String, String> {
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
