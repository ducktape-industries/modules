// The program natively over `program::MockHost`, told of frames as the kernel tells a hook.

use ::program::{Env, MockHost, Program, code, hook};
use store::PageRequest;

use crate::{Activity, Frame, PROGRAM, Query, Reply, Row, Used};

const ADA: &[u8] = &[1; 32];
const ADA_PHONE: &[u8] = &[2; 32];
const STRANGER: &[u8] = &[3; 32];

fn kernel(height: u64) -> Env {
    Env {
        height,
        time: height * 1000,
        ..MockHost::env(PROGRAM).root()
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
    ::program::hook::<Activity>(&host.exec(kernel(record.height)), record.clone()).unwrap();
}

fn ask(host: &MockHost, query: Query) -> Reply {
    Activity::query(&host.query(MockHost::env(PROGRAM)), query).unwrap()
}

fn row(record: &hook::Record) -> Row {
    Row {
        frame: frame(record),
        signer: record.signer.clone(),
        account: record.account,
    }
}

fn rows(reply: Reply) -> (Vec<Row>, Option<Vec<u8>>) {
    match reply {
        Reply::Recent(page) => (page.items, page.next),
        other => panic!("{other:?}"),
    }
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
fn the_chain_lists_every_frame_newest_first_whoever_sent_it() {
    let (host, records) = told();
    let recent = |program: Option<&str>, page| {
        rows(ask(
            &host,
            Query::Recent {
                program: program.map(Into::into),
                page,
            },
        ))
    };
    let (all, next) = recent(None, PageRequest::default());
    let newest_first: Vec<Row> = [4, 3, 2, 1, 0]
        .iter()
        .map(|at| row(&records[*at]))
        .collect();
    assert_eq!(
        all, newest_first,
        "later blocks first, later positions first within one"
    );
    assert_eq!(next, None);
    assert_eq!(
        all[2].account, None,
        "a frame whose key holds no account is listed with the rest"
    );
    let (first, next) = recent(None, PageRequest::first(2));
    assert_eq!(first, newest_first[..2]);
    let (rest, next) = recent(None, PageRequest::resume(next, 2));
    assert_eq!(
        rest,
        newest_first[2..4],
        "a page resumes where the last one stopped"
    );
    let (last, next) = recent(None, PageRequest::resume(next, 2));
    assert_eq!((last, next), (newest_first[4..].to_vec(), None));
    let (chat, next) = recent(Some("chat"), PageRequest::default());
    assert_eq!(chat, [row(&records[4]), row(&records[3]), row(&records[0])]);
    assert_eq!(next, None);
    assert_eq!(recent(Some("poll"), PageRequest::default()).0, []);
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
            ..MockHost::env(PROGRAM)
        },
        Env {
            height: 3,
            ..MockHost::env(PROGRAM).from_program("chat", Some(8))
        },
    ] {
        let refused = host.refused(|| ::program::hook::<Activity>(&host.exec(env), forged.clone()));
        assert_eq!(refused.code, code::UNAUTHORIZED);
    }
    let op =
        host.refused(|| ::program::execute::<Activity>(&host.exec(MockHost::env(PROGRAM)), &[]));
    assert_eq!(op.code, code::UNSUPPORTED);
}
