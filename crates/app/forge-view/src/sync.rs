//! Which reads the screen on display needs, and keeping them fresh. Every
//! read is cached under the `Query` that asked it; `sync` issues what the
//! screen lacks and drops what it no longer shows. A log is held a page at
//! a time (`logs`), every other read whole (`data`).
use std::collections::BTreeSet;

use ducktape_view_guest::Paged;
use ducktape_view_guest::prelude::*;

use crate::queries::{self, PAGE};
use crate::state::{ChangeTab, Filter, Forge, Progress, RepoTab};
use forge::{ChangeFilter, ChangeState, PageRequest, Query, Revision};

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
            // another reader: the names shown are read as theirs until
            // the roster lands again
            self.names = Loadable::Idle;
            cx.load(self, queries::roster(cx.host()), |forge| &mut forge.names);
            self.data.clear();
            self.logs.clear();
            self.rereading.clear();
        }
        cx.notify();
        self.sync(cx);
    }

    /// One read, once. Landing it advances whatever depends on it. A log
    /// reads its first page, and on from there as its list is scrolled.
    pub(crate) fn read(&mut self, query: Query, cx: &mut Context<Self>) {
        if self.data.contains_key(&query) || self.logs.contains_key(&query) {
            return;
        }
        if matches!(query, Query::Log { .. }) {
            let (host, asked) = (cx.host(), query.clone());
            let page = move |after| queries::log_page(host.clone(), asked.clone(), after);
            let log = cx.new(|cx| Paged::new(page, cx));
            let landed = cx.observe(&log, |forge, _, cx| forge.sync(cx));
            self.logs.insert(query, (log, landed));
            return;
        }
        let landing = query.clone();
        let asked = query.clone();
        let fetch = queries::fetch(cx.host(), asked);
        let task = cx.land(fetch, move |forge, result, cx| {
            forge.data.insert(landing, Loadable::from(result));
            cx.notify();
            forge.sync(cx);
        });
        self.data.insert(query, Loadable::Loading(task));
    }

    /// Everything on screen, asked again: the view came back into sight.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reread(|_| true, cx);
        self.reread_conversations(None, cx);
    }

    /// A forge block landed: retire the ops it carried, then ask again for
    /// the reads it wrote to. `None` is a reopened node link: everything is
    /// asked again.
    pub(crate) fn reconcile(&mut self, change: Option<&Change>, cx: &mut Context<Self>) {
        let pending = self.pending.len();
        self.pending.retain(|op| op.progress != Progress::Accepted);
        if self.pending.len() != pending {
            cx.notify();
        }
        self.reread(
            |query| change.is_none_or(|change| change.touches::<forge::Forge, _>(query)),
            cx,
        );
    }

    /// A chat block landed: the forge reads answered partly from chat (a
    /// judgment waits on chat's threads) and the change conversations it
    /// wrote to are read again. `None` is a reopened chat link: all of them.
    pub(crate) fn chat_changed(&mut self, change: Option<&Change>, cx: &mut Context<Self>) {
        self.reread(
            |query| change.is_none_or(|change| change.touches::<chat::Chat, _>(query)),
            cx,
        );
        self.reread_conversations(change, cx);
    }

    /// Asks again for the reads on screen that `touched` names, keeping what
    /// is there until the fresh answer lands; a read that lands what is
    /// there draws nothing. Each read lives in its slot, or beside the
    /// refusal it shows, so a screen the reader leaves drops its reads with
    /// it. A read still out is left to land: its landing runs `sync`, and
    /// the next block asks it again. A refusal is asked again with every
    /// forge or chat block, whatever it wrote: forge refuses a listing
    /// `stale` when any op moved its count, and the answer replaces the
    /// refusal. A log reads the pages it holds again, not its whole history.
    fn reread(&mut self, touched: impl Fn(&Query) -> bool, cx: &mut Context<Self>) {
        for (query, (log, _)) in &self.logs {
            let (out, refused) = log.read(|log| (log.is_loading(), log.failed().is_some()));
            if !out && (refused || touched(query)) {
                log.update(cx, |log, cx| log.reread(cx));
            }
        }
        // the reads to ask again, named before any is asked: `load` takes
        // the view, which this loop would otherwise still hold
        let again: Vec<(Query, bool)> = self
            .data
            .iter()
            .filter_map(|(query, slot)| match slot {
                Loadable::Idle | Loadable::Loading(_) => None,
                Loadable::Failed(_) => Some((query.clone(), true)),
                Loadable::Ready(_) | Loadable::Reloading(..) => {
                    touched(query).then(|| (query.clone(), false))
                }
            })
            .collect();
        for (query, refused) in again {
            let work = queries::fetch(cx.host(), query.clone());
            let landing = query.clone();
            if refused {
                let task = cx.land(work, move |forge, result, cx| {
                    if replaces_refusal(forge.data.get_mut(&landing), result) {
                        cx.notify();
                        forge.sync(cx);
                    }
                });
                self.rereading.insert(query, task);
            } else {
                cx.load(self, work, move |forge| {
                    forge.data.entry(landing.clone()).or_insert(Loadable::Idle)
                });
            }
        }
        self.sync(cx);
    }

    /// A chat block landed: the change conversations it wrote to are read
    /// again (`None`: every one), on the same terms as [`Self::reread`].
    pub(crate) fn reread_conversations(&mut self, change: Option<&Change>, cx: &mut Context<Self>) {
        let (host, viewer) = (cx.host(), self.viewer());
        let touched = |channel: &String| {
            change.is_none_or(|change| {
                change.touches::<chat::Chat, _>(&chat::ask::Roots {
                    channel_id: channel.clone(),
                    viewer: viewer.clone(),
                    page: PageRequest::default(),
                })
            })
        };
        // named before any is asked, as in `reread`
        let again: Vec<(String, bool)> = self
            .messages
            .iter()
            .filter_map(|(channel, slot)| match slot {
                Loadable::Idle | Loadable::Loading(_) => None,
                Loadable::Failed(_) => Some((channel.clone(), true)),
                Loadable::Ready(_) | Loadable::Reloading(..) => {
                    touched(channel).then(|| (channel.clone(), false))
                }
            })
            .collect();
        for (channel, refused) in again {
            let rows = queries::conversation(host.clone(), channel.clone(), viewer.clone());
            let landing = channel.clone();
            if refused {
                let task = cx.land(rows, move |forge, result, cx| {
                    if replaces_refusal(forge.messages.get_mut(&landing), result) {
                        cx.notify();
                    }
                });
                self.rereading_messages.insert(channel, task);
            } else {
                cx.load(self, rows, move |forge| {
                    forge.messages.entry(landing.clone()).or_default()
                });
            }
        }
    }

    /// An identity block landed: the names are read again.
    pub(crate) fn reread_names(&mut self, cx: &mut Context<Self>) {
        cx.load(self, queries::roster(cx.host()), |forge| &mut forge.names);
    }

    /// Retry one read the reader asked to retry.
    pub(crate) fn retry(&mut self, query: Query, cx: &mut Context<Self>) {
        self.rereading.remove(&query);
        self.data.remove(&query);
        self.logs.remove(&query);
        self.read(query, cx);
        cx.notify();
    }

    /// Issue what this screen needs and drop what it does not.
    pub(crate) fn sync(&mut self, cx: &mut Context<Self>) {
        self.land_goto();
        let needed = self.needed();
        let keys: BTreeSet<&Query> = needed.iter().collect();
        self.data.retain(|query, _| keys.contains(query));
        self.logs.retain(|query, _| keys.contains(query));
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
            cx.load(self, conversation, move |forge| {
                forge.messages.entry(landing.clone()).or_default()
            });
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
                let open = self.nav.commit.as_deref();
                std::iter::once(self.log_query())
                    .chain(open.into_iter().flat_map(|oid| self.commit_reads(oid)))
                    .collect()
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

    /// The reads the open commit's page needs beside the log: its own row
    /// when the log on screen does not hold it (a restored view holds the
    /// first page again), and its diff once the commit, and so its parent,
    /// is known.
    fn commit_reads(&self, oid: &str) -> Vec<Query> {
        let mut wanted = Vec::new();
        let held = self.logs.get(&self.log_query()).is_none_or(|(log, _)| {
            log.read(|log| log.is_loading() || log.rows().iter().any(|commit| commit.oid == oid))
        });
        if !held {
            wanted.push(self.commit_query(oid));
        }
        wanted.extend(self.commit_diff_query(oid));
        wanted
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
