use super::*;

fn every(host: &MockHost, page: PageRequest) -> (Vec<Frame>, Option<Vec<u8>>) {
    frames(ask(host, Query::Frames { page }))
}

#[test]
fn every_frame_reads_back_once_newest_first_whoever_signed_it() {
    let (host, records) = told();
    let (all, next) = every(&host, PageRequest::default());
    let newest_first: Vec<Frame> = records.iter().rev().map(frame).collect();
    assert_eq!(all, newest_first);
    assert_eq!(next, None);
}

#[test]
fn a_frame_told_out_of_order_still_reads_in_its_place_on_the_chain() {
    let host = MockHost::default();
    let records = [
        record(7, 1, ADA, Some(5), "chat"),
        record(2, 0, STRANGER, None, "identity"),
        record(7, 0, ADA_PHONE, Some(5), "forge"),
        record(11, 4, ADA, Some(5), "chat"),
    ];
    for record in &records {
        tell(&host, record);
    }
    let places: Vec<(u64, u32)> = every(&host, PageRequest::default())
        .0
        .iter()
        .map(|frame| (frame.height, frame.position))
        .collect();
    assert_eq!(places, [(11, 4), (7, 1), (7, 0), (2, 0)]);
}

#[test]
fn the_network_wide_pages_resume_and_cover_every_frame_once() {
    let (host, records) = told();
    let mut read = Vec::new();
    let mut page = PageRequest::first(2);
    loop {
        let (items, next) = every(&host, page);
        assert!(items.len() <= 2);
        read.extend(items);
        let Some(next) = next else {
            break;
        };
        page = PageRequest::resume(Some(next), 2);
    }
    let newest_first: Vec<Frame> = records.iter().rev().map(frame).collect();
    assert_eq!(read, newest_first);
}
