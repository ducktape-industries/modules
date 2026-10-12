//! `cargo run -p explorer-view --example live`: Explorer in the real app
//! over a fake node whose blocks carry a few people and chat posts, and
//! whose registry holds grants: the dev account maintains every module
//! and, as the one validator, votes; ada maintains chat and has a change
//! of it waiting, beside one the vote made. `READER=none` shows the same
//! network to a reader with no grant and no vote.
use chat::{Block, Op, PostPolicy};
use ducktape_view_guest::live;
use ducktape_view_guest::methods::Query;
use module_registry::{Change, Grant, Scheduled, Scope};
use valset::{Motion, Valset};

fn main() {
    let unprivileged = std::env::var_os("READER").is_some_and(|reader| reader == "none");
    live::run("explorer", |net| {
        net.seat::<chat::Chat>(chat::MODULE);
        let ada = net.person("ada");
        let bob = net.person("bob");
        net.submit(
            ada,
            chat::MODULE,
            &Op::CreateChannel {
                channel_id: "general".into(),
                name: "General".into(),
                post_policy: PostPolicy::Open,
            },
        )?;
        for (seq, (who, text)) in [(ada, "first"), (bob, "second"), (ada, "third")]
            .into_iter()
            .enumerate()
        {
            net.submit(
                who,
                chat::MODULE,
                &Op::PostMessage {
                    channel_id: "general".into(),
                    message_id: format!("m{seq}"),
                    blocks: vec![Block::paragraph(text)],
                    thread: None,
                },
            )?;
        }
        // the dev key is the one validator: its vote enacts at once
        let me = net.me();
        let vote = |op: &module_registry::Op| {
            let motion = Motion::Registry {
                op: abi::encode(op),
            };
            net.submit(me, valset::MODULE, &valset::Op::Vote(motion))
        };
        let grant = |scope: Scope, account| module_registry::Op::Grant(Grant { scope, account });
        vote(&grant(Scope::Module("chat".into()), ada))?;
        vote(&grant(Scope::Module("trade".into()), bob))?;
        if !unprivileged {
            vote(&grant(Scope::All, me))?;
        }
        let code = net.submit(
            me,
            module_registry::MODULE,
            &module_registry::Op::Publish {
                body: b"chat, the next release".to_vec(),
            },
        )?;
        let code: abi::BlobId = abi::decode(&code).expect("a blob id");
        let at = |height: u64, change: Change| {
            module_registry::Op::Schedule(Scheduled { height, change })
        };
        let set = |code| {
            Change::Set(module_registry::Entry {
                program: chat::MODULE.into(),
                code,
                params: Vec::new(),
            })
        };
        // ada's change by her grant, then the vote's
        net.submit(ada, module_registry::MODULE, &at(700, set(code)))?;
        vote(&at(900, set(code)))?;
        if unprivileged {
            // the view hears the dev key vote for no one
            let validators: valset::Reply =
                net.query(valset::MODULE, &valset::Query::Validators)?;
            net.handle::<Query<Valset>>(move |query| {
                Ok(match query {
                    valset::Query::Voices { .. } => valset::Reply::Voices(Vec::new()),
                    _ => validators.clone(),
                })
            });
        }
        Ok(())
    })
}
