//! The scheduler founded on ducktape's host: a program's scheduled run lands at the end of its
//! block as that program, and the scheduler keeps the run's receipt.
use super::*;

use scheduler::{Entry, EntryId, Op, Query, Reply};

impl Net {
    async fn entry(&self, id: EntryId) -> Option<Entry> {
        let Reply::Entry(entry) = self.ask(scheduler::PROGRAM, &Query::Entry { id }).await else {
            panic!()
        };
        entry
    }
}

#[test]
fn a_scheduled_run_lands_at_the_end_of_its_block_and_the_scheduler_keeps_its_receipt() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let due = net.height + 3;
        let later = vec![Step::Op(HostOp::Set {
            key: b"ran".to_vec(),
            value: b"yes".to_vec(),
        })];
        let schedule = Op::Schedule {
            height: due,
            target: "probe".into(),
            payload: abi::encode(&later),
        };
        let receipt = net.sent_by("probe", scheduler::PROGRAM, &schedule).await;
        let id: EntryId = abi::decode(output_of(&receipt)).unwrap();
        let ran = |net: &Net| {
            net.host
                .view(Layer::Confirmed)
                .get("probe", b"ran")
                .unwrap()
        };
        net.tick().await;
        assert_eq!(ran(&net), None);
        assert_eq!(net.entry(id).await.unwrap().receipt, None);
        let applied = net.tick().await;
        assert_eq!(net.height, due);
        assert!(applied.submissions.is_empty());
        assert_eq!(ran(&net), Some(b"yes".to_vec()));
        let entry = net.entry(id).await.expect("the entry is kept");
        assert_eq!(entry.scheduler, "probe");
        let receipt = entry.receipt.expect("the scheduler kept the receipt");
        assert_eq!(receipt.program, "probe");
        output_of(&receipt);
    });
}
