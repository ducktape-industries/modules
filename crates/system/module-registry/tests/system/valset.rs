use super::*;

#[test]
fn anyone_seats_members_and_the_next_epoch_reads_them() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let admitted = net
            .as_anyone(
                valset::MODULE,
                &valset::Op::Set(membership(3, valset::Role::Resident)),
            )
            .await;
        output_of(&admitted);
        assert_eq!(net.memberships().await.len(), 3);
        assert_eq!(net.validators().await.len(), 2);
        let valset::Reply::Members(members) =
            net.ask(valset::MODULE, &valset::Query::Members).await
        else {
            panic!()
        };
        assert_eq!(members.len(), 3);
        let promoted = net
            .as_anyone(
                valset::MODULE,
                &valset::Op::Set(membership(3, valset::Role::Validator)),
            )
            .await;
        output_of(&promoted);
        assert_eq!(net.validators().await.len(), 3);
        let epoch = (net.height + EPOCH_LENGTH) / EPOCH_LENGTH;
        while net.host.epoch_members(epoch).unwrap().is_none() {
            net.tick().await;
        }
        assert_eq!(net.host.epoch_members(epoch).unwrap().unwrap().len(), 3);
        for seed in [1, 2] {
            let removed = net
                .as_anyone(valset::MODULE, &valset::Op::Remove { key: public(seed) })
                .await;
            output_of(&removed);
        }
        assert_eq!(net.validators().await, vec![public(3)]);
        let last = net
            .as_anyone(valset::MODULE, &valset::Op::Remove { key: public(3) })
            .await;
        assert_eq!(refusal_of(&last), reason::WRONG_STATE);
        let demoted = net
            .as_anyone(
                valset::MODULE,
                &valset::Op::Set(membership(3, valset::Role::Resident)),
            )
            .await;
        assert_eq!(refusal_of(&demoted), reason::WRONG_STATE);
        let valset::Reply::Membership(Some(membership)) = net
            .ask(
                valset::MODULE,
                &valset::Query::Membership { key: public(3) },
            )
            .await
        else {
            panic!()
        };
        assert_eq!(membership.role, valset::Role::Validator);
    });
}

#[test]
fn memberships_resume_in_key_order_at_the_answering_height() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        net.tick().await;
        let mut after = None;
        let mut keys = Vec::new();
        loop {
            let valset::Reply::Memberships(reply) = net
                .ask(
                    valset::MODULE,
                    &valset::Query::Memberships {
                        page: PageRequest {
                            after,
                            limit: Some(1),
                        },
                    },
                )
                .await
            else {
                panic!()
            };
            assert_eq!(reply.height, net.height);
            assert_eq!(reply.items.len(), 1);
            keys.push(reply.items[0].key.clone());
            after = reply.next;
            if after.is_none() {
                break;
            }
        }
        let mut expected = vec![public(1), public(2)];
        expected.sort();
        assert_eq!(keys, expected);
    });
}

#[test]
fn a_resident_is_a_member_the_next_epoch_does_not_seat() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let admitted = net
            .as_anyone(
                valset::MODULE,
                &valset::Op::Set(membership(3, valset::Role::Resident)),
            )
            .await;
        output_of(&admitted);
        let epoch = (net.height + EPOCH_LENGTH) / EPOCH_LENGTH;
        while net.host.epoch_members(epoch).unwrap().is_none() {
            net.tick().await;
        }
        let members = net.host.epoch_members(epoch).unwrap().unwrap();
        assert_eq!(members.len(), 3);
        assert!(members.iter().any(|member| member.key == public(3)));
        let seated = net.host.epoch_validators(epoch).unwrap().unwrap();
        assert_eq!(seated, net.validators().await);
        assert_eq!(seated.len(), 2);
        assert!(!seated.contains(&public(3)));
    });
}

#[test]
fn a_newcomer_past_the_cap_is_refused_on_the_host() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        // the two founding validators fill a cap of two: the kernel handed
        // valset the cap it founded with
        let mut net = Net::found_with(context, dir.path(), Vec::new(), 2).await;
        let refused = net
            .as_anyone(
                valset::MODULE,
                &valset::Op::Set(membership(3, valset::Role::Resident)),
            )
            .await;
        assert_eq!(refusal_of(&refused), reason::CAPACITY);
        assert_eq!(net.memberships().await.len(), 2);
    });
}
