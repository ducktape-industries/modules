//! [`Relay`]: the node's live tracks for two or more views under test, each
//! in its own [`TestAppContext`], so what one publishes the others hear.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use program::live::Grant;

use crate::host::Error;
use crate::methods::{
    LiveAnnounce, LiveItem, LiveMode, LivePublish, LivePublishTrack, LiveSend, LiveSendObject,
    LiveStart, LiveSubscribe, LiveSubscribeTrack, LiveTrack, LiveTracks, Method, Publishing,
    Watching, refusal,
};
use crate::wire::{Event, Frame};

use super::{FakeHost, TestAppContext};

/// The node's relay for views under test: [`link`](Relay::link) each
/// view's [`TestAppContext`] and it answers their `live.*` requests as one
/// node would: a track a view publishes is announced to every
/// `live.tracks` of its room, an object one view sends reaches every view
/// subscribed to the track, the latest group is replayed to a late
/// subscriber, and a publisher hears its subscriber count. The n-th view
/// linked acts as account n; the views opened in one [`TestAppContext`]
/// are windows of that one app, so a track one of them publishes another
/// is refused `duplicate_track`, as the app refuses it.
///
/// Grants are rows, as the room's program writes them
/// (`program::live::grant`), decided as the node decides them
/// ([`Grant::publishes`], [`Grant::subscribes`]): an account's own row,
/// else the room's `*` row, else nothing. Every room starts as if its
/// program wrote `*` = [`Grant::all`]; [`hold`](Relay::hold) takes every
/// row of a room away, [`grant`](Relay::grant) and
/// [`revoke`](Relay::revoke) write and remove one, each the block that does
/// it. An ask its row does not cover hears `Waiting` and waits for a block
/// that covers it; a request a block no longer covers ends `not_granted`,
/// and a room list whose row moved hears the tracks it gains and loses.
///
/// An item lands in a view's host as the frame that caused it is taken,
/// and the view reads it on its next tick: `a.simulate_click("send");
/// b.run_until_parked();` and `b` has it. The order is the frames' order,
/// so a run is the same every time.
///
/// ```
/// # use view::testing::{Relay, TestAppContext};
/// let (a, b) = (TestAppContext::new(), TestAppContext::new());
/// let relay = Relay::new();
/// relay.link(&a).link(&b);
/// ```
#[derive(Clone, Default)]
pub struct Relay(Rc<RefCell<State>>);

#[derive(Default)]
struct State {
    hosts: Vec<FakeHost>,
    /// Every track up, by room and name.
    tracks: BTreeMap<(String, String), Published>,
    /// Every `live.subscribe` open: the track it reads, by link and id.
    subscriptions: BTreeMap<(usize, u64), (String, String)>,
    /// Every `live.tracks` open, by link and id: its room, and the grant it
    /// hears the room's tracks by (the row it was last aimed at).
    watches: BTreeMap<(usize, u64), (String, Grant)>,
    /// The grant rows of each room a test changed, by who (`None` the `*`
    /// row). A room not here holds `*` = [`Grant::all`].
    rows: BTreeMap<String, BTreeMap<Option<u64>, Grant>>,
    /// Every ask no row covers yet, in the order asked.
    waiting: Vec<Waiting>,
}

/// An ask no row covers yet, kept until a block's does, answered then as if
/// just asked.
struct Waiting {
    link: usize,
    id: u64,
    ask: Ask,
}

enum Ask {
    /// With the objects the view sent onto the track meanwhile, by their
    /// `live.send` ids: they go out once the track is up, as the app holds
    /// them.
    Publish(LivePublish, Vec<(u64, LiveSend)>),
    Subscribe(LiveSubscribe),
    Watch(String),
}

impl Ask {
    fn room(&self) -> &str {
        match self {
            Ask::Publish(publish, _) => &publish.track.room,
            Ask::Subscribe(subscribe) => &subscribe.track.room,
            Ask::Watch(room) => room,
        }
    }

    fn publishes(&self) -> bool {
        match self {
            Ask::Publish(..) => true,
            Ask::Subscribe(_) | Ask::Watch(_) => false,
        }
    }

    /// Whether `grant` lets `account` ask this: publish or subscribe to the
    /// track, or, for a room list, subscribe to anything in the room.
    fn granted(&self, grant: &Grant, account: u64) -> bool {
        match self {
            Ask::Publish(publish, _) => grant.publishes(&publish.track.name, account),
            Ask::Subscribe(subscribe) => grant.subscribes(&subscribe.track.name, account),
            Ask::Watch(_) => grant.watches(),
        }
    }

    /// The `Waiting` item that tells the view its ask waits.
    fn waits(&self, id: u64) -> Event {
        match self {
            Ask::Publish(..) => item::<LivePublishTrack>(id, Ok(Publishing::Waiting), false),
            Ask::Subscribe(_) => item::<LiveSubscribeTrack>(id, Ok(LiveItem::Waiting), false),
            Ask::Watch(_) => item::<LiveTracks>(id, Ok(Watching::Waiting), false),
        }
    }
}

struct Published {
    link: usize,
    request: u64,
    mode: LiveMode,
    /// The group in hand: its number, the objects sent in it so far, and
    /// whether it closed. What a late subscriber is replayed.
    group: Option<Group>,
    subscribers: Vec<Subscriber>,
}

impl Published {
    /// The track `name` coming up or going down, told to the room list `id`.
    fn announced(&self, id: u64, name: &str, up: bool) -> Event {
        let announce = LiveAnnounce {
            name: name.to_owned(),
            by: State::account(self.link),
            mode: self.mode,
            up,
        };
        item::<LiveTracks>(id, Ok(Watching::Announce(announce)), false)
    }
}

/// One `live.subscribe` of a track.
struct Subscriber {
    link: usize,
    id: u64,
    /// A [`LiveStart::Next`] subscription's stream group in flight when it
    /// subscribed: it hears none of that group, only the ones after. A
    /// datagram stands alone, so a datagram track passes nothing over.
    passed: Option<u64>,
}

impl Subscriber {
    fn hears(&self, group: u64) -> bool {
        self.passed.is_none_or(|passed| group > passed)
    }
}

struct Group {
    number: u64,
    objects: Vec<(u32, Vec<u8>)>,
    closed: bool,
}

const REVOKED: &str = "the room's program took the grant back";

/// The last item of a request its room's rows no longer cover.
fn revoked() -> Error {
    Error::new(refusal::NOT_GRANTED, REVOKED)
}

fn item<M: Method>(id: u64, reply: Result<M::Reply, Error>, done: bool) -> Event {
    Event::Response {
        id,
        result: reply.map(|reply| M::encode_reply(&reply)),
        done,
    }
}

impl Relay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Links `cx`'s view to the relay, as the account after the last one
    /// linked (the first is account 1).
    pub fn link(&self, cx: &TestAppContext) -> &Self {
        let host = cx.host();
        let link = {
            let mut state = self.0.borrow_mut();
            state.hosts.push(host.clone());
            state.hosts.len() - 1
        };
        let relay = self.clone();
        host.after_frame(move |_, frame| relay.take(link, frame));
        self
    }

    /// The block that takes every row of `room` away, as a room whose
    /// program has written none yet: each `live.publish`, `live.subscribe`
    /// and `live.tracks` asked in it hears `Waiting`, and waits for a
    /// [`grant`](Relay::grant) that covers it; what a view sends onto a
    /// waiting track goes out once the track is up. What the rows allowed
    /// before ends as [`revoke`](Relay::revoke) ends it.
    pub fn hold(&self, room: &str) -> &Self {
        self.0.borrow_mut().block(room, |rows| rows.clear());
        self
    }

    /// The block that writes `who`'s row in `room` (`None`: the `*` row),
    /// replacing the one it had. What the room's rows no longer cover ends
    /// as [`revoke`](Relay::revoke) ends it; then each ask waiting in the
    /// room that they now cover is answered as if just asked, the
    /// publications first, so a waiting subscription to one of them finds
    /// it up.
    pub fn grant(&self, room: &str, who: Option<u64>, grant: Grant) {
        self.0.borrow_mut().block(room, |rows| {
            rows.insert(who, grant);
        });
    }

    /// The block that removes `who`'s row in `room` (`None`: the `*` row), as
    /// `program::live::revoke`. What no row covers now ends with a final
    /// `not_granted`, in the node's order: publications (their subscribers
    /// hear `publisher_gone`, and each room list that heard the track hears
    /// it go down), then subscriptions (each publisher hears the lower
    /// count), then room lists. A room list the rows still let watch is
    /// re-aimed instead: it hears the tracks still up that its new row adds
    /// come up and the ones it drops go down, as a fresh `live.tracks` would
    /// list them. An ask waiting for `who` that the `*` row covers is
    /// answered.
    pub fn revoke(&self, room: &str, who: Option<u64>) {
        self.0.borrow_mut().block(room, |rows| {
            rows.remove(&who);
        });
    }

    /// The view linked as `cx` goes away with its tracks up, as an app
    /// that quit: each one ends `publisher_gone` for its subscribers and
    /// goes down in every `live.tracks`; each of its subscriptions is
    /// dropped, and the track's publisher hears the lower count.
    pub fn gone(&self, cx: &TestAppContext) {
        let host = cx.host();
        let mut state = self.0.borrow_mut();
        let Some(link) = state.hosts.iter().position(|linked| linked.same(&host)) else {
            panic!("this view is not linked to the relay");
        };
        let up: Vec<_> = state
            .tracks
            .iter()
            .filter(|(_, published)| published.link == link)
            .map(|(track, _)| track.clone())
            .collect();
        for track in up {
            state.end(&track, refusal::PUBLISHER_GONE, "the publisher went away");
        }
        let subscribed: Vec<u64> = state
            .subscriptions
            .keys()
            .filter(|(at, _)| *at == link)
            .map(|(_, id)| *id)
            .collect();
        for id in subscribed {
            state.unsubscribe(link, id);
        }
        state.watches.retain(|(at, _), _| *at != link);
        state.waiting.retain(|waiting| waiting.link != link);
    }

    /// What one frame of the view at `link` asked of the relay.
    fn take(&self, link: usize, frame: &Frame) {
        let mut state = self.0.borrow_mut();
        for id in &frame.cancels {
            state.cancel(link, *id);
        }
        for request in &frame.requests {
            let payload = &request.payload;
            match request.kind.as_str() {
                LivePublishTrack::KIND => {
                    if let Ok(publish) = LivePublishTrack::decode_request(payload) {
                        state.ask(link, request.id, Ask::Publish(publish, Vec::new()));
                    }
                }
                LiveSendObject::KIND => {
                    if let Ok(send) = LiveSendObject::decode_request(payload) {
                        state.send(link, request.id, send);
                    }
                }
                LiveSubscribeTrack::KIND => {
                    if let Ok(subscribe) = LiveSubscribeTrack::decode_request(payload) {
                        state.ask(link, request.id, Ask::Subscribe(subscribe));
                    }
                }
                LiveTracks::KIND => {
                    if let Ok(room) = LiveTracks::decode_request(payload) {
                        state.ask(link, request.id, Ask::Watch(room));
                    }
                }
                _ => {}
            }
        }
    }
}

impl State {
    fn tell(&self, link: usize, event: Event) {
        self.hosts[link].owe(vec![event]);
    }

    fn account(link: usize) -> u64 {
        link as u64 + 1
    }

    /// The grant `room`'s rows give `account`: its own row, else the `*`
    /// row.
    fn grant_of(&self, room: &str, account: u64) -> Option<Grant> {
        let Some(rows) = self.rows.get(room) else {
            return Some(Grant::all());
        };
        rows.get(&Some(account))
            .or_else(|| rows.get(&None))
            .cloned()
    }

    /// Whether `room`'s rows let `link` ask `ask`.
    fn covers(&self, link: usize, ask: &Ask) -> bool {
        let account = Self::account(link);
        self.grant_of(ask.room(), account)
            .is_some_and(|grant| ask.granted(&grant, account))
    }

    /// Whether `room`'s rows let `link` subscribe to the track `name`.
    fn subscribes(&self, link: usize, room: &str, name: &str) -> bool {
        let account = Self::account(link);
        self.grant_of(room, account)
            .is_some_and(|grant| grant.subscribes(name, account))
    }

    /// Whether `link` publishes `track` already, up or waiting.
    fn publishes(&self, link: usize, track: &LiveTrack) -> bool {
        let up = self
            .tracks
            .get(&(track.room.clone(), track.name.clone()))
            .is_some_and(|held| held.link == link);
        up || self.waiting.iter().any(|waiting| {
            waiting.link == link
                && matches!(&waiting.ask, Ask::Publish(publish, _) if publish.track == *track)
        })
    }

    /// `room`'s rows, made explicit: a room a test never changed holds
    /// `*` = [`Grant::all`].
    fn rows_of(&mut self, room: &str) -> &mut BTreeMap<Option<u64>, Grant> {
        self.rows
            .entry(room.to_owned())
            .or_insert_with(|| BTreeMap::from([(None, Grant::all())]))
    }

    /// Each room list of the track's room whose grant lets it hear the
    /// track hears it come up or go down.
    fn announce(&self, (room, name): &(String, String), published: &Published, up: bool) {
        for ((at, id), (watched, grant)) in &self.watches {
            if watched == room && grant.subscribes(name, Self::account(*at)) {
                self.tell(*at, published.announced(*id, name, up));
            }
        }
    }

    /// `link`'s room list `id`, aimed at `grant`: it hears the tracks up
    /// that `grant` adds come up and the ones it drops go down, by name, so
    /// it holds what a fresh `live.tracks` would.
    fn rewatch(&mut self, link: usize, id: u64, grant: Grant) {
        let Some((room, was)) = self.watches.get(&(link, id)).cloned() else {
            return;
        };
        let account = Self::account(link);
        for ((at, name), published) in &self.tracks {
            let now = grant.subscribes(name, account);
            if *at == room && was.subscribes(name, account) != now {
                self.tell(link, published.announced(id, name, now));
            }
        }
        self.watches.insert((link, id), (room, grant));
    }

    /// The publisher hears its subscriber count, which just moved.
    fn count(&self, published: &Published) {
        let count = Publishing::Subscribers(published.subscribers.len() as u32);
        self.tell(
            published.link,
            item::<LivePublishTrack>(published.request, Ok(count), false),
        );
    }

    /// An ask, kept waiting while no row of its room covers it. A second
    /// publication of a track this app publishes, up or waiting, is refused
    /// first, as the app refuses it before the node sees it.
    fn ask(&mut self, link: usize, id: u64, ask: Ask) {
        if let Ask::Publish(publish, _) = &ask
            && self.publishes(link, &publish.track)
        {
            let refusal = Error::new(
                refusal::DUPLICATE_TRACK,
                "another window of this app publishes this track",
            );
            self.tell(link, item::<LivePublishTrack>(id, Err(refusal), true));
            return;
        }
        if !self.covers(link, &ask) {
            self.tell(link, ask.waits(id));
            self.waiting.push(Waiting { link, id, ask });
            return;
        }
        self.admit(link, id, ask);
    }

    fn admit(&mut self, link: usize, id: u64, ask: Ask) {
        match ask {
            Ask::Publish(publish, sent) => {
                self.publish(link, id, publish);
                for (send_id, send) in sent {
                    self.send(link, send_id, send);
                }
            }
            Ask::Subscribe(subscribe) => self.subscribe(link, id, subscribe),
            Ask::Watch(room) => self.watch(link, id, room),
        }
    }

    /// A block that `write`s `room`'s rows, as the node takes it: every
    /// open request in the room is rechecked, then every ask waiting in it
    /// is asked again, the publications first, so a waiting subscription to
    /// one of them finds it up.
    fn block(&mut self, room: &str, write: impl FnOnce(&mut BTreeMap<Option<u64>, Grant>)) {
        write(self.rows_of(room));
        self.recheck(room);
        let (in_room, elsewhere): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|waiting| waiting.ask.room() == room);
        self.waiting = elsewhere;
        let (publications, others): (Vec<_>, Vec<_>) = in_room
            .into_iter()
            .partition(|waiting| waiting.ask.publishes());
        for waiting in publications.into_iter().chain(others) {
            if !self.covers(waiting.link, &waiting.ask) {
                // waits still, and is not told again
                self.waiting.push(waiting);
                continue;
            }
            self.admit(waiting.link, waiting.id, waiting.ask);
        }
    }

    /// Every open request in `room` its rows no longer cover ends with a
    /// final `not_granted`, in the node's order: publications first (their
    /// subscribers, whose own grant this is not, hear `publisher_gone`, and
    /// the tracks go down in each room list under the grant it holds), then
    /// subscriptions, then room lists; a room list the rows still let watch
    /// is re-aimed at its new row over the tracks still up.
    fn recheck(&mut self, room: &str) {
        let unpublished: Vec<(String, String)> = self
            .tracks
            .iter()
            .filter(|((at, name), published)| {
                let account = Self::account(published.link);
                let publishes = self
                    .grant_of(at, account)
                    .is_some_and(|grant| grant.publishes(name, account));
                at == room && !publishes
            })
            .map(|(track, _)| track.clone())
            .collect();
        for track in unpublished {
            let gone = "the publisher's grant was taken back";
            let Some(published) = self.end(&track, refusal::PUBLISHER_GONE, gone) else {
                continue;
            };
            let ended = item::<LivePublishTrack>(published.request, Err(revoked()), true);
            self.tell(published.link, ended);
        }
        let unsubscribed: Vec<(usize, u64)> = self
            .subscriptions
            .iter()
            .filter(|((link, _), (at, name))| at == room && !self.subscribes(*link, at, name))
            .map(|(subscription, _)| *subscription)
            .collect();
        for (link, id) in unsubscribed {
            self.unsubscribe(link, id);
            self.tell(link, item::<LiveSubscribeTrack>(id, Err(revoked()), true));
        }
        let watching: Vec<(usize, u64)> = self
            .watches
            .iter()
            .filter(|(_, (watched, _))| watched == room)
            .map(|(watch, _)| *watch)
            .collect();
        for (link, id) in watching {
            match self
                .grant_of(room, Self::account(link))
                .filter(Grant::watches)
            {
                Some(grant) => self.rewatch(link, id, grant),
                None => {
                    self.watches.remove(&(link, id));
                    self.tell(link, item::<LiveTracks>(id, Err(revoked()), true));
                }
            }
        }
    }

    /// The objects held for `link`'s waiting publication of `track`.
    fn held_objects(
        &mut self,
        link: usize,
        track: &LiveTrack,
    ) -> Option<&mut Vec<(u64, LiveSend)>> {
        self.waiting
            .iter_mut()
            .find_map(|waiting| match &mut waiting.ask {
                Ask::Publish(publish, sent) if waiting.link == link && publish.track == *track => {
                    Some(sent)
                }
                Ask::Publish(..) | Ask::Subscribe(_) | Ask::Watch(_) => None,
            })
    }

    fn publish(&mut self, link: usize, id: u64, publish: LivePublish) {
        let track = (publish.track.room, publish.track.name);
        // this app's own second publication `ask` refused already
        if self.tracks.contains_key(&track) {
            let refusal = Error::new(refusal::DUPLICATE_TRACK, "another account publishes it");
            self.tell(link, item::<LivePublishTrack>(id, Err(refusal), true));
            return;
        }
        let published = Published {
            link,
            request: id,
            mode: publish.mode,
            group: None,
            subscribers: Vec::new(),
        };
        // no count yet: the publisher hears one when a subscriber moves it
        self.announce(&track, &published, true);
        self.tracks.insert(track, published);
    }

    /// The track ends with `code` for each subscriber and goes down in
    /// every `live.tracks`; its publication, if it was up.
    fn end(&mut self, track: &(String, String), code: &str, message: &str) -> Option<Published> {
        let published = self.tracks.remove(track)?;
        for subscriber in &published.subscribers {
            self.subscriptions.remove(&(subscriber.link, subscriber.id));
            let refusal = Error::new(code, message);
            let ended = item::<LiveSubscribeTrack>(subscriber.id, Err(refusal), true);
            self.tell(subscriber.link, ended);
        }
        self.announce(track, &published, false);
        Some(published)
    }

    fn cancel(&mut self, link: usize, id: u64) {
        let publication = self
            .tracks
            .iter()
            .find(|(_, published)| published.link == link && published.request == id)
            .map(|(track, _)| track.clone());
        if let Some(track) = publication {
            self.end(
                &track,
                refusal::TRACK_ENDED,
                "the publisher ended the track",
            );
        }
        self.unsubscribe(link, id);
        self.watches.remove(&(link, id));
        self.waiting
            .retain(|waiting| (waiting.link, waiting.id) != (link, id));
    }

    /// `link`'s subscription `id`, if it has one, is dropped from its track,
    /// and the track's publisher hears the lower count.
    fn unsubscribe(&mut self, link: usize, id: u64) {
        let Some(track) = self.subscriptions.remove(&(link, id)) else {
            return;
        };
        let Some(published) = self.tracks.get_mut(&track) else {
            return;
        };
        published
            .subscribers
            .retain(|subscriber| (subscriber.link, subscriber.id) != (link, id));
        self.count(&self.tracks[&track]);
    }

    fn send(&mut self, link: usize, id: u64, send: LiveSend) {
        if let Some(held) = self.held_objects(link, &send.track) {
            held.push((id, send));
            return;
        }
        let track = (send.track.room, send.track.name);
        let Some(published) = self.tracks.get_mut(&track).filter(|held| held.link == link) else {
            let refusal = Error::new(refusal::LIVE_NOT_PUBLISHED, "no such track is published");
            self.tell(link, item::<LiveSendObject>(id, Err(refusal), true));
            return;
        };
        // each item with the group it belongs to
        let mut items = Vec::new();
        match published.mode {
            LiveMode::Datagram => {
                let newest = published
                    .group
                    .as_ref()
                    .and_then(|group| Some((group.number, group.objects.last()?.0)));
                // an object older than the newest is dropped, as the node drops it
                if newest.is_some_and(|newest| (send.group, send.object) <= newest) {
                    return;
                }
                published.group = Some(Group {
                    number: send.group,
                    objects: vec![(send.object, send.body.clone())],
                    closed: false,
                });
                let object = LiveItem::Object {
                    group: send.group,
                    object: send.object,
                    body: send.body,
                };
                items.push((send.group, object));
            }
            LiveMode::Stream => {
                let open = published
                    .group
                    .as_ref()
                    .is_some_and(|group| group.number == send.group && !group.closed);
                if !open {
                    // a newer group closes the one before it
                    if let Some(group) = published.group.as_ref().filter(|group| !group.closed) {
                        let end = LiveItem::GroupEnd {
                            group: group.number,
                            complete: true,
                        };
                        items.push((group.number, end));
                    }
                    published.group = Some(Group {
                        number: send.group,
                        objects: Vec::new(),
                        closed: false,
                    });
                }
                let group = published.group.as_mut().expect("the group in hand");
                if !send.body.is_empty() || !send.end {
                    let object = group.objects.len() as u32;
                    group.objects.push((object, send.body.clone()));
                    let object = LiveItem::Object {
                        group: send.group,
                        object,
                        body: send.body,
                    };
                    items.push((send.group, object));
                }
                if send.end {
                    group.closed = true;
                    let end = LiveItem::GroupEnd {
                        group: send.group,
                        complete: true,
                    };
                    items.push((send.group, end));
                }
            }
        }
        let mut told = Vec::new();
        for subscriber in &published.subscribers {
            for (group, live) in &items {
                if !subscriber.hears(*group) {
                    continue;
                }
                let event = item::<LiveSubscribeTrack>(subscriber.id, Ok(live.clone()), false);
                told.push((subscriber.link, event));
            }
        }
        for (at, event) in told {
            self.tell(at, event);
        }
    }

    fn subscribe(&mut self, link: usize, id: u64, subscribe: LiveSubscribe) {
        let track = (subscribe.track.room, subscribe.track.name);
        let Some(published) = self.tracks.get_mut(&track) else {
            let refusal = Error::new(refusal::UNKNOWN_TRACK, "no one publishes this track");
            self.tell(link, item::<LiveSubscribeTrack>(id, Err(refusal), true));
            return;
        };
        let in_hand = published.group.as_ref();
        let in_flight = in_hand
            .filter(|group| published.mode == LiveMode::Stream && !group.closed)
            .map(|group| group.number);
        let (replay, passed) = match subscribe.from {
            LiveStart::Latest => (in_hand, None),
            LiveStart::Next => (None, in_flight),
        };
        published.subscribers.push(Subscriber { link, id, passed });
        let mut items = vec![LiveItem::Opened {
            by: Self::account(published.link),
            mode: published.mode,
            latest: replay.map(|group| group.number),
        }];
        if let Some(group) = replay {
            items.extend(group.objects.iter().map(|(object, body)| LiveItem::Object {
                group: group.number,
                object: *object,
                body: body.clone(),
            }));
            if group.closed {
                items.push(LiveItem::GroupEnd {
                    group: group.number,
                    complete: true,
                });
            }
        }
        for live in items {
            self.tell(link, item::<LiveSubscribeTrack>(id, Ok(live), false));
        }
        self.subscriptions.insert((link, id), track.clone());
        self.count(&self.tracks[&track]);
    }

    fn watch(&mut self, link: usize, id: u64, room: String) {
        let account = Self::account(link);
        let grant = self
            .grant_of(&room, account)
            .expect("an admitted room list has a row");
        for ((at, name), published) in &self.tracks {
            if *at == room && grant.subscribes(name, account) {
                self.tell(link, published.announced(id, name, true));
            }
        }
        self.watches.insert((link, id), (room, grant));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{DEFAULT_PRIORITY, Received, Track, subscribe};
    use crate::methods::{Capability, LiveTrack};
    use crate::prelude::*;
    use crate::{Task, View};
    use serde::{Deserialize, Serialize};

    /// The kinds the relay answers, held to [`crate::methods::ALL`].
    const KINDS: [&str; 4] = [
        LivePublishTrack::KIND,
        LiveSendObject::KIND,
        LiveSubscribeTrack::KIND,
        LiveTracks::KIND,
    ];

    #[test]
    fn the_relay_answers_every_live_kind() {
        let live: Vec<&str> = crate::methods::ALL
            .iter()
            .copied()
            .filter(|kind| kind.starts_with("live."))
            .collect();
        assert_eq!(live, KINDS);
    }

    fn track(name: &str) -> LiveTrack {
        LiveTrack {
            room: "lobby".into(),
            name: name.into(),
        }
    }

    fn latest(name: &str) -> LiveSubscribe {
        LiveSubscribe {
            track: track(name),
            from: LiveStart::Latest,
        }
    }

    /// One line per person: publishes `echo/<me>` in stream mode, one
    /// group per line, and hears every `echo/*` `live.tracks` announces.
    #[derive(Default, Serialize, Deserialize)]
    struct Echo {
        me: u64,
        heard: Vec<String>,
        roster: Vec<(String, u64, bool)>,
        subscribers: Option<u32>,
        /// `Waiting` items, of any of its asks.
        waits: u32,
        refused: Vec<String>,
        #[serde(skip)]
        mine: Option<Track<String>>,
        #[serde(skip)]
        hearing: BTreeMap<String, Task<()>>,
        #[serde(skip)]
        watch: Option<Task<()>>,
    }

    impl View for Echo {
        const NAME: &'static str = "Echo";
        const CAPABILITIES: &'static [Capability] = &[Capability::Live];
    }

    impl Render for Echo {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().children(self.heard.iter().map(|line| div().child(line.clone())))
        }
    }

    impl Echo {
        fn start(&mut self, me: u64, cx: &mut Context<Self>) {
            self.me = me;
            cx.notify();
            let mine = LivePublish {
                track: track(&format!("echo/{me}")),
                mode: LiveMode::Stream,
                priority: DEFAULT_PRIORITY,
            };
            self.mine = Some(Track::publish(cx, mine, |view: &mut Echo, item, cx| {
                match item {
                    Ok(Publishing::Waiting) => view.waits += 1,
                    Ok(Publishing::Subscribers(count)) => view.subscribers = Some(count),
                    Err(refusal) => view.refused.push(refusal.code),
                }
                cx.notify();
            }));
            self.watch = Some(cx.follow::<LiveTracks>("lobby".into(), |view, item, cx| {
                match item {
                    Ok(Watching::Waiting) => view.waits += 1,
                    Ok(Watching::Announce(LiveAnnounce { name, by, up, .. })) => {
                        view.roster.push((name.clone(), by, up));
                        if up && by != view.me {
                            view.hear(&name, cx);
                        }
                    }
                    Err(refusal) => view.refused.push(refusal.code),
                }
                cx.notify();
            }));
        }

        /// Subscribes to `name`: its lines into `heard`, its refusals into
        /// `refused`.
        fn hear(&mut self, name: &str, cx: &mut Context<Self>) {
            let task = subscribe(cx, latest(name), |view: &mut Echo, item, cx| {
                match item {
                    Ok(Received::Object { value, .. }) => view.heard.push(value),
                    Ok(Received::Waiting) => view.waits += 1,
                    Ok(Received::Opened { .. } | Received::GroupEnd { .. }) => return,
                    Err(refusal) => view.refused.push(refusal.code),
                }
                cx.notify();
            });
            self.hearing.insert(name.to_owned(), task);
        }

        fn say(&mut self, line: &str, cx: &mut Context<Self>) {
            let mine = self.mine.as_mut().unwrap();
            mine.send(&cx.host(), &line.to_owned());
            mine.next_group(&cx.host());
        }
    }

    fn echo(cx: &mut TestAppContext, me: u64) -> crate::Entity<Echo> {
        let view = cx.open::<Echo>();
        cx.update(&view, |echo, _, cx| echo.start(me, cx));
        cx.run_until_parked();
        view
    }

    #[test]
    fn a_line_one_view_sends_reaches_the_other_and_a_late_one_hears_the_last() {
        let (mut a, mut b, mut c) = (
            TestAppContext::new(),
            TestAppContext::new(),
            TestAppContext::new(),
        );
        let relay = Relay::new();
        relay.link(&a).link(&b).link(&c);
        let one = echo(&mut a, 1);
        let two = echo(&mut b, 2);
        a.run_until_parked();
        // each hears the other's track come up, and subscribes
        assert!(a.update(&one, |echo, _, _| echo.roster.contains(&(
            "echo/2".into(),
            2,
            true
        ))));
        a.update(&one, |echo, _, cx| echo.say("hello", cx));
        a.run_until_parked();
        b.run_until_parked();
        assert!(b.has_text("hello"), "{:?}", b.texts());
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(1));
        // a third, later: the group in hand is replayed to it
        let _three = echo(&mut c, 3);
        assert!(c.has_text("hello"), "{:?}", c.texts());
        a.run_until_parked();
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(2));
        b.update(&two, |echo, _, cx| echo.say("hi", cx));
        b.run_until_parked();
        a.run_until_parked();
        assert!(a.has_text("hi"), "{:?}", a.texts());
    }

    #[test]
    fn a_publisher_hears_its_count_only_as_it_moves_and_a_subscriber_gone_lowers_it() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = echo(&mut a, 1);
        // up, and no one listens: no count, as the node tells none
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), None);
        let _two = echo(&mut b, 2);
        a.run_until_parked();
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(1));
        relay.gone(&b);
        a.run_until_parked();
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(0));
    }

    #[test]
    fn a_publisher_gone_ends_each_subscription_and_takes_its_track_down() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let _one = echo(&mut a, 1);
        let two = echo(&mut b, 2);
        a.run_until_parked();
        relay.gone(&a);
        b.run_until_parked();
        let (refused, roster) = b.update(&two, |echo, _, _| {
            (echo.refused.clone(), echo.roster.clone())
        });
        assert_eq!(refused, [refusal::PUBLISHER_GONE]);
        assert!(roster.contains(&("echo/1".into(), 1, false)), "{roster:?}");
    }

    #[test]
    fn a_track_another_account_holds_is_refused_and_an_unknown_one_too() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let _one = echo(&mut a, 1);
        // b claims a's name
        let two = echo(&mut b, 1);
        b.run_until_parked();
        let refused = b.update(&two, |echo, _, _| echo.refused.clone());
        assert_eq!(refused, [refusal::DUPLICATE_TRACK]);
        b.update(&two, |echo, _, cx| echo.hear("nobody", cx));
        b.run_until_parked();
        let refused = b.update(&two, |echo, _, _| echo.refused.clone());
        assert_eq!(refused, [refusal::DUPLICATE_TRACK, refusal::UNKNOWN_TRACK]);
    }

    #[test]
    fn an_ask_in_a_held_room_waits_and_its_grant_answers_it() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.hold("lobby").link(&a).link(&b);
        // b asks for a's track before a publishes it, and for one no one will
        let two = b.open::<Echo>();
        b.update(&two, |echo, _, cx| {
            echo.hear("echo/1", cx);
            echo.hear("nobody", cx);
        });
        b.run_until_parked();
        let one = echo(&mut a, 1);
        // a line said while the track waits goes out with it
        a.update(&one, |echo, _, cx| echo.say("early", cx));
        a.run_until_parked();
        // a's publication and room list each hear `Waiting`, nothing refused
        let waited = a.update(&one, |echo, _, _| {
            (
                echo.waits,
                echo.refused.clone(),
                echo.subscribers,
                echo.roster.clone(),
            )
        });
        assert_eq!(waited, (2, vec![], None, vec![]));
        let waited = b.update(&two, |echo, _, _| (echo.waits, echo.refused.clone()));
        assert_eq!(waited, (2, vec![]));
        relay.grant("lobby", None, Grant::all());
        a.run_until_parked();
        b.run_until_parked();
        // a's track came up first, so b's subscription found it and heard
        // the line; the one to no one's track ended `unknown_track`
        assert!(b.has_text("early"), "{:?}", b.texts());
        assert_eq!(
            b.update(&two, |echo, _, _| echo.refused.clone()),
            [refusal::UNKNOWN_TRACK]
        );
        // a's publication and room list went on past their `Waiting`
        let admitted = a.update(&one, |echo, _, _| (echo.subscribers, echo.roster.clone()));
        assert_eq!(admitted, (Some(1), vec![("echo/1".into(), 1, true)]));
    }

    fn listens() -> Grant {
        Grant {
            publish: Vec::new(),
            subscribe: vec!["*".into()],
        }
    }

    #[test]
    fn an_accounts_own_row_replaces_the_star_row_and_account_names_the_asker() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        // each publishes under its own number and hears all; account 2 only listens
        let open = Grant {
            publish: vec!["echo/{account}".into()],
            subscribe: vec!["*".into()],
        };
        relay.grant("lobby", None, open);
        relay.grant("lobby", Some(2), listens());
        let one = echo(&mut a, 1);
        let two = echo(&mut b, 2);
        a.run_until_parked();
        // 2's own row is the whole of what it may do: its track waits, its
        // room list does not
        let heard = b.update(&two, |echo, _, _| (echo.waits, echo.roster.clone()));
        assert_eq!(heard, (1, vec![("echo/1".into(), 1, true)]));
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(1));
        // `{account}` is the asker's number: 1 may not publish 2's name, and
        // the ask waits rather than being refused
        let theirs = LivePublish {
            track: track("echo/2"),
            mode: LiveMode::Stream,
            priority: DEFAULT_PRIORITY,
        };
        let _theirs = a.update(&one, |_, _, cx| {
            Track::<String>::publish(cx, theirs, |echo: &mut Echo, item, cx| {
                if let Ok(Publishing::Waiting) = item {
                    echo.waits += 1;
                }
                cx.notify();
            })
        });
        a.run_until_parked();
        let asked = a.update(&one, |echo, _, _| (echo.waits, echo.refused.clone()));
        assert_eq!(asked, (1, vec![]));
    }

    #[test]
    fn a_row_taken_back_ends_what_it_allowed_with_a_final_not_granted() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = echo(&mut a, 1);
        let two = echo(&mut b, 2);
        a.run_until_parked();
        // 1 may listen and no longer speak: its own row replaces the `*` row
        relay.grant("lobby", Some(1), listens());
        a.run_until_parked();
        b.run_until_parked();
        // 1's publication ended `not_granted`, and 2's subscription to it
        // `publisher_gone`: 2's own grant is intact; 2's room list heard it
        // go down
        assert_eq!(
            a.update(&one, |echo, _, _| echo.refused.clone()),
            [refusal::NOT_GRANTED]
        );
        let (refused, roster) = b.update(&two, |echo, _, _| {
            (echo.refused.clone(), echo.roster.clone())
        });
        assert_eq!(refused, [refusal::PUBLISHER_GONE]);
        assert!(roster.contains(&("echo/1".into(), 1, false)), "{roster:?}");
        // and 1 still hears 2
        b.update(&two, |echo, _, cx| echo.say("still here", cx));
        b.run_until_parked();
        a.run_until_parked();
        assert!(a.has_text("still here"), "{:?}", a.texts());
        // the `*` row taken back: 2 has no row, so its publication and its
        // room list end, and 1's subscription to 2 with them
        relay.revoke("lobby", None);
        a.run_until_parked();
        b.run_until_parked();
        assert_eq!(
            b.update(&two, |echo, _, _| echo.refused.clone()),
            [
                refusal::PUBLISHER_GONE,
                refusal::NOT_GRANTED,
                refusal::NOT_GRANTED
            ]
        );
        let (refused, roster) = a.update(&one, |echo, _, _| {
            (echo.refused.clone(), echo.roster.clone())
        });
        assert_eq!(refused, [refusal::NOT_GRANTED, refusal::PUBLISHER_GONE]);
        assert!(roster.contains(&("echo/2".into(), 2, false)), "{roster:?}");
    }

    #[test]
    fn an_own_row_removed_leaves_the_star_row_to_answer_what_waited() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        // 2 may do nothing in the room, the `*` row everything
        relay.grant("lobby", Some(2), Grant::default());
        let one = echo(&mut a, 1);
        let two = echo(&mut b, 2);
        a.run_until_parked();
        assert_eq!(b.update(&two, |echo, _, _| echo.waits), 2);
        relay.revoke("lobby", Some(2));
        a.run_until_parked();
        b.run_until_parked();
        // b's subscription to 1 tells 1 its count
        a.run_until_parked();
        // the `*` row covers 2's asks now: its track is up, and each hears
        // the other
        let heard_two = a.update(&one, |echo, _, _| echo.roster.clone());
        assert!(
            heard_two.contains(&("echo/2".into(), 2, true)),
            "{heard_two:?}"
        );
        let heard_one = b.update(&two, |echo, _, _| echo.roster.clone());
        assert!(
            heard_one.contains(&("echo/1".into(), 1, true)),
            "{heard_one:?}"
        );
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(1));
        // 2 may publish and no longer listen: its subscription ends, and 1
        // hears the lower count
        let speaks = Grant {
            publish: vec!["*".into()],
            subscribe: Vec::new(),
        };
        relay.grant("lobby", Some(2), speaks);
        a.run_until_parked();
        b.run_until_parked();
        assert_eq!(a.update(&one, |echo, _, _| echo.subscribers), Some(0));
        assert_eq!(
            b.update(&two, |echo, _, _| echo.refused.clone()),
            [refusal::NOT_GRANTED; 2]
        );
    }

    #[test]
    fn a_waiting_ask_withdrawn_or_gone_is_forgotten() {
        let (mut a, mut b, mut c) = (
            TestAppContext::new(),
            TestAppContext::new(),
            TestAppContext::new(),
        );
        let relay = Relay::new();
        relay.hold("lobby").link(&a).link(&b).link(&c);
        let one = echo(&mut a, 1);
        let _two = echo(&mut b, 2);
        a.update(&one, |echo, _, _| echo.mine = None);
        a.run_until_parked();
        relay.gone(&b);
        relay.grant("lobby", None, Grant::all());
        let three = echo(&mut c, 3);
        let roster = c.update(&three, |echo, _, _| echo.roster.clone());
        assert_eq!(roster, [("echo/3".into(), 3, true)]);
    }

    #[test]
    fn a_track_asks_the_node_for_the_priority_it_names() {
        let mut a = TestAppContext::new();
        let one = a.open::<Echo>();
        let top = LivePublish {
            track: track("cursor/1"),
            mode: LiveMode::Datagram,
            priority: 0,
        };
        let _top = a.update(&one, |_, _, cx| {
            Track::<u32>::publish(cx, top, |_: &mut Echo, _, _| {})
        });
        a.run_until_parked();
        let asked = a.host().requests::<LivePublishTrack>();
        assert_eq!(
            asked.iter().map(|ask| ask.priority).collect::<Vec<_>>(),
            [0]
        );
    }

    #[test]
    fn a_next_subscription_passes_over_the_group_in_hand() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let mut doc: Option<Track<u32>> = None;
        let request = LivePublish {
            track: track("doc"),
            mode: LiveMode::Stream,
            priority: DEFAULT_PRIORITY,
        };
        a.update(&one, |_, _, cx| {
            doc = Some(Track::publish(cx, request, |_: &mut Echo, _, _| {}));
        });
        a.run_until_parked();
        let doc = doc.as_mut().unwrap();
        a.update(&one, |_, _, cx| doc.send(&cx.host(), &1));
        a.run_until_parked();
        let two = b.open::<Echo>();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let next = LiveSubscribe {
            track: track("doc"),
            from: LiveStart::Next,
        };
        let _task = b.update(&two, |_, _, cx| {
            subscribe(
                cx,
                next,
                move |_: &mut Echo, item: Result<Received<u32>, _>, _| {
                    sink.borrow_mut().push(item.unwrap());
                },
            )
        });
        b.run_until_parked();
        a.update(&one, |_, _, cx| {
            doc.send(&cx.host(), &2);
            doc.next_group(&cx.host());
            doc.send(&cx.host(), &3);
        });
        a.run_until_parked();
        b.run_until_parked();
        // the rest of group 0 and its end are passed over; group 1 is heard
        // from its first object
        assert_eq!(
            *heard.borrow(),
            [
                Received::Opened {
                    by: 1,
                    mode: LiveMode::Stream,
                    latest: None
                },
                Received::Object {
                    group: 1,
                    object: 0,
                    value: 3
                }
            ]
        );
    }

    #[test]
    fn a_datagram_track_keeps_the_newest_and_drops_an_older_object() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let mut cursor: Option<Track<u32>> = None;
        let request = LivePublish {
            track: track("cursor/1"),
            mode: LiveMode::Datagram,
            priority: DEFAULT_PRIORITY,
        };
        a.update(&one, |_, _, cx| {
            cursor = Some(Track::publish(cx, request, |_: &mut Echo, _, _| {}));
        });
        a.run_until_parked();
        let cursor = cursor.as_mut().unwrap();
        a.update(&one, |_, _, cx| {
            cursor.send_latest(&cx.host(), 5, &50);
            cursor.send_latest(&cx.host(), 5, &51);
        });
        a.run_until_parked();
        let two = b.open::<Echo>();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let _task = b.update(&two, |_, _, cx| {
            subscribe(
                cx,
                latest("cursor/1"),
                move |_: &mut Echo, item: Result<Received<u32>, _>, _| {
                    sink.borrow_mut().push(item.unwrap());
                },
            )
        });
        b.run_until_parked();
        // the newest object alone is replayed, its group named first
        assert_eq!(
            *heard.borrow(),
            [
                Received::Opened {
                    by: 1,
                    mode: LiveMode::Datagram,
                    latest: Some(5)
                },
                Received::Object {
                    group: 5,
                    object: 1,
                    value: 51
                }
            ]
        );
        a.update(&one, |_, _, cx| {
            cursor.send_latest(&cx.host(), 4, &40);
            cursor.send_latest(&cx.host(), 6, &60);
        });
        a.run_until_parked();
        b.run_until_parked();
        assert_eq!(
            heard.borrow()[2..],
            [Received::Object {
                group: 6,
                object: 0,
                value: 60
            }]
        );
    }

    #[test]
    fn a_next_datagram_subscription_hears_every_datagram_from_now_on() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let mut cursor: Option<Track<u32>> = None;
        let request = LivePublish {
            track: track("cursor/1"),
            mode: LiveMode::Datagram,
            priority: DEFAULT_PRIORITY,
        };
        a.update(&one, |_, _, cx| {
            cursor = Some(Track::publish(cx, request, |_: &mut Echo, _, _| {}));
        });
        a.run_until_parked();
        let cursor = cursor.as_mut().unwrap();
        a.update(&one, |_, _, cx| cursor.send_latest(&cx.host(), 0, &1));
        a.run_until_parked();
        let two = b.open::<Echo>();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let next = LiveSubscribe {
            track: track("cursor/1"),
            from: LiveStart::Next,
        };
        let _task = b.update(&two, |_, _, cx| {
            subscribe(
                cx,
                next,
                move |_: &mut Echo, item: Result<Received<u32>, _>, _| {
                    sink.borrow_mut().push(item.unwrap());
                },
            )
        });
        b.run_until_parked();
        // the publisher keeps group 0: a datagram stands alone, so each new
        // one is heard
        a.update(&one, |_, _, cx| {
            cursor.send_latest(&cx.host(), 0, &2);
            cursor.send_latest(&cx.host(), 0, &3);
        });
        a.run_until_parked();
        b.run_until_parked();
        assert_eq!(
            *heard.borrow(),
            [
                Received::Opened {
                    by: 1,
                    mode: LiveMode::Datagram,
                    latest: None
                },
                Received::Object {
                    group: 0,
                    object: 1,
                    value: 2
                },
                Received::Object {
                    group: 0,
                    object: 2,
                    value: 3
                }
            ]
        );
    }

    /// A room list that only records what it hears.
    fn watcher(cx: &mut TestAppContext) -> crate::Entity<Echo> {
        let view = cx.open::<Echo>();
        cx.update(&view, |echo, _, cx| {
            echo.watch = Some(cx.follow::<LiveTracks>("lobby".into(), |echo, item, cx| {
                match item {
                    Ok(Watching::Announce(LiveAnnounce { name, by, up, .. })) => {
                        echo.roster.push((name, by, up))
                    }
                    Ok(Watching::Waiting) => echo.waits += 1,
                    Err(refusal) => echo.refused.push(refusal.code),
                }
                cx.notify();
            }));
        });
        cx.run_until_parked();
        view
    }

    fn stream(name: &str) -> LivePublish {
        LivePublish {
            track: track(name),
            mode: LiveMode::Stream,
            priority: DEFAULT_PRIORITY,
        }
    }

    #[test]
    fn a_room_list_hears_the_tracks_its_moved_row_adds_and_drops() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let presence = Grant {
            publish: vec!["*".into()],
            subscribe: vec!["presence/*".into()],
        };
        relay.grant("lobby", None, presence.clone());
        let one = a.open::<Echo>();
        let mut doc: Option<Track<u32>> = None;
        let _presence = a.update(&one, |_, _, cx| {
            doc = Some(Track::publish(cx, stream("doc"), |_: &mut Echo, _, _| {}));
            Track::<u32>::publish(cx, stream("presence/1"), |_: &mut Echo, _, _| {})
        });
        a.run_until_parked();
        let two = watcher(&mut b);
        let roster = |b: &mut TestAppContext| b.update(&two, |echo, _, _| echo.roster.clone());
        assert_eq!(roster(&mut b), [("presence/1".into(), 1, true)]);
        // widened: `doc` comes up in the list, as a fresh one would hold it
        relay.grant("lobby", None, Grant::all());
        b.run_until_parked();
        assert_eq!(
            roster(&mut b),
            [("presence/1".into(), 1, true), ("doc".into(), 1, true)]
        );
        // narrowed back: `doc` goes down, and when it really ends no second
        // down is told
        relay.grant("lobby", None, presence);
        b.run_until_parked();
        a.update(&one, |_, _, _| doc = None);
        a.run_until_parked();
        b.run_until_parked();
        assert_eq!(
            roster(&mut b),
            [
                ("presence/1".into(), 1, true),
                ("doc".into(), 1, true),
                ("doc".into(), 1, false)
            ]
        );
    }

    #[test]
    fn a_block_that_narrows_a_room_list_and_ends_a_track_tells_its_down_once() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let _doc = a.update(&one, |_, _, cx| {
            Track::<u32>::publish(cx, stream("doc"), |_: &mut Echo, _, _| {})
        });
        a.run_until_parked();
        let two = watcher(&mut b);
        // one `*` row takes 1's publish of `doc` and 2's view of it
        let presence = Grant {
            publish: vec!["presence/*".into()],
            subscribe: vec!["presence/*".into()],
        };
        relay.grant("lobby", None, presence);
        b.run_until_parked();
        let (roster, refused) = b.update(&two, |echo, _, _| {
            (echo.roster.clone(), echo.refused.clone())
        });
        assert_eq!(roster, [("doc".into(), 1, true), ("doc".into(), 1, false)]);
        assert!(refused.is_empty(), "{refused:?}");
    }

    /// A publication that records its refusals, prefixed with `tag`.
    fn tagged(
        cx: &mut TestAppContext,
        view: &crate::Entity<Echo>,
        tag: &'static str,
        name: &str,
    ) -> Track<String> {
        let request = stream(name);
        cx.update(view, |_, _, cx| {
            Track::publish(cx, request, move |echo: &mut Echo, item, cx| {
                if let Err(refusal) = item {
                    echo.refused.push(format!("{tag}:{}", refusal.code));
                }
                cx.notify();
            })
        })
    }

    #[test]
    fn a_track_this_app_publishes_already_is_refused_and_the_first_goes_on() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let mut first = tagged(&mut a, &one, "first", "echo/1");
        a.run_until_parked();
        let _two = echo(&mut b, 2);
        a.run_until_parked();
        // another window of the same app publishes the same track
        let _second = tagged(&mut a, &one, "second", "echo/1");
        a.run_until_parked();
        assert_eq!(
            a.update(&one, |echo, _, _| echo.refused.clone()),
            ["second:duplicate_track"]
        );
        a.update(&one, |_, _, cx| {
            first.send(&cx.host(), &"still mine".into())
        });
        a.run_until_parked();
        b.run_until_parked();
        assert!(b.has_text("still mine"), "{:?}", b.texts());
    }

    #[test]
    fn a_track_this_app_waits_to_publish_is_refused_a_second_time() {
        let mut a = TestAppContext::new();
        let relay = Relay::new();
        relay.hold("lobby").link(&a);
        let one = a.open::<Echo>();
        let _first = tagged(&mut a, &one, "first", "echo/1");
        let _second = tagged(&mut a, &one, "second", "echo/1");
        a.run_until_parked();
        assert_eq!(
            a.update(&one, |echo, _, _| echo.refused.clone()),
            ["second:duplicate_track"]
        );
    }

    #[test]
    fn a_block_that_widens_a_room_list_and_ends_the_track_tells_it_nothing() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let presence = Grant {
            publish: vec!["*".into()],
            subscribe: vec!["presence/*".into()],
        };
        relay.grant("lobby", None, presence);
        let one = a.open::<Echo>();
        let _doc = tagged(&mut a, &one, "doc", "doc");
        a.run_until_parked();
        let two = watcher(&mut b);
        // one `*` row: publish presence only, subscribe to all. The node ends
        // `doc` first, so the widened list never holds it
        let widened = Grant {
            publish: vec!["presence/*".into()],
            subscribe: vec!["*".into()],
        };
        relay.grant("lobby", None, widened);
        a.run_until_parked();
        b.run_until_parked();
        assert_eq!(b.update(&two, |echo, _, _| echo.roster.clone()), []);
        assert_eq!(
            a.update(&one, |echo, _, _| echo.refused.clone()),
            ["doc:not_granted"]
        );
    }

    #[test]
    fn a_block_that_ends_a_room_list_and_its_track_tells_the_track_down_first() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let _doc = tagged(&mut a, &one, "doc", "doc");
        a.run_until_parked();
        let two = watcher(&mut b);
        relay.revoke("lobby", None);
        b.run_until_parked();
        let (roster, refused) = b.update(&two, |echo, _, _| {
            (echo.roster.clone(), echo.refused.clone())
        });
        assert_eq!(roster, [("doc".into(), 1, true), ("doc".into(), 1, false)]);
        assert_eq!(refused, [refusal::NOT_GRANTED]);
    }

    #[test]
    fn a_hold_after_the_asks_ends_what_the_rows_allowed() {
        let mut a = TestAppContext::new();
        let relay = Relay::new();
        relay.link(&a);
        let one = echo(&mut a, 1);
        relay.hold("lobby");
        a.run_until_parked();
        // its publication and its room list, each `not_granted`
        assert_eq!(
            a.update(&one, |echo, _, _| echo.refused.clone()),
            [refusal::NOT_GRANTED; 2]
        );
    }
}
