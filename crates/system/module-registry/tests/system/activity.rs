use super::*;

const ALICE: AccountNumber = 6;
const ALICE_KEY: u64 = 7;

fn apps() -> Vec<Founding> {
    vec![founding(activity::MODULE, &program("activity"))]
}

impl Net {
    async fn acted(&self, account: AccountNumber) -> Vec<activity::Frame> {
        let asked = activity::Query::Acted {
            account,
            page: PageRequest::default(),
        };
        let activity::Reply::Frames(page) = self.ask(activity::MODULE, &asked).await else {
            panic!()
        };
        page.items
    }

    async fn signed(&self, key: &[u8]) -> Vec<(u64, u32, bool)> {
        let asked = activity::Query::Signed {
            key: key.to_vec(),
            page: PageRequest::default(),
        };
        let activity::Reply::Frames(page) = self.ask(activity::MODULE, &asked).await else {
            panic!()
        };
        page.items
            .into_iter()
            .map(|frame| (frame.height, frame.position, frame.applied))
            .collect()
    }

    async fn rename(&mut self, name: &str) {
        let op = identity::Op::SetName {
            account: ALICE,
            name: name.into(),
        };
        self.apply(&public(ALICE_KEY), identity::MODULE, &op).await;
    }

    async fn schedule(&mut self, height: u64, change: module_registry::Change) {
        let op = module_registry::Op::Schedule(module_registry::Scheduled { height, change });
        output_of(&self.govern(&op).await);
    }
}

#[test]
fn a_founding_hook_is_told_of_every_frame_and_answers_by_account_and_key() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(
            context,
            dir.path(),
            apps(),
            vec![activity::MODULE],
            MEMBER_CAP,
        )
        .await;
        let create = identity::Op::Create {
            name: "Alice".into(),
            scheme: Scheme::Ed25519,
        };
        let created: AccountNumber = abi::decode(
            &net.apply(&public(ALICE_KEY), identity::MODULE, &create)
                .await,
        )
        .unwrap();
        assert_eq!(created, ALICE);
        net.rename("Ada").await;
        assert_eq!(
            net.refuse(&public(ALICE_KEY), identity::MODULE, &create)
                .await,
            reason::ALREADY_EXISTS
        );
        assert_eq!(
            net.signed(&public(ALICE_KEY)).await,
            [(3, 0, false), (2, 0, true), (1, 0, true)],
            "every frame the key signed, refused or not, newest first"
        );
        let acted = net.acted(ALICE).await;
        let heights: Vec<u64> = acted.iter().map(|frame| frame.height).collect();
        assert_eq!(
            heights,
            [3, 2],
            "the create ran before the key held an account"
        );
        assert_eq!(acted[1].target, identity::MODULE);
        assert_eq!(acted[1].time, TIME + 2000);
        let asked = activity::Query::Used {
            page: PageRequest::default(),
        };
        let activity::Reply::Used(used) = net.ask(activity::MODULE, &asked).await else {
            panic!()
        };
        assert_eq!(
            used.items,
            [activity::Used {
                account: ALICE,
                program: identity::MODULE.into(),
                frames: 2,
            }]
        );
    });
}

#[test]
fn a_hook_the_registry_schedules_is_told_from_its_height_until_it_is_removed() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(context, dir.path(), apps(), Vec::new(), MEMBER_CAP).await;
        let create = identity::Op::Create {
            name: "Alice".into(),
            scheme: Scheme::Ed25519,
        };
        net.apply(&public(ALICE_KEY), identity::MODULE, &create)
            .await;
        let hooked_at = net.height + 3;
        net.schedule(
            hooked_at,
            module_registry::Change::SetHook(activity::MODULE.into()),
        )
        .await;
        net.rename("Before").await;
        assert_eq!(net.height, hooked_at - 1);
        assert_eq!(net.signed(&public(ALICE_KEY)).await, []);
        net.rename("Hooked").await;
        assert_eq!(net.signed(&public(ALICE_KEY)).await, [(hooked_at, 0, true)]);
        let unhooked_at = net.height + 2;
        net.schedule(
            unhooked_at,
            module_registry::Change::RemoveHook(activity::MODULE.into()),
        )
        .await;
        assert_eq!(net.height, unhooked_at - 1);
        net.rename("Unhooked").await;
        let votes = net.signed(&public(FOUNDERS[1])).await;
        assert_eq!(
            votes,
            [(unhooked_at - 1, 1, true)],
            "the vote that hooked it went unheard, the one that unhooks it is heard"
        );
        assert_eq!(net.signed(&public(ALICE_KEY)).await, [(hooked_at, 0, true)]);
    });
}
