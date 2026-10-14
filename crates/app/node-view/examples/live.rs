//! `cargo run -p node-view --example live`: Nodes in the real app over a
//! fake node whose validators are the dev key and a node key on the dev
//! account's agent, beside a resident; the dev key votes, so the vote is
//! on screen with a promotion and a change of valset's code waiting.
use ducktape_view_guest::rig;
use module_registry::{Change, Entry, Scheduled};
use valset::{Motion, Op};

fn main() {
    rig::run(valset::MODULE, |net| {
        let me = net.me();
        net.submit(me, valset::MODULE, &Op::Vote(Motion::Door { open: true }))?;
        let enroll = |address: &str| Op::Enroll {
            address: Some(address.into()),
            invite: None,
        };
        // a validator's node key, on an agent the dev account operates
        let vps = net.agent(me, "validator vps")?;
        net.submit(vps, valset::MODULE, &enroll("10.0.0.3:4000"))?;
        let seat = Motion::Promote {
            key: net.key_of(vps),
        };
        net.submit(me, valset::MODULE, &Op::Vote(seat))?;
        let ada = net.person("ada");
        net.submit(ada, valset::MODULE, &enroll("10.0.0.2:4000"))?;
        // two validators now: each of the dev key's votes waits for the other
        let promote = Motion::Promote {
            key: net.key_of(ada),
        };
        net.submit(me, valset::MODULE, &Op::Vote(promote))?;
        let change = module_registry::Op::Schedule(Scheduled {
            height: net.height() + 300,
            change: Change::Set(Entry {
                program: valset::MODULE.into(),
                code: abi::BlobId::Sha256([0x7e; 32]),
                params: Vec::new(),
            }),
        });
        let registry = Motion::Registry {
            op: abi::encode(&change),
        };
        net.submit(me, valset::MODULE, &Op::Vote(registry))
            .map(drop)
    })
}
