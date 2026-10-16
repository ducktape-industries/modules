//! The order a streamed reply's events fold in: a repeat is dropped, a `seq` back at 0 starts
//! the reply over, a gap breaks it.
use super::*;
use crate::wire::tests::{TEXT, events};
use crate::{Said, Wire};

fn event(seq: u64, data: &[u8]) -> Event {
    Event {
        id: 1,
        seq,
        data: data.to_vec(),
    }
}

fn through(partial: Partial, events: &[Vec<u8>], seqs: impl IntoIterator<Item = u64>) -> Partial {
    seqs.into_iter().fold(partial, |partial, seq| {
        fold(partial, &event(seq, &events[seq as usize]))
    })
}

#[test]
fn a_repeated_event_is_dropped() {
    let events = events(TEXT);
    let once = through(Partial::new(Wire::AnthropicMessages), &events, 0..8);
    let repeated = through(
        Partial::new(Wire::AnthropicMessages),
        &events,
        [0, 1, 2, 3, 3, 2, 4, 5, 6, 7, 7],
    );
    assert_eq!(repeated, once);
    assert_eq!(once.said, [Said::Text("Hello, world".into())]);
    assert_eq!(once.flow, Flow::Ended);
}

#[test]
fn an_event_back_at_zero_starts_the_reply_over() {
    let events = events(TEXT);
    let first = through(Partial::new(Wire::AnthropicMessages), &events, 0..5);
    assert_eq!(first.said, [Said::Text("Hello, world".into())]);
    let restarted = through(first, &events, 0..4);
    assert_eq!(restarted.said, [Said::Text("Hello".into())]);
    assert_eq!(restarted.next_seq, 4);
    let finished = through(restarted, &events, 4..8);
    assert_eq!(
        finished,
        through(Partial::new(Wire::AnthropicMessages), &events, 0..8)
    );
}

#[test]
fn a_gap_breaks_the_reply_until_it_starts_over() {
    let events = events(TEXT);
    let gapped = through(Partial::new(Wire::AnthropicMessages), &events, [0, 1, 3, 4]);
    assert_eq!(
        gapped.flow,
        Flow::Broken {
            reason: "event 3 arrived where event 2 was due".into()
        }
    );
    let over = through(gapped, &events, 0..8);
    assert_eq!(over.flow, Flow::Ended);
}
