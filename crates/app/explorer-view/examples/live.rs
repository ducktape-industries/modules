//! `cargo run -p explorer-view --example live`: Explorer in the real app
//! over a fake node whose blocks carry a few people and chat posts, and
//! whose programs holds grants: the dev account maintains every program
//! and, as the one validator, votes; ada maintains chat and has a change
//! of it waiting, beside the vote's adding trade, which bob maintains.
//! `READER=none` shows the same network to a reader with no grant and no
//! vote.
use chat::{Block, Op, PostPolicy};
use programs::{Change, Grant, Scheduled, Scope};
use valset::{Motion, Valset};
use view::live;
use view::methods::Query;

fn main() {
    let unprivileged = std::env::var_os("READER").is_some_and(|reader| reader == "none");
    live::run("explorer", |net| {
        // told of every frame from here, as on a network: who did what
        net.hook::<activity::Activity>(activity::PROGRAM);
        net.seat::<chat::Chat>(chat::PROGRAM);
        let ada = net.person("ada");
        let bob = net.person("bob");
        net.submit(
            ada,
            chat::PROGRAM,
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
                chat::PROGRAM,
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
        let vote = |op: &programs::Op| {
            let motion = Motion::Programs {
                op: abi::encode(op),
            };
            net.submit(me, valset::PROGRAM, &valset::Op::Vote(motion))
        };
        let grant = |scope: Scope, account| programs::Op::Grant(Grant { scope, account });
        vote(&grant(Scope::Program("chat".into()), ada))?;
        vote(&grant(Scope::Program("trade".into()), bob))?;
        if !unprivileged {
            vote(&grant(Scope::All, me))?;
        }
        let kind = programs::CODE_KIND;
        let body = b"chat, the next release".to_vec();
        let code = net.hold(kind, &body)?;
        let at = |height: u64, change: Change| programs::Op::Schedule(Scheduled { height, change });
        let set = |program: &str| {
            Change::Set(programs::Entry {
                program: program.into(),
                code,
                params: Vec::new(),
            })
        };
        // ada's change by her grant; the vote's on another name, since the
        // vote's change to chat would take hers along
        net.submit(ada, programs::PROGRAM, &at(700, set(chat::PROGRAM)))?;
        vote(&at(900, set("trade")))?;
        if unprivileged {
            // the view hears the dev key vote for no one
            let validators: valset::Reply =
                net.query(valset::PROGRAM, &valset::Query::Validators)?;
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
