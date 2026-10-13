use super::*;

#[test]
fn a_published_program_is_scheduled_by_the_validators_vote_and_seated_at_its_height() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let output = net
            .apply(
                &public(7),
                module_registry::MODULE,
                &module_registry::Op::Publish {
                    body: program("identity"),
                },
            )
            .await;
        let code: BlobId = abi::decode(&output).unwrap();
        let entry = module_registry::Entry {
            program: "identity2".into(),
            code,
            params: Vec::new(),
        };
        let unpublished = net
            .govern(&module_registry::Op::Schedule(module_registry::Scheduled {
                height: net.height + 3,
                change: module_registry::Change::Set(module_registry::Entry {
                    program: "ghost".into(),
                    code: BlobId::Sha256([9; 32]),
                    params: Vec::new(),
                }),
            }))
            .await;
        assert_eq!(refusal_of(&unpublished), reason::NOT_FOUND);
        // a change at or below the block is refused at the first vote, as
        // the registry would refuse it at enactment
        let past = module_registry::Op::Schedule(module_registry::Scheduled {
            height: net.height,
            change: module_registry::Change::Set(entry.clone()),
        });
        let vote = valset::Op::Vote(valset::Motion::Registry {
            op: abi::encode(&past),
        });
        let refused = net
            .refuse(&public(FOUNDERS[0]), valset::MODULE, &vote)
            .await;
        assert_eq!(refused, reason::INVALID_INPUT);
        let lands_at = net.height + 6;
        let scheduled = net
            .govern(&module_registry::Op::Schedule(module_registry::Scheduled {
                height: lands_at,
                change: module_registry::Change::Set(entry.clone()),
            }))
            .await;
        output_of(&scheduled);
        let taken = net
            .govern(&module_registry::Op::Schedule(module_registry::Scheduled {
                height: lands_at,
                change: module_registry::Change::Remove("identity2".into()),
            }))
            .await;
        assert_eq!(refusal_of(&taken), reason::ALREADY_EXISTS);
        let module_registry::Reply::Scheduled(pending) = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::Scheduled {
                    page: PageRequest::default(),
                },
            )
            .await
        else {
            panic!()
        };
        assert_eq!(pending.height, net.height);
        assert_eq!(pending.next, None);
        let pending = pending.items;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].height, lands_at);
        assert_eq!(
            pending[0].change,
            module_registry::Change::Set(entry.clone())
        );
        let module_registry::Reply::Programs(later) = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::At(lands_at),
            )
            .await
        else {
            panic!()
        };
        assert!(later.iter().any(|entry| entry.program == "identity2"));
        while net.height + 1 < lands_at {
            let applied = net.tick().await;
            assert!(applied.admissions.is_empty());
        }
        let applied = net.tick().await;
        assert_eq!(applied.height, lands_at);
        // identity giving identity2 its account, then the admission: a
        // program has its account before its init runs
        let admitted: Vec<&str> = applied
            .admissions
            .iter()
            .map(|receipt| receipt.program.as_str())
            .collect();
        assert_eq!(admitted, [identity::MODULE, "identity2"]);
        assert!(net.host.programs().unwrap().contains_key("identity2"));
        // no op has run on the registry since the schedule: what a client
        // lists is answered at its height all the same
        let module_registry::Reply::Roster {
            height, programs, ..
        } = net
            .ask(module_registry::MODULE, &module_registry::Query::Roster)
            .await
        else {
            panic!()
        };
        assert_eq!(height, net.height);
        assert!(programs.iter().any(|entry| entry.program == "identity2"));
        net.apply(
            &public(7),
            "identity2",
            &identity::Op::Create {
                name: "Seven".into(),
                scheme: Scheme::Ed25519,
            },
        )
        .await;
        let module_registry::Reply::Program {
            height,
            entry: Some(seated),
        } = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::Program("identity2".into()),
            )
            .await
        else {
            panic!()
        };
        assert_eq!(height, net.height);
        assert_eq!(seated, entry);
        let removal_at = net.height + 5;
        let removal = net
            .govern(&module_registry::Op::Schedule(module_registry::Scheduled {
                height: removal_at,
                change: module_registry::Change::Remove("identity2".into()),
            }))
            .await;
        output_of(&removal);
        let cancelled = net
            .govern(&module_registry::Op::Cancel {
                height: removal_at,
                program: "identity2".into(),
            })
            .await;
        output_of(&cancelled);
        net.ticks(3).await;
        assert!(net.host.programs().unwrap().contains_key("identity2"));
        let removal_at = net.height + 3;
        let removal = net
            .govern(&module_registry::Op::Schedule(module_registry::Scheduled {
                height: removal_at,
                change: module_registry::Change::Remove("identity2".into()),
            }))
            .await;
        output_of(&removal);
        net.ticks(2).await;
        assert!(!net.host.programs().unwrap().contains_key("identity2"));
    });
}

#[test]
fn schedule_pages_and_missing_programs_report_the_answering_height() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let output = net
            .apply(
                &public(7),
                module_registry::MODULE,
                &module_registry::Op::Publish {
                    body: program("identity"),
                },
            )
            .await;
        let code: BlobId = abi::decode(&output).unwrap();
        for program in ["z", "a", "m"] {
            let receipt = net
                .govern(&module_registry::Op::Schedule(module_registry::Scheduled {
                    height: 100,
                    change: module_registry::Change::Set(module_registry::Entry {
                        program: program.into(),
                        code,
                        params: Vec::new(),
                    }),
                }))
                .await;
            output_of(&receipt);
        }
        let mut after = None;
        let mut programs = Vec::new();
        loop {
            let module_registry::Reply::Scheduled(reply) = net
                .ask(
                    module_registry::MODULE,
                    &module_registry::Query::Scheduled {
                        page: PageRequest {
                            after,
                            limit: Some(2),
                        },
                    },
                )
                .await
            else {
                panic!()
            };
            assert_eq!(reply.height, net.height);
            assert!(reply.items.len() <= 2);
            for scheduled in reply.items {
                assert_eq!(scheduled.height, 100);
                programs.push(scheduled.change.program().to_owned());
            }
            after = reply.next;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(programs, ["a", "m", "z"]);
        let reply: module_registry::Reply = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::Program("missing".into()),
            )
            .await;
        assert_eq!(
            reply,
            module_registry::Reply::Program {
                height: net.height,
                entry: None
            }
        );
    });
}

#[test]
fn a_schedule_from_a_key_or_another_module_is_refused() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let output = net
            .apply(
                &public(7),
                module_registry::MODULE,
                &module_registry::Op::Publish {
                    body: program("identity"),
                },
            )
            .await;
        let code: BlobId = abi::decode(&output).unwrap();
        let schedule = module_registry::Op::Schedule(module_registry::Scheduled {
            height: net.height + 5,
            change: module_registry::Change::Set(module_registry::Entry {
                program: "identity2".into(),
                code,
                params: Vec::new(),
            }),
        });
        for signer in [public(1), public(7)] {
            let refused = net
                .refuse(&signer, module_registry::MODULE, &schedule)
                .await;
            assert_eq!(refused, reason::UNAUTHORIZED);
        }
        let by_module = net.as_anyone(module_registry::MODULE, &schedule).await;
        assert_eq!(refusal_of(&by_module), reason::UNAUTHORIZED);
        let module_registry::Reply::Scheduled(pending) = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::Scheduled {
                    page: PageRequest::default(),
                },
            )
            .await
        else {
            panic!()
        };
        assert!(pending.items.is_empty());
        let enacted = net.govern(&schedule).await;
        let voted: valset::Voted = abi::decode(output_of(&enacted)).unwrap();
        assert_eq!(voted, valset::Voted::Enacted);
        let scheduled = enacted
            .nested
            .iter()
            .find(|receipt| receipt.program == module_registry::MODULE)
            .unwrap();
        output_of(scheduled);
    });
}

/// The first grant, by the validators' vote; then the granted device's
/// account schedules an upgrade with no vote, the schedule says whose it
/// is, and it lands `NOTICE` blocks on.
#[test]
fn a_maintainer_the_vote_granted_upgrades_a_module_with_no_vote() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let create = identity::Op::Create {
            name: "Seven".into(),
            scheme: Scheme::Ed25519,
        };
        let output = net.apply(&public(7), identity::MODULE, &create).await;
        let seven: AccountNumber = abi::decode(&output).unwrap();
        let output = net
            .apply(
                &public(7),
                module_registry::MODULE,
                &module_registry::Op::Publish {
                    body: program("chat"),
                },
            )
            .await;
        let code: BlobId = abi::decode(&output).unwrap();
        let upgrade = |height| {
            module_registry::Op::Schedule(module_registry::Scheduled {
                height,
                change: module_registry::Change::Set(module_registry::Entry {
                    program: "probe".into(),
                    code,
                    params: Vec::new(),
                }),
            })
        };
        // the frame lands in the next block
        let lands_at = net.height + 1 + module_registry::NOTICE;
        let refused = net
            .refuse(&public(7), module_registry::MODULE, &upgrade(lands_at))
            .await;
        assert_eq!(refused, reason::UNAUTHORIZED, "no grant yet");
        let granted = net
            .govern(&module_registry::Op::Grant(module_registry::Grant {
                scope: module_registry::Scope::Module("probe".into()),
                account: seven,
            }))
            .await;
        output_of(&granted);
        let lands_at = net.height + 1 + module_registry::NOTICE;
        net.apply(&public(7), module_registry::MODULE, &upgrade(lands_at))
            .await;
        let module_registry::Reply::Scheduled(pending) = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::Scheduled {
                    page: PageRequest::default(),
                },
            )
            .await
        else {
            panic!()
        };
        let module_registry::Op::Schedule(scheduled) = upgrade(lands_at) else {
            unreachable!()
        };
        assert_eq!(
            pending.items,
            [module_registry::Pending {
                height: lands_at,
                change: scheduled.change,
                by: Some(seven),
            }]
        );
        let running = |net: &Net| net.host.programs().unwrap()["probe"];
        let before = running(&net);
        net.ticks(lands_at - 1 - net.height).await;
        assert_eq!(running(&net), before, "a block before");
        net.tick().await;
        assert_eq!(running(&net), code);
    });
}

/// Through valset's enactment: the vote's change at the very key a
/// maintainer's change holds takes that change along instead of failing
/// the vote, and the vote's cancel of a change already gone is done.
#[test]
fn the_vote_overrules_a_maintainers_change_at_its_own_key() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found(context, dir.path()).await;
        let create = identity::Op::Create {
            name: "Seven".into(),
            scheme: Scheme::Ed25519,
        };
        let output = net.apply(&public(7), identity::MODULE, &create).await;
        let seven: AccountNumber = abi::decode(&output).unwrap();
        let output = net
            .apply(
                &public(7),
                module_registry::MODULE,
                &module_registry::Op::Publish {
                    body: program("chat"),
                },
            )
            .await;
        let code: BlobId = abi::decode(&output).unwrap();
        let granted = net
            .govern(&module_registry::Op::Grant(module_registry::Grant {
                scope: module_registry::Scope::Module("probe".into()),
                account: seven,
            }))
            .await;
        output_of(&granted);
        let lands_at = net.height + 1 + module_registry::NOTICE;
        let change = module_registry::Change::Set(module_registry::Entry {
            program: "probe".into(),
            code,
            params: Vec::new(),
        });
        let at = |change: &module_registry::Change| {
            module_registry::Op::Schedule(module_registry::Scheduled {
                height: lands_at,
                change: change.clone(),
            })
        };
        net.apply(&public(7), module_registry::MODULE, &at(&change))
            .await;
        let overruled = net.govern(&at(&change)).await;
        output_of(&overruled);
        let module_registry::Reply::Scheduled(pending) = net
            .ask(
                module_registry::MODULE,
                &module_registry::Query::Scheduled {
                    page: PageRequest::default(),
                },
            )
            .await
        else {
            panic!()
        };
        assert_eq!(
            pending.items,
            [module_registry::Pending {
                height: lands_at,
                change,
                by: None,
            }]
        );
        let gone = net
            .govern(&module_registry::Op::Cancel {
                height: lands_at + 1,
                program: "probe".into(),
            })
            .await;
        output_of(&gone);
    });
}
