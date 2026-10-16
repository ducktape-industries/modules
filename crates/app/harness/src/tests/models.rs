use super::*;

fn model(chain: &MockChain, id: &str) -> Option<ModelRow> {
    let Reply::Model(row) = chain
        .query(PROGRAM, &Query::Model { id: id.into() })
        .unwrap()
    else {
        panic!()
    };
    row
}

#[test]
fn the_first_account_to_set_a_model_owns_it_and_only_it_changes_or_removes_it() {
    let world = world();
    let chain = &world.chain;
    let taken = chain.submit(signed(BO), PROGRAM, &set_model(MODEL, "claude-other"));
    assert_eq!(taken.unwrap_err().code, code::UNAUTHORIZED);
    let removed = chain.submit(signed(BO), PROGRAM, &Op::RemoveModel { id: MODEL.into() });
    assert_eq!(removed.unwrap_err().code, code::UNAUTHORIZED);
    chain
        .submit(signed(BO), PROGRAM, &set_model("bo-model", "claude-bo"))
        .unwrap();
    let Reply::Models(page) = chain
        .query(
            PROGRAM,
            &Query::Models {
                page: PageRequest::default(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    let owners: Vec<(String, AccountNumber)> = page
        .items
        .into_iter()
        .map(|row| (row.id, row.owner))
        .collect();
    assert_eq!(
        owners,
        [("bo-model".into(), world.bo), (MODEL.into(), world.ada)]
    );
    chain
        .submit(signed(ADA), PROGRAM, &Op::RemoveModel { id: MODEL.into() })
        .unwrap();
    assert_eq!(model(chain, MODEL), None);
    let gone = chain.submit(signed(ADA), PROGRAM, &Op::RemoveModel { id: MODEL.into() });
    assert_eq!(gone.unwrap_err().code, code::NOT_FOUND);
}

#[test]
fn a_model_record_is_checked_and_keeps_its_window_while_it_names_the_same_model() {
    let world = world();
    let chain = &world.chain;
    for refused in [
        set_model("Claude", MODEL),
        set_model("", MODEL),
        set_model("ok", "has space"),
        set_model("ok", ""),
    ] {
        let error = chain.submit(signed(ADA), PROGRAM, &refused).unwrap_err();
        assert_eq!(error.code, code::INVALID_INPUT, "{refused:?}");
    }
    for service in egress::RESERVED {
        let reserved = Op::SetModel {
            id: "ok".into(),
            wire: Wire::AnthropicMessages,
            service: service.into(),
            name: MODEL.into(),
        };
        let error = chain.submit(signed(ADA), PROGRAM, &reserved).unwrap_err();
        assert_eq!(error.code, code::INVALID_INPUT, "{service}");
    }
    world.opened("m1", "hello", Some(WINDOW));
    assert_eq!(model(chain, MODEL).unwrap().window, Window::Tokens(WINDOW));
    let moved = Op::SetModel {
        id: MODEL.into(),
        wire: Wire::AnthropicMessages,
        service: "elsewhere".into(),
        name: MODEL.into(),
    };
    chain.submit(signed(ADA), PROGRAM, &moved).unwrap();
    let row = model(chain, MODEL).unwrap();
    assert_eq!(
        (row.service.as_str(), row.window),
        ("elsewhere", Window::Tokens(WINDOW))
    );
    chain
        .submit(signed(ADA), PROGRAM, &set_model(MODEL, "claude-next"))
        .unwrap();
    assert_eq!(model(chain, MODEL).unwrap().window, Window::Unasked);
}

#[test]
fn a_model_op_reads_as_its_title() {
    let set = describe(&set_model(MODEL, "claude-test-1"));
    assert_eq!(set.title, "Set model · claude-test");
    let removed = describe(&Op::RemoveModel { id: MODEL.into() });
    assert_eq!(removed.title, "Remove model · claude-test");
}
