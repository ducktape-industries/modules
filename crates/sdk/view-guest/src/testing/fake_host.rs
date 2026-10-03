use crate::{
    host::{Error, Shape, malformed},
    methods::{self, Method},
    wire::{Event, Frame, Request},
};
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};

/// A method kind and the program it addresses (`None` for the host's own).
type Key = (String, Option<String>);
type Handler = Box<dyn FnMut(&Request) -> Option<Event>>;
type Stream = Rc<RefCell<StreamState>>;

#[derive(Default)]
struct State {
    /// What answers an ask (`handle`, `refuse`, `never`).
    asks: HashMap<Key, Handler>,
    /// What answers a subscription instead of a feed (`refuse`).
    refusals: HashMap<Key, Handler>,
    /// One feed per method: the subscriptions it reaches.
    streams: HashMap<Key, Stream>,
    requests: Vec<Request>,
    events: Vec<Event>,
    declared: Option<&'static [methods::Capability]>,
    /// The programs the view's `TARGETS` names; a node method naming
    /// another fails the test, as the app refuses it.
    targets: &'static [&'static str],
}

/// A typed host, by the shape of each request:
/// - an ask waits for its answer, so the test says what comes back
///   ([`handle`](Self::handle), [`refuse`](Self::refuse) or
///   [`never`](Self::never)); an ask nothing answers fails the test;
/// - a subscription stays open, quiet until the test feeds it through
///   [`stream`](Self::stream), before or after the view subscribes;
/// - a notify waits for nothing.
///
/// Every request is recorded: [`requests`](Self::requests).
#[derive(Clone, Default)]
pub struct FakeHost(Rc<RefCell<State>>);

fn key<C: Method>() -> Key {
    (C::KIND.to_owned(), C::TARGET.map(str::to_owned))
}

/// The entry for `kind` addressed to `target`, else the one addressed to
/// no program in particular.
fn find<'a, T>(
    map: &'a mut HashMap<Key, T>,
    kind: &str,
    target: Option<&str>,
) -> Option<&'a mut T> {
    let exact = (kind.to_owned(), target.map(str::to_owned));
    if map.contains_key(&exact) {
        return map.get_mut(&exact);
    }
    map.get_mut(&(kind.to_owned(), None))
}

impl FakeHost {
    /// Answers each ask of `C` with what `handler` returns.
    pub fn handle<C: Method>(
        &self,
        mut handler: impl FnMut(C::Request) -> Result<C::Reply, Error> + 'static,
    ) {
        self.0.borrow_mut().asks.insert(
            key::<C>(),
            answer::<C>(move |request| Some(handler(request))),
        );
    }

    /// The host never answers an ask of `C`: it stays pending.
    pub fn never<C: Method>(&self) {
        self.0
            .borrow_mut()
            .asks
            .insert(key::<C>(), answer::<C>(|_| None));
    }

    /// Refuses every ask of and subscription to `C`.
    pub fn refuse<C: Method>(&self, code: &str, message: &str) {
        let refusal = Error::new(code, message);
        let mut state = self.0.borrow_mut();
        let ask = refusal.clone();
        state
            .asks
            .insert(key::<C>(), answer::<C>(move |_| Some(Err(ask.clone()))));
        state
            .refusals
            .insert(key::<C>(), answer::<C>(move |_| Some(Err(refusal.clone()))));
    }

    /// The feed of `C`'s subscriptions: every one the view opens, before or
    /// after this call, hears what it sends. One feed per method: a second
    /// call hands out the same one.
    pub fn stream<C: Method>(&self) -> StreamSender<C> {
        let state = self
            .0
            .borrow_mut()
            .streams
            .entry(key::<C>())
            .or_default()
            .clone();
        StreamSender {
            state,
            host: self.clone(),
            marker: PhantomData,
        }
    }

    /// Every `C` the view asked, subscribed to or notified, in order.
    pub fn requests<C: Method>(&self) -> Vec<C::Request> {
        self.0
            .borrow()
            .requests
            .iter()
            .filter(|r| matches::<C>(r))
            .map(|r| C::decode_request(&r.payload).expect("valid capability request"))
            .collect()
    }
    pub(super) fn reset_connection(&self) {
        let mut state = self.0.borrow_mut();
        state.events.clear();
        for stream in state.streams.values() {
            let mut stream = stream.borrow_mut();
            if let Some(host) = stream.host.take() {
                for id in stream.ids.drain(..) {
                    host.close_stream(id);
                }
            }
        }
    }
    pub(super) fn declare(
        &self,
        capabilities: &'static [methods::Capability],
        targets: &'static [&'static str],
    ) {
        let mut state = self.0.borrow_mut();
        state.declared = Some(capabilities);
        state.targets = targets;
    }
    pub(super) fn take_events(&self) -> Vec<Event> {
        std::mem::take(&mut self.0.borrow_mut().events)
    }
    /// Answers or feed items wait for the view's next tick.
    pub(super) fn owes_events(&self) -> bool {
        !self.0.borrow().events.is_empty()
    }

    pub(super) fn accept(&self, frame: &Frame, host: &crate::host::Host) {
        // The app refuses a frame past either budget whole, and the view
        // with it; a test fails on it instead.
        assert!(
            frame.requests.len() <= crate::wire::MAX_REQUESTS
                && frame.cancels.len() <= crate::wire::MAX_CANCELS,
            "the host refuses a frame of {} requests and {} cancels: one frame carries at most {} and {}",
            frame.requests.len(),
            frame.cancels.len(),
            crate::wire::MAX_REQUESTS,
            crate::wire::MAX_CANCELS,
        );
        let mut state = self.0.borrow_mut();
        for stream in state.streams.values() {
            let mut stream = stream.borrow_mut();
            stream.host = Some(host.clone());
            stream.ids.retain(|id| !frame.cancels.contains(id));
        }
        for request in &frame.requests {
            state.requests.push(request.clone());
            // The app refuses a method the manifest leaves out
            // (`undeclared_capability`); a test fails on it instead.
            if let (Some(declared), Some((capability, _))) =
                (state.declared, methods::Capability::of_kind(&request.kind))
            {
                assert!(
                    declared.contains(&capability),
                    "{}: `{}` needs the `{}` capability, \
                     which this view's CAPABILITIES does not declare",
                    methods::refusal::UNDECLARED_CAPABILITY,
                    request.kind,
                    capability.as_str()
                );
            }
            // and a node method naming a program the manifest does not
            // list (`undeclared_target`)
            let target = target_of(request);
            if state.declared.is_some()
                && let Some(target) = &target
            {
                assert!(
                    state.targets.contains(&target.as_str()),
                    "{}: `{}` names `{target}`, which this view's TARGETS does not list",
                    methods::refusal::UNDECLARED_TARGET,
                    request.kind,
                );
            }
            let (kind, target) = (request.kind.as_str(), target.as_deref());
            let shown = || {
                host.diagnostic(request.id)
                    .unwrap_or_else(|| String::from_utf8_lossy(&request.payload).into_owned())
            };
            let event = match host.shape(request.id) {
                Shape::Notify => {
                    find(&mut state.asks, kind, target).and_then(|answer| answer(request))
                }
                Shape::Ask => {
                    if let Some(answer) = find(&mut state.asks, kind, target) {
                        answer(request)
                    } else if find(&mut state.streams, kind, target).is_some() {
                        panic!(
                            "the view asks `{kind}` {}, and `stream` feeds only subscriptions: \
                             answer the ask with `handle`, `refuse` or `never`",
                            shown()
                        )
                    } else {
                        panic!(
                            "unhandled ask `{kind}` {}: the view waits for its answer, so say \
                             what comes back with `handle`, `refuse` or `never`",
                            shown()
                        )
                    }
                }
                Shape::Subscription => {
                    if let Some(refuse) = find(&mut state.refusals, kind, target) {
                        refuse(request)
                    } else {
                        if find(&mut state.streams, kind, target).is_none() {
                            assert!(
                                find(&mut state.asks, kind, target).is_none(),
                                "the view subscribes to `{kind}` {}, and `handle` and `never` \
                                 answer only asks: a subscription stays open by itself, and \
                                 `stream` feeds it",
                                shown()
                            );
                        }
                        let stream = match find(&mut state.streams, kind, target) {
                            Some(stream) => stream.clone(),
                            None => state
                                .streams
                                .entry((kind.to_owned(), target.map(str::to_owned)))
                                .or_default()
                                .clone(),
                        };
                        let mut stream = stream.borrow_mut();
                        stream.host = Some(host.clone());
                        stream.ids.push(request.id);
                        if stream.closed {
                            host.close_stream(request.id);
                        }
                        None
                    }
                }
            };
            state.events.extend(event);
        }
    }
}

/// An answer to one request of `C`: the reply `reply` gives, if any.
fn answer<C: Method>(
    mut reply: impl FnMut(C::Request) -> Option<Result<C::Reply, Error>> + 'static,
) -> Handler {
    Box::new(move |request| {
        let reply = match C::decode_request(&request.payload) {
            Ok(decoded) => reply(decoded)?,
            Err(error) => Err(malformed(error)),
        };
        Some(Event::Response {
            id: request.id,
            result: reply.map(|reply| C::encode_reply(&reply)),
            done: true,
        })
    })
}

/// The program a node method addresses: `module.changes` names it outright,
/// `module.query` and `op.submit` carry it on their [`methods::Call`]
/// envelope. `None` for every other kind (`module.describe` names a
/// program too, but reads its describe module: no target of the view's).
fn target_of(request: &Request) -> Option<String> {
    match request.kind.as_str() {
        "module.changes" => methods::decode::<String>(&request.payload).ok(),
        "module.query" | "op.submit" => methods::decode::<methods::Call>(&request.payload)
            .ok()
            .map(|call| call.target),
        _ => None,
    }
}

fn matches<C: Method>(request: &Request) -> bool {
    request.kind == C::KIND
        && C::TARGET.is_none_or(|target| target_of(request).as_deref() == Some(target))
}

#[derive(Default)]
struct StreamState {
    ids: Vec<u64>,
    closed: bool,
    host: Option<crate::host::Host>,
}

/// What a test sends down a method's subscriptions.
#[must_use = "the feed is how a test speaks for the host: keep it to send"]
pub struct StreamSender<C: Method> {
    state: Stream,
    host: FakeHost,
    marker: PhantomData<C>,
}
impl<C: Method> StreamSender<C> {
    /// One item to every open subscription; with none open, the item would
    /// reach nobody, and the test fails instead.
    pub fn send(&self, item: C::Reply) {
        let state = self.state.borrow();
        assert!(!state.closed, "cannot send to a closed stream");
        assert!(
            !state.ids.is_empty(),
            "no subscription to `{}` is open: the view has not subscribed (open it \
             first) or it dropped the stream, and the item would reach nobody",
            C::KIND
        );
        let payload = C::encode_reply(&item);
        self.host
            .0
            .borrow_mut()
            .events
            .extend(state.ids.iter().map(|id| Event::Response {
                id: *id,
                result: Ok(payload.clone()),
                done: false,
            }));
    }
    /// Whether the view holds a subscription this feed reaches.
    pub fn subscribed(&self) -> bool {
        !self.state.borrow().ids.is_empty()
    }
    pub fn close(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        let host = state.host.clone();
        for id in state.ids.drain(..) {
            host.as_ref().expect("active stream host").close_stream(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods::{Changes, Program, Query};

    struct First;
    struct Second;
    macro_rules! module {
        ($name:ident, $target:literal) => {
            impl Program for $name {
                const NAME: &'static str = $target;
                type Op = ();
                type Query = String;
                type Reply = String;
            }
        };
    }
    module!(First, "first");
    module!(Second, "second");

    fn request<C: Method>(id: u64, value: C::Request) -> Request {
        Request {
            id,
            kind: C::KIND.into(),
            payload: C::encode_request(&value),
        }
    }

    #[test]
    fn handlers_and_request_history_distinguish_envelope_targets() {
        let host = FakeHost::default();
        host.handle::<Query<First>>(|query| Ok(format!("first:{query}")));
        host.handle::<Query<Second>>(|query| Ok(format!("second:{query}")));
        host.accept(
            &Frame {
                requests: vec![
                    request::<Query<First>>(1, "one".into()),
                    request::<Query<Second>>(2, "two".into()),
                ],
                ..Frame::default()
            },
            &crate::host::Host::default(),
        );
        assert_eq!(host.requests::<Query<First>>(), ["one"]);
        assert_eq!(host.requests::<Query<Second>>(), ["two"]);
        let events = host.take_events();
        assert!(
            matches!(&events[0], Event::Response { id: 1, result: Ok(bytes), done: true } if Query::<First>::decode_reply(bytes).unwrap() == "first:one")
        );
        assert!(
            matches!(&events[1], Event::Response { id: 2, result: Ok(bytes), done: true } if Query::<Second>::decode_reply(bytes).unwrap() == "second:two")
        );
    }

    #[test]
    fn streams_stop_delivering_to_cancelled_subscriptions() {
        let host = FakeHost::default();
        let feed = host.stream::<Changes<First>>();
        let channel = crate::host::Host::default();
        let stream = channel.subscribe::<Changes<First>>(());
        host.accept(
            &Frame {
                requests: channel.drain_outbox(),
                ..Frame::default()
            },
            &channel,
        );
        feed.send(None);
        assert_eq!(host.take_events().len(), 1);
        drop(stream);
        host.accept(
            &Frame {
                cancels: channel.drain_cancels(),
                ..Frame::default()
            },
            &channel,
        );
        assert!(!feed.subscribed());
    }

    /// Sends what `channel` asked since the last frame to `host`.
    fn flush(host: &FakeHost, channel: &crate::host::Host) {
        host.accept(
            &Frame {
                requests: channel.drain_outbox(),
                cancels: channel.drain_cancels(),
                ..Frame::default()
            },
            channel,
        );
    }

    #[test]
    fn a_subscription_nothing_feeds_stays_open_and_a_later_feed_reaches_it() {
        use futures::StreamExt;
        let host = FakeHost::default();
        let channel = crate::host::Host::default();
        let mut stream = channel.subscribe::<Changes<First>>(());
        flush(&host, &channel);
        assert_eq!(host.requests::<Changes<First>>().len(), 1);
        assert!(host.take_events().is_empty());
        let feed = host.stream::<Changes<First>>();
        feed.send(None);
        for event in host.take_events() {
            let crate::wire::Event::Response { id, result, done } = event else {
                panic!("an answer")
            };
            channel.fulfill(id, result, done);
        }
        assert!(futures::executor::block_on(stream.next()).is_some());
    }

    #[test]
    fn a_second_feed_of_one_method_is_the_first() {
        let host = FakeHost::default();
        let first = host.stream::<Changes<First>>();
        let channel = crate::host::Host::default();
        let _stream = channel.subscribe::<Changes<First>>(());
        flush(&host, &channel);
        let second = host.stream::<Changes<First>>();
        second.send(None);
        assert_eq!(host.take_events().len(), 1);
        first.send(None);
        assert_eq!(host.take_events().len(), 1);
    }

    #[test]
    #[should_panic(expected = "no subscription to `module.changes` is open")]
    fn a_feed_nobody_hears_fails_the_test() {
        FakeHost::default().stream::<Changes<First>>().send(None);
    }

    #[test]
    #[should_panic(expected = "the view asks `module.query`")]
    fn a_feed_for_an_ask_names_the_mismatch() {
        let host = FakeHost::default();
        let _feed = host.stream::<Query<First>>();
        let channel = crate::host::Host::default();
        let _ask = channel.ask::<Query<First>>("one".into());
        flush(&host, &channel);
    }

    #[test]
    #[should_panic(expected = "the view subscribes to `module.changes`")]
    fn an_answer_for_a_subscription_names_the_mismatch() {
        let host = FakeHost::default();
        host.never::<Changes<First>>();
        let channel = crate::host::Host::default();
        let _stream = channel.subscribe::<Changes<First>>(());
        flush(&host, &channel);
    }

    #[test]
    fn a_notify_is_recorded() {
        let host = FakeHost::default();
        let channel = crate::host::Host::default();
        channel.log("hello");
        flush(&host, &channel);
        assert_eq!(host.requests::<crate::methods::HostLog>(), ["hello"]);
        assert!(host.take_events().is_empty());
    }

    #[test]
    fn one_capability_can_handle_asks_and_subscriptions_independently() {
        let host = FakeHost::default();
        host.handle::<Query<First>>(|query| Ok(format!("answer:{query}")));
        let feed = host.stream::<Query<First>>();
        let channel = crate::host::Host::default();
        let _ask = channel.ask::<Query<First>>("ask".into());
        let _stream = channel.subscribe::<Query<First>>("subscribe".into());
        host.accept(
            &Frame {
                requests: channel.drain_outbox(),
                ..Frame::default()
            },
            &channel,
        );
        let replies = host.take_events();
        assert!(matches!(
            &replies[..],
            [Event::Response {
                id: 0,
                done: true,
                ..
            }]
        ));
        feed.send("item".into());
        assert!(matches!(
            &host.take_events()[..],
            [Event::Response {
                id: 1,
                done: false,
                ..
            }]
        ));
    }

    #[test]
    fn closing_a_feed_finishes_without_fabricating_an_item_or_refusal() {
        use futures::StreamExt;
        let host = FakeHost::default();
        let feed = host.stream::<Changes<First>>();
        let channel = crate::host::Host::default();
        let mut stream = channel.subscribe::<Changes<First>>(());
        host.accept(
            &Frame {
                requests: channel.drain_outbox(),
                ..Frame::default()
            },
            &channel,
        );
        feed.close();
        assert!(futures::executor::block_on(stream.next()).is_none());
    }

    #[test]
    #[should_panic(expected = "unhandled ask `module.query`")]
    fn an_ask_nothing_answers_fails_at_the_host_boundary() {
        let host = FakeHost::default();
        let channel = crate::host::Host::default();
        let _ask = channel.ask::<Query<First>>("unexpected".into());
        flush(&host, &channel);
    }
}
