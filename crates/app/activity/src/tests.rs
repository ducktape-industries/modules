// The module natively over `guest::MockHost`, told of frames as the kernel tells a hook.

use guest::{Env, MockHost, Module, code, hook};
use store::PageRequest;

use crate::{Activity, Frame, KEPT, MAX_PAGE, MODULE, Query, Reply, Used};

const ADA: &[u8] = &[1; 32];
const ADA_PHONE: &[u8] = &[2; 32];
const STRANGER: &[u8] = &[3; 32];

fn kernel(height: u64) -> Env {
    Env {
        height,
        time: height * 1000,
        ..MockHost::env(MODULE).root()
    }
}

fn record(
    height: u64,
    position: u32,
    signer: &[u8],
    account: Option<u64>,
    target: &str,
) -> hook::Record {
    hook::Record {
        height,
        position,
        signer: signer.to_vec(),
        account,
        target: target.into(),
        applied: true,
    }
}

fn frame(record: &hook::Record) -> Frame {
    Frame {
        height: record.height,
        position: record.position,
        time: record.height * 1000,
        target: record.target.clone(),
        applied: record.applied,
    }
}

fn tell(host: &MockHost, record: &hook::Record) {
    guest::hook::<Activity>(&host.exec(kernel(record.height)), record.clone()).unwrap();
}

fn ask(host: &MockHost, query: Query) -> Reply {
    Activity::query(&host.query(MockHost::env(MODULE)), query).unwrap()
}

fn frames(reply: Reply) -> (Vec<Frame>, Option<Vec<u8>>) {
    match reply {
        Reply::Frames(page) => (page.items, page.next),
        other => panic!("{other:?}"),
    }
}

fn told() -> (MockHost, Vec<hook::Record>) {
    let host = MockHost::default();
    let records = vec![
        record(3, 0, ADA, Some(5), "chat"),
        hook::Record {
            applied: false,
            ..record(3, 1, ADA_PHONE, Some(5), "forge")
        },
        record(3, 2, STRANGER, None, "identity"),
        record(4, 0, ADA, Some(5), "chat"),
        record(9, 3, ADA_PHONE, Some(5), "chat"),
    ];
    for record in &records {
        tell(&host, record);
    }
    (host, records)
}

#[test]
fn an_account_reads_what_it_did_newest_first_and_a_key_what_it_signed() {
    let (host, records) = told();
    let (acted, next) = frames(ask(
        &host,
        Query::Acted {
            account: 5,
            page: PageRequest::default(),
        },
    ));
    let newest_first: Vec<Frame> = [4, 3, 1, 0].iter().map(|at| frame(&records[*at])).collect();
    assert_eq!(acted, newest_first);
    assert_eq!(next, None);
    let (signed, _) = frames(ask(
        &host,
        Query::Signed {
            key: ADA_PHONE.to_vec(),
            page: PageRequest::default(),
        },
    ));
    assert_eq!(signed, [frame(&records[4]), frame(&records[1])]);
    let (stranger, _) = frames(ask(
        &host,
        Query::Signed {
            key: STRANGER.to_vec(),
            page: PageRequest::default(),
        },
    ));
    assert_eq!(
        stranger,
        [frame(&records[2])],
        "a key that holds no account is kept by key"
    );
}

#[test]
fn a_page_resumes_where_the_last_one_stopped() {
    let (host, records) = told();
    let acted = |page| frames(ask(&host, Query::Acted { account: 5, page }));
    let (first, next) = acted(PageRequest::first(3));
    assert_eq!(first.len(), 3);
    let (rest, last) = acted(PageRequest::resume(next, 3));
    assert_eq!(rest, [frame(&records[0])]);
    assert_eq!(last, None);
}

#[test]
fn each_account_counts_its_frames_by_program_and_the_count_pages() {
    let (host, _) = told();
    tell(&host, &record(10, 0, b"\x04", Some(8), "chat"));
    let used = |page| match ask(&host, Query::Used { page }) {
        Reply::Used(page) => (page.items, page.next),
        other => panic!("{other:?}"),
    };
    let count = |account, program: &str, frames| Used {
        account,
        program: program.into(),
        frames,
    };
    let (all, next) = used(PageRequest::default());
    assert_eq!(
        all,
        [
            count(5, "chat", 3),
            count(5, "forge", 1),
            count(8, "chat", 1)
        ]
    );
    assert_eq!(next, None);
    let (first, next) = used(PageRequest::first(2));
    assert_eq!(first, all[..2]);
    assert_eq!(used(PageRequest::resume(next, 2)).0, all[2..]);
}

#[test]
fn only_the_kernel_tells_it_of_a_frame_and_no_one_writes_to_it() {
    let host = MockHost::default();
    let forged = record(3, 0, ADA, Some(5), "chat");
    for env in [
        Env {
            height: 3,
            ..MockHost::env(MODULE)
        },
        Env {
            height: 3,
            ..MockHost::env(MODULE).from_module("chat", Some(8))
        },
    ] {
        let refused = host.refused(|| guest::hook::<Activity>(&host.exec(env), forged.clone()));
        assert_eq!(refused.code, code::UNAUTHORIZED);
    }
    let op = host.refused(|| guest::execute::<Activity>(&host.exec(MockHost::env(MODULE)), &[]));
    assert_eq!(op.code, code::UNSUPPORTED);
}

fn every(host: &MockHost, query: impl Fn(PageRequest) -> Query) -> Vec<(u64, u32)> {
    let mut read = Vec::new();
    let mut next = None;
    loop {
        let (page, more) = frames(ask(host, query(PageRequest::resume(next, MAX_PAGE))));
        read.extend(page.into_iter().map(|frame| (frame.height, frame.position)));
        let Some(more) = more else {
            return read;
        };
        next = Some(more);
    }
}

fn rows(host: &MockHost, prefix: &[u8]) -> usize {
    let state = &host.borrow().state;
    state.keys().filter(|key| key.starts_with(prefix)).count()
}

#[test]
fn each_key_and_account_keeps_its_newest_rows_and_drops_its_oldest_first() {
    let host = MockHost::default();
    let mut told = vec![(1, 0), (1, 1)];
    told.extend((2..=KEPT).map(|height| (height, 0)));
    for (height, position) in &told {
        let signer = if *height == KEPT { ADA_PHONE } else { ADA };
        tell(&host, &record(*height, *position, signer, Some(5), "chat"));
    }
    tell(&host, &record(7, 1, STRANGER, None, "chat"));
    let acted = || every(&host, |page| Query::Acted { account: 5, page });
    let signed = || {
        every(&host, |page| Query::Signed {
            key: ADA.to_vec(),
            page,
        })
    };
    let newest: Vec<(u64, u32)> = told.iter().rev().copied().collect();
    assert_eq!(acted().len(), KEPT as usize);
    assert_eq!(
        acted(),
        newest[..KEPT as usize],
        "the oldest frame, (1, 0), went"
    );
    assert_eq!(
        signed(),
        newest[1..],
        "ADA signed exactly KEPT frames and kept them all"
    );
    tell(&host, &record(KEPT + 1, 0, ADA, Some(5), "forge"));
    let acted = acted();
    assert_eq!(acted.len(), KEPT as usize);
    assert_eq!(acted.first(), Some(&(KEPT + 1, 0)));
    assert_eq!(
        acted.last(),
        Some(&(2, 0)),
        "(1, 1) went next, before block 2"
    );
    let signed = signed();
    assert_eq!(signed.len(), KEPT as usize);
    assert_eq!(
        signed.last(),
        Some(&(1, 1)),
        "ADA's first drop is its oldest, (1, 0)"
    );
    tell(&host, &record(KEPT + 2, 0, ADA, Some(5), "forge"));
    assert_eq!(
        every(&host, |page| Query::Signed {
            key: ADA.to_vec(),
            page
        })
        .last(),
        Some(&(2, 0))
    );
    assert_eq!(
        every(&host, |page| Query::Signed {
            key: STRANGER.to_vec(),
            page
        }),
        [(7, 1)],
        "another key keeps its own rows"
    );
    assert_eq!(
        rows(&host, b"a/"),
        KEPT as usize,
        "no row past the cap is left in state"
    );
    assert_eq!(
        rows(&host, b"k/"),
        KEPT as usize + 2,
        "ADA's cap, its phone's one, the stranger's one"
    );
    let Reply::Used(used) = ask(
        &host,
        Query::Used {
            page: PageRequest::default(),
        },
    ) else {
        panic!()
    };
    let counted: Vec<u64> = used.items.iter().map(|used| used.frames).collect();
    assert_eq!(counted, [KEPT + 1, 2], "the counts keep every frame");
}
