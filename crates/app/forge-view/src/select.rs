//! What the screens read out of the view: the replies that have landed,
//! picked by the query that asked them, and the reader's own identity.
use ducktape_view_guest::Loadable;
use ducktape_view_guest::host::Error;

use crate::queries::PAGE;
use crate::state::{self, ChangeTab, Forge, Nav, change_key};
use forge::Principal;
use forge::{
    Bounds, Change, Comparison, PageResponse, Query, RefInfo, Reply, RepoInfo, Review, Revision,
};

/// The open change, as its screens read it: the record, its two current
/// endpoints (either can be gone) and the reviews landed so far.
pub(crate) type OpenChange<'a> = (
    &'a Change,
    &'a Option<String>,
    &'a Option<String>,
    &'a PageResponse<Review>,
);

/// A read, in the three states a screen draws.
pub(crate) enum Stage<'a> {
    Loading,
    Failed(&'a Error),
    Ready(&'a Reply),
}

impl Forge {
    /// The reader's account number, as the host resolved it.
    pub(crate) fn my_account(&self) -> Option<u64> {
        self.session.account
    }

    /// The reader as a principal: their account; nobody while their seated key
    /// holds none. [`Principal::writer`] is the one rule every view gates its
    /// writes on, as `ExecCtx::sender` refuses them.
    pub(crate) fn me_principal(&self) -> Option<Principal> {
        Principal::writer(self.my_account())
    }

    /// Whether the reader writes at all: connected, and an account to write
    /// as. A key that holds none reads everything and writes nothing.
    pub(crate) fn may_write(&self) -> bool {
        self.session.connected && self.me_principal().is_some()
    }

    /// Whether the reader may push, merge and close in the open repository:
    /// its owner or a writer, as forge's `require_writer` checks.
    // ponytail: reads the writers' first page only; a reader granted past it
    // sees Merge and Close off until the page walks further
    pub(crate) fn writes_repo(&self) -> bool {
        let (Some(me), Some((info, _, writers))) = (self.me_principal(), self.repo()) else {
            return false;
        };
        self.may_write() && (info.repo.owner == me || writers.items.contains(&me))
    }

    /// Whether the reader owns the open repository: only the owner changes
    /// its settings and writers.
    pub(crate) fn owns_repo(&self) -> bool {
        let (Some(me), Some((info, _, _))) = (self.me_principal(), self.repo()) else {
            return false;
        };
        self.may_write() && info.repo.owner == me
    }

    pub(crate) fn stage(&self, query: &Query) -> Stage<'_> {
        match self.data.get(query) {
            Some(Loadable::Ready(reply) | Loadable::Reloading(reply, _)) => Stage::Ready(reply),
            Some(Loadable::Failed(refusal)) => Stage::Failed(refusal),
            _ => Stage::Loading,
        }
    }

    pub(crate) fn ready(&self, query: &Query) -> Option<&Reply> {
        match self.stage(query) {
            Stage::Ready(reply) => Some(reply),
            _ => None,
        }
    }

    pub(crate) fn repo_name(&self) -> String {
        self.nav.repo.clone().unwrap_or_default()
    }

    pub(crate) fn repo_query(&self) -> Query {
        Query::Repo {
            repo: self.repo_name(),
            page: PAGE,
        }
    }

    pub(crate) fn refs_query(&self) -> Query {
        Query::Refs {
            repo: self.repo_name(),
            page: PAGE,
        }
    }

    pub(crate) fn repo(&self) -> Option<(&RepoInfo, &Bounds, &PageResponse<Principal>)> {
        match self.ready(&self.repo_query())? {
            Reply::Repo {
                repo,
                bounds,
                writers,
                ..
            } => Some((repo, bounds, writers)),
            _ => None,
        }
    }

    pub(crate) fn refs(&self) -> Option<&[RefInfo]> {
        match self.ready(&self.refs_query())? {
            Reply::Refs { page, .. } => Some(&page.items),
            _ => None,
        }
    }

    pub(crate) fn branches(&self) -> Vec<Vec<u8>> {
        self.refs()
            .unwrap_or_default()
            .iter()
            .filter(|info| info.name.starts_with(b"refs/heads/"))
            .map(|info| info.name.clone())
            .collect()
    }

    /// The default head of the repo, or `refs/heads/main` before it lands.
    pub(crate) fn default_head(&self) -> Vec<u8> {
        self.repo()
            .map(|(info, _, _)| info.repo.settings.head.clone())
            .unwrap_or_else(|| b"refs/heads/main".to_vec())
    }

    /// The ref the reader is browsing.
    pub(crate) fn head_name(&self) -> Vec<u8> {
        self.nav.rev.clone().unwrap_or_else(|| self.default_head())
    }

    pub(crate) fn revision(&self) -> Revision {
        self.nav.revision(&self.default_head())
    }

    /// The commit the browsed ref points at, once refs have landed.
    pub(crate) fn head_oid(&self) -> Option<String> {
        let name = self.head_name();
        self.refs()?
            .iter()
            .find(|info| info.name == name)
            .map(|info| info.target.clone())
    }

    /// The README of the root tree: `README.md` over any other `README*`.
    pub(crate) fn readme(&self) -> Option<(Vec<u8>, String)> {
        let Reply::Tree { page, .. } = self.ready(&self.tree_query(Vec::new())?)? else {
            return None;
        };
        page.items
            .iter()
            .filter(|entry| {
                entry.kind != forge::EntryKind::Directory
                    && String::from_utf8_lossy(&entry.name)
                        .to_lowercase()
                        .starts_with("readme")
            })
            .min_by_key(|entry| !entry.name.eq_ignore_ascii_case(b"readme.md"))
            .map(|entry| (entry.name.clone(), entry.oid.clone()))
    }

    pub(crate) fn commit_parent(&self, oid: &str) -> Option<String> {
        let log = self.ready(&Query::Log {
            repo: self.repo_name(),
            from: self.revision(),
            exclude: None,
            page: PAGE,
        })?;
        let Reply::Log { page, .. } = log else {
            return None;
        };
        page.items
            .iter()
            .find(|commit| commit.oid == oid)?
            .parents
            .first()
            .cloned()
    }

    /// How many changes are open here, once the Changes tab has read them
    /// all (a list with more pages to come counts none).
    pub(crate) fn open_changes(&self) -> Option<u64> {
        let query = Query::Changes {
            repo: self.nav.repo.clone()?,
            filter: forge::ChangeFilter {
                state: Some(forge::ChangeState::Open),
                ..forge::ChangeFilter::default()
            },
            page: PAGE,
        };
        match self.ready(&query)? {
            Reply::Changes { page, .. } if page.next.is_none() => Some(page.items.len() as u64),
            _ => None,
        }
    }

    pub(crate) fn change_query(&self) -> Option<Query> {
        Some(Query::Change {
            repo: self.nav.repo.clone()?,
            n: self.nav.change?,
            page: PAGE,
        })
    }

    pub(crate) fn change(&self) -> Option<OpenChange<'_>> {
        match self.ready(&self.change_query()?)? {
            Reply::Change {
                change,
                source_head,
                target_head,
                reviews,
                ..
            } => Some((change, source_head, target_head, reviews)),
            _ => None,
        }
    }

    /// What the open change compares: its source and target, or, once
    /// merged, the two heads the merge joined (the refs move on after).
    pub(crate) fn endpoints(&self) -> Option<(Revision, Revision)> {
        let (change, _, _, _) = self.change()?;
        Some(match &change.merged_heads {
            Some(heads) => (
                Revision::Oid(heads.source.clone()),
                Revision::Oid(heads.target.clone()),
            ),
            None => (change.from.clone(), Revision::Ref(change.into.clone())),
        })
    }

    pub(crate) fn compare_query(&self) -> Option<Query> {
        let (from, into) = self.endpoints()?;
        Some(Query::Compare {
            repo: self.nav.repo.clone()?,
            from,
            into,
        })
    }

    pub(crate) fn compare(&self) -> Option<&Comparison> {
        match self.ready(&self.compare_query()?)? {
            Reply::Compare { comparison, .. } => Some(comparison),
            _ => None,
        }
    }

    pub(crate) fn diff_query(&self) -> Option<Query> {
        let (_, source_head, _, _) = self.change()?;
        Some(Query::Diff {
            repo: self.nav.repo.clone()?,
            base: self.compare()?.base.clone(),
            head: source_head.clone()?,
            path: None,
            page: PAGE,
        })
    }

    /// The principals chat marks as the reader's own: at most one.
    pub(crate) fn viewer(&self) -> Vec<Principal> {
        self.me_principal().into_iter().collect()
    }

    /// What a person or chat author is called: their account name once
    /// the roster has landed.
    pub(crate) fn principal_name(&self, principal: &Principal) -> String {
        match self.names.ready() {
            Some(names) => names.member(principal),
            None => chat::view::unnamed(principal),
        }
    }

    /// The hidden chat channel of the open change.
    pub(crate) fn open_channel(&self) -> Option<String> {
        let (change, _, _, _) = self.change()?;
        (self.nav.change_tab == ChangeTab::Conversation).then(|| change.channel.clone())
    }

    pub(crate) fn review(&self) -> Option<&state::ReviewSession> {
        self.reviews
            .get(&change_key(self.nav.repo.as_deref()?, self.nav.change?))
    }

    /// The open change's review being written, to edit.
    pub(crate) fn review_mut(&mut self) -> Option<&mut state::ReviewSession> {
        let key = change_key(self.nav.repo.as_deref()?, self.nav.change?);
        self.reviews.get_mut(&key)
    }

    pub(crate) fn pending_in(&self, scope: &str) -> Vec<&state::Pending> {
        self.pending.iter().filter(|op| op.scope == scope).collect()
    }

    pub(crate) fn nav(&self) -> &Nav {
        &self.nav
    }
}
