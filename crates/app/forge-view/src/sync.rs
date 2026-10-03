//! Which reads the screen on display needs, and keeping them fresh. Every
//! read is cached under the `Query` that asked it; `sync` issues what the
//! screen lacks and drops what it no longer shows.
use std::collections::BTreeSet;

use ducktape_view_guest::Context;
use ducktape_view_guest::Loadable;
use ducktape_view_guest::host::Error;

use crate::api::Session;
use crate::queries::{self, PAGE};
use crate::state::{ChangeTab, Filter, Forge, Progress, RepoTab};
use forge::{ChangeFilter, ChangeState, Query, Revision};

/// How many branches the Refs screen compares against the default head in
/// one pass. Beyond that the screen says so rather than walking a fleet of
/// refs through the program's compare budget.
pub(crate) const COMPARED_REFS: usize = 20;

impl Forge {
    pub(crate) fn session_changed(&mut self, next: Session, cx: &mut Context<Self>) {
        if next == self.session {
            return;
        }
        let reader_changed = next.signer != self.session.signer;
        self.session = next;
        if reader_changed {
            self.names = cx.load(queries::roster(cx.host()), |forge| &mut forge.names);
            self.data.clear();
            self.rereading.clear();
        }
        cx.notify();
        self.sync(cx);
    }

    /// One read, once. Landing it advances whatever depends on it.
    pub(crate) fn read(&mut self, query: Query, cx: &mut Context<Self>) {
        if self.data.contains_key(&query) {
            return;
        }
        let landing = query.clone();
        let asked = query.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = queries::fetch(cx.host(), asked).await;
            // the view is gone: nothing is waiting for this read
            let _ = this.update(cx, |forge, cx| {
                forge.data.insert(landing, Loadable::from(result));
                cx.notify();
                forge.sync(cx);
            });
        });
        self.data.insert(query, Loadable::Loading(task));
    }

    /// Ask again for everything on screen, keeping what is there until the
    /// fresh answer lands; a read that lands what is there draws nothing.
    /// Each read lives in its slot, or beside the refusal it shows, so a
    /// screen the reader leaves drops its reads with it. A read still out is left to land: its landing
    /// runs `sync`, and the next block asks it again. A refusal stays on
    /// screen while it is asked again, and an answer replaces it.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        for (query, slot) in self.data.iter_mut() {
            let landing = query.clone();
            let work = queries::fetch(cx.host(), query.clone());
            match slot {
                Loadable::Idle | Loadable::Loading(_) => {}
                Loadable::Failed(_) => {
                    let task = cx.refresh(work, move |forge, result, cx| {
                        if replaces_refusal(forge.data.get_mut(&landing), result) {
                            cx.notify();
                            forge.sync(cx);
                        }
                    });
                    self.rereading.insert(query.clone(), task);
                }
                Loadable::Ready(_) | Loadable::Reloading(..) => {
                    cx.reload(slot, work, move |forge| {
                        forge.data.entry(landing.clone()).or_insert(Loadable::Idle)
                    })
                }
            }
        }
        let viewer = self.viewer();
        for (channel, slot) in self.messages.iter_mut() {
            let landing = channel.clone();
            let rows = queries::conversation(cx.host(), channel.clone(), viewer.clone());
            match slot {
                Loadable::Idle | Loadable::Loading(_) => {}
                Loadable::Failed(_) => {
                    let task = cx.refresh(rows, move |forge, result, cx| {
                        if replaces_refusal(forge.messages.get_mut(&landing), result) {
                            cx.notify();
                        }
                    });
                    self.rereading_messages.insert(channel.clone(), task);
                }
                Loadable::Ready(_) | Loadable::Reloading(..) => {
                    cx.reload(slot, rows, move |forge| {
                        forge.messages.entry(landing.clone()).or_default()
                    })
                }
            }
        }
        self.sync(cx);
    }

    /// A new block landed: retire what it carried, then re-read.
    pub(crate) fn reconcile(&mut self, cx: &mut Context<Self>) {
        let pending = self.pending.len();
        self.pending.retain(|op| op.progress != Progress::Accepted);
        if self.pending.len() != pending {
            cx.notify();
        }
        self.refresh(cx);
    }

    /// Retry one read the reader asked to retry.
    pub(crate) fn retry(&mut self, query: Query, cx: &mut Context<Self>) {
        self.rereading.remove(&query);
        self.data.remove(&query);
        self.read(query, cx);
        cx.notify();
    }

    /// Issue what this screen needs and drop what it does not.
    pub(crate) fn sync(&mut self, cx: &mut Context<Self>) {
        self.land_goto();
        let needed = self.needed();
        let keys: BTreeSet<&Query> = needed.iter().collect();
        self.data.retain(|query, _| keys.contains(query));
        self.rereading.retain(|query, _| keys.contains(query));
        for query in needed {
            self.read(query, cx);
        }
        let Some(channel) = self.open_channel() else {
            self.messages.clear();
            self.rereading_messages.clear();
            return;
        };
        if !self.messages.contains_key(&channel) {
            let conversation = queries::conversation(cx.host(), channel.clone(), self.viewer());
            let landing = channel.clone();
            let slot = cx.load(conversation, move |forge| {
                forge.messages.entry(landing.clone()).or_default()
            });
            self.messages.insert(channel, slot);
        }
    }

    /// Every question the current screen has. A question whose arguments are
    /// not known yet (a tree needs its commit) simply is not asked until the
    /// read that answers it lands.
    fn needed(&self) -> Vec<Query> {
        let mut wanted = vec![Query::Repos { page: PAGE }];
        let Some(repo) = self.nav.repo.clone() else {
            return wanted;
        };
        wanted.push(self.repo_query());
        wanted.push(self.refs_query());
        wanted.push(Query::Activity { repo: repo.clone() });
        match self.nav.change {
            Some(n) => wanted.extend(self.change_reads(&repo, n)),
            None => wanted.extend(self.tab_reads(repo)),
        }
        wanted
    }

    /// The reads of the repository tab on screen.
    fn tab_reads(&self, repo: String) -> Vec<Query> {
        let blob = |oid: String| Query::Blob {
            repo: repo.clone(),
            oid,
            range: None,
        };
        match self.nav.tab {
            RepoTab::Readme => {
                let readme = self.readme().map(|(_, oid)| blob(oid));
                self.tree_query(Vec::new())
                    .into_iter()
                    .chain(readme)
                    .collect()
            }
            RepoTab::Code => {
                let open = self.nav.blob.as_ref().map(|(_, oid)| blob(oid.clone()));
                self.tree_queries().into_iter().chain(open).collect()
            }
            RepoTab::Commits => {
                let log = Query::Log {
                    repo: repo.clone(),
                    from: self.revision(),
                    exclude: None,
                    page: PAGE,
                };
                let diff = self.nav.commit.clone().map(|commit| Query::Diff {
                    repo: repo.clone(),
                    base: self.commit_parent(&commit),
                    head: commit,
                    path: None,
                    page: PAGE,
                });
                std::iter::once(log).chain(diff).collect()
            }
            RepoTab::Changes => self.changes_query(&repo).into_iter().collect(),
            RepoTab::Refs => {
                let head = self.head_name();
                self.branches()
                    .into_iter()
                    .take(COMPARED_REFS)
                    .filter(|name| *name != head)
                    .map(|name| Query::Compare {
                        repo: repo.clone(),
                        from: Revision::Ref(name),
                        into: Revision::Ref(head.clone()),
                    })
                    .collect()
            }
            RepoTab::Settings => Vec::new(),
        }
    }

    /// The reads one open change needs: its record, how it compares with its
    /// target, and the diff of that comparison.
    fn change_reads(&self, repo: &str, n: u64) -> Vec<Query> {
        let mut wanted = vec![Query::Change {
            repo: repo.to_owned(),
            n,
            page: PAGE,
        }];
        let (Some((_, source_head, _, _)), Some((from, into))) = (self.change(), self.endpoints())
        else {
            return wanted;
        };
        wanted.extend(self.compare_query());
        if self.nav.change_tab == ChangeTab::Commits {
            // the change's own commits: what its target does not reach
            wanted.push(crate::ui::commits::query(self, from, Some(into)));
        }
        if let (ChangeTab::Files, Some(head), Some(comparison)) =
            (self.nav.change_tab, source_head.clone(), self.compare())
        {
            wanted.push(Query::Diff {
                repo: repo.to_owned(),
                base: comparison.base.clone(),
                head,
                path: None,
                page: PAGE,
            });
        }
        wanted
    }

    /// The Changes tab's question. A filter about "me" asks nothing while
    /// the reader is nobody (no account): there is no one to judge.
    pub(crate) fn changes_query(&self, repo: &str) -> Option<Query> {
        let me = self.me_principal();
        let state = match self.filter {
            Filter::Judgment => {
                return Some(Query::Judgment {
                    principal: me?,
                    page: PAGE,
                });
            }
            Filter::Merged => Some(ChangeState::Merged),
            Filter::Closed => Some(ChangeState::Closed),
            Filter::Open => Some(ChangeState::Open),
            Filter::Authored | Filter::Involves => None,
        };
        let personal = matches!(self.filter, Filter::Authored | Filter::Involves);
        if personal && me.is_none() {
            return None;
        }
        Some(Query::Changes {
            repo: repo.to_owned(),
            filter: ChangeFilter {
                state,
                author: me.clone().filter(|_| self.filter == Filter::Authored),
                involves: me.filter(|_| self.filter == Filter::Involves),
            },
            page: PAGE,
        })
    }
}

/// Lands a refusal's new answer on the slot that showed it: a value, or
/// another refusal, replaces it, and the same refusal again changes
/// nothing. A slot no longer showing a refusal (left, or retried) takes
/// nothing. Says whether the slot changed.
fn replaces_refusal<T>(slot: Option<&mut Loadable<T>>, result: Result<T, Error>) -> bool {
    let Some(slot) = slot.filter(|slot| slot.failed().is_some()) else {
        return false;
    };
    if slot.failed() == result.as_ref().err() {
        return false;
    }
    *slot = Loadable::from(result);
    true
}
