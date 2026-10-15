//! [`Relay`]: the node's live tracks for two or more views under test, each
//! in its own [`TestAppContext`], so what one publishes the others hear.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::host::Error;
use crate::methods::{
    LiveAnnounce, LiveItem, LiveMode, LivePublish, LivePublishTrack, LiveSend, LiveSendObject,
    LiveSubscribe, LiveSubscribeTrack, LiveTrack, LiveTracks, Method, Publishing, refusal,
};
use crate::wire::{Event, Frame};

use super::{FakeHost, TestAppContext};

/// The node's relay for views under test: [`link`](Relay::link) each
/// view's [`TestAppContext`] and it answers their `live.*` requests as one
/// node would: a track a view publishes is announced to every
/// `live.tracks` of its room, an object one view sends reaches every view
/// subscribed to the track, the latest group is replayed to a late
/// subscriber, and a publisher hears its subscriber count. Every room is
/// granted unless the test [`hold`](Relay::hold)s it; the n-th view linked
/// acts as account n.
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
    /// Every `live.tracks` open: its room, by link and id.
    watches: BTreeMap<(usize, u64), String>,
    /// The rooms whose program has granted nothing yet.
    held: BTreeSet<String>,
    /// Every ask a held room keeps, in the order asked.
    waiting: Vec<Waiting>,
}

/// An ask a held room keeps until [`Relay::grant`], answered then as if
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

    /// The `not_granted` item that tells the view its ask waits.
    fn waits(&self, id: u64) -> Event {
        let refusal = refused(
            refusal::NOT_GRANTED,
            "the room's program has granted nothing yet",
        );
        match self {
            Ask::Publish(..) => item::<LivePublishTrack>(id, Err(refusal), false),
            Ask::Subscribe(_) => item::<LiveSubscribeTrack>(id, Err(refusal), false),
            Ask::Watch(_) => item::<LiveTracks>(id, Err(refusal), false),
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
    subscribers: Vec<(usize, u64)>,
}

struct Group {
    number: u64,
    objects: Vec<(u32, Vec<u8>)>,
    closed: bool,
}

fn item<M: Method>(id: u64, reply: Result<M::Reply, Error>, done: bool) -> Event {
    Event::Response {
        id,
        result: reply.map(|reply| M::encode_reply(&reply)),
        done,
    }
}

fn refused(code: &str, message: &str) -> Error {
    Error::new(code, message)
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

    /// `room`'s program has granted nothing yet, as before the block that
    /// writes its rows: each `live.publish`, `live.subscribe` and
    /// `live.tracks` in it hears one `not_granted` item that does not end
    /// it, and waits for [`grant`](Relay::grant). What a view sends onto a
    /// waiting track goes out once the track is up.
    pub fn hold(&self, room: &str) -> &Self {
        self.0.borrow_mut().held.insert(room.to_owned());
        self
    }

    /// The block that writes `room`'s rows: each ask waiting in it is
    /// answered as if just asked, the publications first, so a waiting
    /// subscription to one of them finds it up.
    pub fn grant(&self, room: &str) {
        self.0.borrow_mut().grant(room);
    }

    /// The view linked as `cx` goes away with its tracks up, as an app
    /// that quit: each one ends `publisher_gone` for its subscribers and
    /// goes down in every `live.tracks`; its subscriptions are dropped.
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
        state.subscriptions.retain(|(at, _), _| *at != link);
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

    fn announce(&self, (room, name): &(String, String), link: usize, mode: LiveMode, up: bool) {
        let announce = LiveAnnounce {
            name: name.clone(),
            by: Self::account(link),
            mode,
            up,
        };
        for ((at, id), watched) in &self.watches {
            if watched == room {
                self.tell(*at, item::<LiveTracks>(*id, Ok(announce.clone()), false));
            }
        }
    }

    fn count(&self, published: &Published) {
        let count = Publishing::Subscribers(published.subscribers.len() as u32);
        self.tell(
            published.link,
            item::<LivePublishTrack>(published.request, Ok(count), false),
        );
    }

    /// An ask, kept waiting while its room is held.
    fn ask(&mut self, link: usize, id: u64, ask: Ask) {
        if self.held.contains(ask.room()) {
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

    fn grant(&mut self, room: &str) {
        self.held.remove(room);
        let (granted, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|waiting| waiting.ask.room() == room);
        self.waiting = held;
        let (publications, others): (Vec<_>, Vec<_>) = granted
            .into_iter()
            .partition(|waiting| waiting.ask.publishes());
        for Waiting { link, id, ask } in publications.into_iter().chain(others) {
            self.admit(link, id, ask);
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
        if let Some(held) = self.tracks.get(&track)
            && held.link != link
        {
            let refusal = refused(refusal::DUPLICATE_TRACK, "another account publishes it");
            self.tell(link, item::<LivePublishTrack>(id, Err(refusal), true));
            return;
        }
        // the same account publishing again supersedes its old publication
        if self.tracks.contains_key(&track) {
            self.end(&track, refusal::PUBLISHER_GONE, "published again");
        }
        let published = Published {
            link,
            request: id,
            mode: publish.mode,
            group: None,
            subscribers: Vec::new(),
        };
        self.count(&published);
        self.announce(&track, link, publish.mode, true);
        self.tracks.insert(track, published);
    }

    /// The track ends with `code` for each subscriber and goes down in
    /// every `live.tracks`.
    fn end(&mut self, track: &(String, String), code: &str, message: &str) {
        let Some(published) = self.tracks.remove(track) else {
            return;
        };
        for (at, id) in &published.subscribers {
            self.subscriptions.remove(&(*at, *id));
            let refusal = refused(code, message);
            self.tell(*at, item::<LiveSubscribeTrack>(*id, Err(refusal), true));
        }
        self.announce(track, published.link, published.mode, false);
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
        if let Some(track) = self.subscriptions.remove(&(link, id))
            && let Some(published) = self.tracks.get_mut(&track)
        {
            published
                .subscribers
                .retain(|subscriber| *subscriber != (link, id));
            let published = &self.tracks[&track];
            self.count(published);
        }
        self.watches.remove(&(link, id));
        self.waiting
            .retain(|waiting| (waiting.link, waiting.id) != (link, id));
    }

    fn send(&mut self, link: usize, id: u64, send: LiveSend) {
        if let Some(held) = self.held_objects(link, &send.track) {
            held.push((id, send));
            return;
        }
        let track = (send.track.room, send.track.name);
        let Some(published) = self.tracks.get_mut(&track).filter(|held| held.link == link) else {
            let refusal = refused(refusal::LIVE_NOT_PUBLISHED, "no such track is published");
            self.tell(link, item::<LiveSendObject>(id, Err(refusal), true));
            return;
        };
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
                items.push(LiveItem::Object {
                    group: send.group,
                    object: send.object,
                    body: send.body,
                });
            }
            LiveMode::Stream => {
                let open = published
                    .group
                    .as_ref()
                    .is_some_and(|group| group.number == send.group && !group.closed);
                if !open {
                    // a newer group closes the one before it
                    if let Some(group) = published.group.as_ref().filter(|group| !group.closed) {
                        items.push(LiveItem::GroupEnd {
                            group: group.number,
                            complete: true,
                        });
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
                    items.push(LiveItem::Object {
                        group: send.group,
                        object,
                        body: send.body,
                    });
                }
                if send.end {
                    group.closed = true;
                    items.push(LiveItem::GroupEnd {
                        group: send.group,
                        complete: true,
                    });
                }
            }
        }
        for (at, subscription) in &published.subscribers {
            for live in &items {
                let event = item::<LiveSubscribeTrack>(*subscription, Ok(live.clone()), false);
                self.hosts[*at].owe(vec![event]);
            }
        }
    }

    fn subscribe(&mut self, link: usize, id: u64, subscribe: LiveSubscribe) {
        let track = (subscribe.track.room, subscribe.track.name);
        let Some(published) = self.tracks.get_mut(&track) else {
            let refusal = refused(refusal::UNKNOWN_TRACK, "no one publishes this track");
            self.tell(link, item::<LiveSubscribeTrack>(id, Err(refusal), true));
            return;
        };
        published.subscribers.push((link, id));
        let replay = published.group.as_ref().filter(|_| subscribe.latest);
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
        let events = items
            .into_iter()
            .map(|live| item::<LiveSubscribeTrack>(id, Ok(live), false))
            .collect();
        self.hosts[link].owe(events);
        self.subscriptions.insert((link, id), track.clone());
        self.count(&self.tracks[&track]);
    }

    fn watch(&mut self, link: usize, id: u64, room: String) {
        let up: Vec<_> = self
            .tracks
            .iter()
            .filter(|((at, _), _)| *at == room)
            .map(|((_, name), published)| LiveAnnounce {
                name: name.clone(),
                by: Self::account(published.link),
                mode: published.mode,
                up: true,
            })
            .collect();
        let events = up
            .into_iter()
            .map(|announce| item::<LiveTracks>(id, Ok(announce), false))
            .collect();
        self.hosts[link].owe(events);
        self.watches.insert((link, id), room);
    }
}

/// The kinds the relay answers, so a test can hold its arms to
/// [`crate::methods::ALL`].
#[cfg(test)]
const KINDS: [&str; 4] = [
    LivePublishTrack::KIND,
    LiveSendObject::KIND,
    LiveSubscribeTrack::KIND,
    LiveTracks::KIND,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{Received, Track, subscribe};
    use crate::methods::{Capability, LiveTrack};
    use crate::prelude::*;
    use crate::{Task, View};
    use serde::{Deserialize, Serialize};

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

    /// One line per person: publishes `echo/<me>` in stream mode, one
    /// group per line, and hears every `echo/*` `live.tracks` announces.
    #[derive(Default, Serialize, Deserialize)]
    struct Echo {
        me: u64,
        heard: Vec<String>,
        roster: Vec<(String, u64, bool)>,
        subscribers: Option<u32>,
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
            self.mine = Some(Track::publish(
                cx,
                track(&format!("echo/{me}")),
                LiveMode::Stream,
                |view: &mut Echo, item, cx| {
                    match item {
                        Ok(Publishing::Subscribers(count)) => view.subscribers = Some(count),
                        Err(refusal) => view.refused.push(refusal.code),
                    }
                    cx.notify();
                },
            ));
            self.watch = Some(cx.follow::<LiveTracks>("lobby".into(), |view, item, cx| {
                let Ok(LiveAnnounce { name, by, up, .. }) = item else {
                    return;
                };
                view.roster.push((name.clone(), by, up));
                cx.notify();
                if up && by != view.me {
                    view.hear(&name, cx);
                }
            }));
        }

        /// Subscribes to `name`: its lines into `heard`, its refusals into
        /// `refused`.
        fn hear(&mut self, name: &str, cx: &mut Context<Self>) {
            let task = subscribe(cx, track(name), true, |view: &mut Echo, item, cx| {
                match item {
                    Ok(Received::Object { value, .. }) => view.heard.push(value),
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
        let waited = a.update(&one, |echo, _, _| {
            (echo.refused.clone(), echo.subscribers, echo.roster.clone())
        });
        assert_eq!(waited, (vec![refusal::NOT_GRANTED.into()], None, vec![]));
        assert_eq!(
            b.update(&two, |echo, _, _| echo.refused.clone()),
            [refusal::NOT_GRANTED, refusal::NOT_GRANTED]
        );
        relay.grant("lobby");
        a.run_until_parked();
        b.run_until_parked();
        // a's track came up first, so b's subscription found it and heard
        // the line; the one to no one's track ended `unknown_track`
        assert!(b.has_text("early"), "{:?}", b.texts());
        assert_eq!(
            b.update(&two, |echo, _, _| echo.refused.clone()),
            [
                refusal::NOT_GRANTED,
                refusal::NOT_GRANTED,
                refusal::UNKNOWN_TRACK
            ]
        );
        // a's publication and room list went on past their `not_granted`
        let admitted = a.update(&one, |echo, _, _| (echo.subscribers, echo.roster.clone()));
        assert_eq!(admitted, (Some(1), vec![("echo/1".into(), 1, true)]));
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
        relay.grant("lobby");
        let three = echo(&mut c, 3);
        let roster = c.update(&three, |echo, _, _| echo.roster.clone());
        assert_eq!(roster, [("echo/3".into(), 3, true)]);
    }

    #[test]
    fn publish_at_asks_the_node_for_its_priority() {
        let mut a = TestAppContext::new();
        let one = a.open::<Echo>();
        let _top = a.update(&one, |_, _, cx| {
            Track::<u32>::publish_at(
                cx,
                track("cursor/1"),
                LiveMode::Datagram,
                0,
                |_: &mut Echo, _, _| {},
            )
        });
        a.run_until_parked();
        let asked = a.host().requests::<LivePublishTrack>();
        assert_eq!(
            asked.iter().map(|ask| ask.priority).collect::<Vec<_>>(),
            [0]
        );
    }

    #[test]
    fn a_datagram_track_keeps_the_newest_and_drops_an_older_object() {
        let (mut a, mut b) = (TestAppContext::new(), TestAppContext::new());
        let relay = Relay::new();
        relay.link(&a).link(&b);
        let one = a.open::<Echo>();
        let mut cursor: Option<Track<u32>> = None;
        a.update(&one, |_, _, cx| {
            cursor = Some(Track::publish(
                cx,
                track("cursor/1"),
                LiveMode::Datagram,
                |_: &mut Echo, _, _| {},
            ));
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
                track("cursor/1"),
                true,
                move |_: &mut Echo, item: Result<Received<u32>, _>, _| {
                    sink.borrow_mut().push(item.unwrap());
                },
            )
        });
        b.run_until_parked();
        // the newest object alone is replayed
        assert_eq!(
            heard.borrow()[1..],
            [Received::Object {
                group: 5,
                object: 1,
                value: 51
            }]
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
}
