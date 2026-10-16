//! An upload: files the reader picks on their device (`file.pick`), put in
//! a folder by their own names in one commit. The host holds the files
//! picked open, and the view reads each a slice at a time (`file.read`),
//! never whole: a small file is read whole only to ride in the commit
//! itself; past what one commit should carry, a file goes first a chunk at
//! a time, each chunk read and then staged (`Stage`, one op per chunk,
//! answered with the chunk's id), and the commit names its chunks. A file
//! larger than [`UPLOAD_MOST`], one whose name `files` takes no path from,
//! one whose name a folder or a link holds there, or a second of one name
//! is left out and says why. Opened by [`Files::pick`]; the form's Commit
//! reads and stages what is left ([`Files::submit`]), then commits.
use files::{BlobId, Change, Content, EntryInfo, Node};
use view::methods::{FILE_READ_MOST, FilePick, FileRead, Pick, Picked, ReadFile, Submitted};
use view::prelude::*;

use crate::state::{Note, Sheet};
use crate::write::{Form, Write, path_in};
use crate::{Files, words};

/// The most a file may hold to be uploaded here.
pub const UPLOAD_MOST: u64 = 32 << 20;

/// What one commit carries of its files' bytes at most; a file past it
/// is staged ahead in chunks.
const INLINE_MOST: u64 = files::CHUNK_SIZE;

// a file that rides in the commit, and every chunk, is one `file.read`
const _: () = assert!(INLINE_MOST <= FILE_READ_MOST && files::CHUNK_SIZE <= FILE_READ_MOST);

/// One picked file, as the upload's form holds it.
pub(crate) struct Upload {
    /// its own name on the reader's device
    pub(crate) name: String,
    pub(crate) size: u64,
    /// the handle the host reads it by
    file: u64,
    /// where it goes, or why it is left out
    pub(crate) path: Result<String, String>,
    /// a file of its name is in the folder: this one replaces it
    pub(crate) replaces: bool,
    way: Way,
}

/// How a file reaches the commit.
enum Way {
    /// in the commit itself: its bytes, once read
    Inline(Option<Vec<u8>>),
    /// ahead of it in chunks: each chunk's id once staged, in order
    Staged(Vec<BlobId>),
}

impl Upload {
    /// What the commit puts, when the file is not left out and it is read,
    /// or every one of its chunks staged.
    fn put(&self) -> Option<Change> {
        let path = self.path.as_ref().ok()?;
        let content = match &self.way {
            Way::Inline(bytes) => Content::Inline(bytes.clone()?),
            Way::Staged(staged) if staged.len() as u64 == chunks(self.size) => Content::Chunks {
                size: self.size,
                chunks: staged.clone(),
            },
            Way::Staged(_) => return None,
        };
        Some(Change::Put {
            path: path.clone(),
            exec: false,
            meta: Default::default(),
            content,
        })
    }

    /// The next slice to read of it, `(offset, len)`: the whole of a file
    /// that rides in the commit, else its next chunk.
    fn unread(&self) -> Option<(u64, u64)> {
        if self.path.is_err() {
            return None;
        }
        match &self.way {
            Way::Inline(None) => Some((0, self.size)),
            Way::Inline(Some(_)) => None,
            Way::Staged(staged) => {
                let offset = staged.len() as u64 * files::CHUNK_SIZE;
                (offset < self.size).then(|| (offset, files::CHUNK_SIZE.min(self.size - offset)))
            }
        }
    }
}

/// How many chunks `files` cuts a file of `size` bytes into.
fn chunks(size: u64) -> u64 {
    size.div_ceil(files::CHUNK_SIZE)
}

/// The files picked, as the form holds them: each one's path in `folder`
/// or why it is left out, and whether it goes in the commit or ahead of
/// it, the commit carrying at most [`INLINE_MOST`] of their bytes in all.
/// `listed` is what the folder holds already.
pub(crate) fn uploads(folder: &str, picked: Vec<Picked>, listed: &[EntryInfo]) -> Vec<Upload> {
    let mut inline_room = INLINE_MOST;
    let mut named: Vec<String> = Vec::new();
    picked
        .into_iter()
        .map(|picked| {
            let path = match picked.size {
                size if size > UPLOAD_MOST => Err(words::too_big_to_upload(size)),
                _ => path_in(folder, &picked.name),
            };
            let there = path
                .as_ref()
                .ok()
                .and_then(|path| listed.iter().find(|entry| entry.path == *path));
            let path = path.and_then(|path| match there.map(|entry| &entry.node) {
                // a file in its place is replaced; a folder or a link is not
                Some(node @ (Node::Dir { .. } | Node::Symlink { .. })) => Err(words::held_by(node)),
                _ if named.contains(&path) => Err(words::PICKED_TWICE.into()),
                _ => Ok(path),
            });
            if let Ok(path) = &path {
                named.push(path.clone());
            }
            let inline = picked.size <= inline_room;
            if inline && path.is_ok() {
                inline_room -= picked.size;
            }
            Upload {
                name: picked.name,
                size: picked.size,
                file: picked.file,
                replaces: path.is_ok() && there.is_some(),
                path,
                way: match inline {
                    true => Way::Inline(None),
                    false => Way::Staged(Vec::new()),
                },
            }
        })
        .collect()
}

/// What the upload's commit puts: every file not left out, read or its
/// chunks all staged; or why it puts nothing.
pub(crate) fn puts(uploads: &[Upload]) -> Result<Vec<Change>, String> {
    let puts: Vec<Change> = uploads.iter().filter_map(Upload::put).collect();
    match puts.is_empty() {
        true => Err(words::NOTHING_TO_UPLOAD.into()),
        false => Ok(puts),
    }
}

/// The names of the files the upload sends: every one not left out.
pub(crate) fn sent(uploads: &[Upload]) -> Vec<&str> {
    uploads
        .iter()
        .filter(|upload| upload.path.is_ok())
        .map(|upload| upload.name.as_str())
        .collect()
}

/// How far the staging is: chunks staged, of all to stage; `None` when
/// nothing goes ahead of the commit.
pub(crate) fn progress(uploads: &[Upload]) -> Option<(u64, u64)> {
    let ahead = || {
        uploads
            .iter()
            .filter(|upload| upload.path.is_ok())
            .filter_map(|upload| match &upload.way {
                Way::Staged(staged) => Some((staged.len() as u64, chunks(upload.size))),
                Way::Inline(_) => None,
            })
    };
    let all: u64 = ahead().map(|(_, all)| all).sum();
    let staged: u64 = ahead().map(|(staged, _)| staged).sum();
    (all > 0).then_some((staged, all))
}

/// Every chunk the upload's files have staged, in order.
fn staged(uploads: &[Upload]) -> Vec<BlobId> {
    uploads
        .iter()
        .filter_map(|upload| match &upload.way {
            Way::Staged(staged) => Some(staged.iter().copied()),
            Way::Inline(_) => None,
        })
        .flatten()
        .collect()
}

/// Each file's staged chunks cut back to the first `held` says the network
/// does not hold: those go again. `held` answers [`staged`]'s chunks.
fn keep_held(uploads: &mut [Upload], mut held: &[bool]) {
    for upload in uploads {
        if let Way::Staged(staged) = &mut upload.way {
            let (its, rest) = held.split_at(staged.len().min(held.len()));
            held = rest;
            staged.truncate(its.iter().take_while(|held| **held).count());
        }
    }
}

impl Files {
    /// Asks the reader for files on their device, to upload into `folder`;
    /// picked, their form opens. Only a press reaches here: the host opens
    /// its picker for nothing else.
    pub(crate) fn pick(&mut self, folder: String, cx: &mut Context<Self>) {
        if self.picking {
            return;
        }
        self.picking = true;
        let ask = cx.host().ask::<FilePick>(Pick { multiple: true });
        cx.land(ask, move |view, answer, cx| view.picked(folder, answer, cx))
            .detach();
    }

    /// The reader picked files, or none; or the host could not ask.
    fn picked(
        &mut self,
        folder: String,
        answer: Result<Vec<Picked>, Error>,
        cx: &mut Context<Self>,
    ) {
        self.picking = false;
        let picked = match answer {
            Ok(picked) => picked,
            Err(refusal) => {
                self.note = Some(Note::Refused(design::sentence(&refusal)));
                cx.notify();
                return;
            }
        };
        // cancelled
        if picked.is_empty() {
            return;
        }
        // the folder left the screen while the picker was open: what it
        // holds, and so what a file would replace, is not known
        let Some(listed) = self.folder_at(&folder) else {
            self.note = Some(Note::Refused(words::FOLDER_LEFT.into()));
            cx.notify();
            return;
        };
        let base = listed.snapshot;
        let uploads = uploads(&folder, picked, &listed.entries);
        self.open_upload(Write::Upload { folder }, base, uploads, cx);
    }

    /// Reads the open upload's next slice, if one is left: `false` when
    /// every file is ready for the commit.
    pub(crate) fn advance(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Sheet::Write(form)) = &mut self.sheet else {
            return false;
        };
        let Some((index, file, (offset, len))) = form
            .upload
            .iter()
            .enumerate()
            .find_map(|(index, upload)| Some((index, upload.file, upload.unread()?)))
        else {
            return false;
        };
        form.sending = true;
        let id = form.id;
        let ask = cx.host().ask::<FileRead>(ReadFile { file, offset, len });
        cx.land(ask, move |view, answer, cx| {
            view.slice_read(id, index, (offset, len), answer, cx)
        })
        .detach();
        true
    }

    /// The form `id`, while it is the one open.
    fn open_form(&mut self, id: u64) -> Option<&mut Form> {
        match &mut self.sheet {
            Some(Sheet::Write(form)) if form.id == id => Some(form),
            _ => None,
        }
    }

    /// The form an upload's answer was for is closed, or another opened:
    /// the upload stopped, and nothing of it was committed.
    fn stopped(&mut self, cx: &mut Context<Self>) {
        self.note = Some(Note::Refused(words::UPLOAD_STOPPED.into()));
        cx.notify();
    }

    /// A slice was read: a file that rides in the commit keeps it and the
    /// upload goes on; a chunk is staged. A slice shorter than asked is a
    /// file changed since it was picked.
    fn slice_read(
        &mut self,
        form: u64,
        index: usize,
        (offset, len): (u64, u64),
        answer: Result<Vec<u8>, Error>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.open_form(form) else {
            return self.stopped(cx);
        };
        let upload = &mut open.upload[index];
        let bytes = match answer {
            Ok(bytes) if bytes.len() as u64 == len => bytes,
            Ok(_) => {
                open.sending = false;
                open.refused = Some(words::changed_since_picked(&upload.name));
                return cx.notify();
            }
            Err(refusal) => {
                open.sending = false;
                open.refused = Some(design::sentence(&refusal));
                return cx.notify();
            }
        };
        match &mut upload.way {
            Way::Inline(held) => {
                *held = Some(bytes);
                open.sending = false;
                self.submit(cx);
            }
            Way::Staged(_) => {
                let at = (offset / files::CHUNK_SIZE) as usize;
                let ask = cx
                    .host()
                    .ask::<Submit<files::Files>>(files::Op::Stage { bytes });
                cx.land(ask, move |view, answer, cx| {
                    view.staged(form, index, at, answer, cx)
                })
                .detach();
            }
        }
    }

    /// The chunk at `at` was staged: its id is kept in its place and the
    /// upload goes on; else the form says why not.
    fn staged(
        &mut self,
        form: u64,
        index: usize,
        at: usize,
        answer: Result<Submitted, Error>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.open_form(form) else {
            return self.stopped(cx);
        };
        open.sending = false;
        let chunk = match &answer {
            Ok(Submitted::Applied { output, .. }) => {
                abi::decode::<BlobId>(output).map_err(|_| words::NO_CHUNK_ID.to_owned())
            }
            _ => Err(design::failure(&answer).unwrap_or_default()),
        };
        match chunk {
            Ok(chunk) => {
                // a chunk kept only in its own place: a list cut back since
                // it was read is never filled out of order
                if let Way::Staged(staged) = &mut open.upload[index].way
                    && staged.len() == at
                {
                    staged.push(chunk);
                }
                self.submit(cx);
            }
            Err(problem) => open.refused = Some(problem),
        }
        cx.notify();
    }

    /// An upload's commit was refused for a chunk the network does not
    /// hold (a staged chunk is a prediction a node can drop): asks which
    /// it holds, so Commit again stages the rest anew.
    /// Commit waits on the answer, so no chunk is staged meanwhile.
    pub(crate) fn recheck_staged(&mut self, form: u64, cx: &mut Context<Self>) {
        let Some(open) = self.open_form(form) else {
            return;
        };
        open.sending = true;
        let chunks = staged(&open.upload);
        let ask = cx.host().query(files::ask::Held { chunks });
        cx.land(ask, move |view, held, cx| {
            let Some(open) = view.open_form(form) else {
                return;
            };
            open.sending = false;
            // unanswered, the lists stay: Commit again is refused again
            // and asks again
            if let Ok(held) = held {
                keep_held(&mut open.upload, &held);
            }
            cx.notify();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> BlobId {
        BlobId::Sha256([n; 32])
    }

    #[test]
    fn a_held_check_cuts_each_file_back_to_its_first_dropped_chunk() {
        let chunk = files::CHUNK_SIZE;
        let picked = |name: &str, size| Picked {
            name: name.into(),
            size,
            file: 0,
        };
        let mut uploads = uploads(
            "/shared",
            vec![
                picked("a.bin", 3 * chunk),
                picked("tiny.txt", 1),
                picked("b.bin", 2 * chunk),
            ],
            &[],
        );
        uploads[0].way = Way::Staged(vec![id(1), id(2), id(3)]);
        uploads[2].way = Way::Staged(vec![id(4), id(5)]);
        assert_eq!(staged(&uploads), [id(1), id(2), id(3), id(4), id(5)]);
        keep_held(&mut uploads, &[true, false, true, true, true]);
        let kept = |upload: &Upload| match &upload.way {
            Way::Staged(staged) => staged.clone(),
            Way::Inline(_) => panic!("{} rides in the commit", upload.name),
        };
        assert_eq!(kept(&uploads[0]), [id(1)], "from its dropped chunk on");
        assert_eq!(kept(&uploads[2]), [id(4), id(5)], "every chunk held");
        assert_eq!(uploads[0].unread(), Some((chunk, chunk)));
    }
}
