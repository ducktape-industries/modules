//! What each op checks and writes: an object's relations replaced at a later revision,
//! the difference recorded as changes, and every change told to every subscriber.
use std::collections::{BTreeMap, BTreeSet};

use ::program::{Error, ExecCtx, QueryCtx, Reply, invalid, stale};

use crate::state::{CHANGES, OBJECTS, OF, ObjectKey, SEQ, SUBSCRIBERS, TO};
use crate::{
    AccountNumber, Actor, Change, Kind, MAX_BATCH, MAX_DELIVER_BYTES, MAX_DETAIL_BYTES,
    MAX_NAME_BYTES, MAX_RELATIONS, MAX_SUBSCRIBERS, PageRequest, PageResponse, ProgramId, Reason,
    Relation, Relations, Source, Transfer, Update,
};

type Held = BTreeMap<(AccountNumber, Reason), Vec<u8>>;

fn object_key(source: &Source) -> ObjectKey {
    (
        source.program.clone(),
        source.object.kind.clone(),
        source.object.id.clone(),
    )
}

fn named(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_NAME_BYTES && !text.contains('/')
}

fn well_formed(update: &Update) -> Result<(), Error> {
    let object_named = named(&update.object.kind) && named(&update.object.id);
    if !object_named {
        return Err(invalid(format!(
            "an object has a kind and an id of 1 to {MAX_NAME_BYTES} bytes, with no '/'"
        )));
    }
    let actor_named = match &update.actor {
        Actor::Account(number) => *number != 0,
        Actor::Key(key) => !key.is_empty(),
        Actor::Program(program) => !program.is_empty(),
        Actor::System => true,
    };
    if !actor_named {
        return Err(invalid(
            "an actor is a live account, a key, a program or the system",
        ));
    }
    if update.relations.len() > MAX_RELATIONS {
        return Err(invalid(format!(
            "an object holds at most {MAX_RELATIONS} relations"
        )));
    }
    let mut seen = BTreeSet::new();
    for relation in &update.relations {
        if relation.recipient == 0 {
            return Err(invalid("a recipient is a live account"));
        }
        let unnamed = matches!(&relation.reason, Reason::Defined(name) if name.is_empty() || name.len() > MAX_NAME_BYTES);
        if unnamed {
            return Err(invalid(format!(
                "a defined reason has a name of 1 to {MAX_NAME_BYTES} bytes"
            )));
        }
        if relation.detail.len() > MAX_DETAIL_BYTES {
            return Err(invalid(format!(
                "a relation's detail is at most {MAX_DETAIL_BYTES} bytes"
            )));
        }
        let distinct = seen.insert((relation.recipient, relation.reason.clone()));
        if !distinct {
            return Err(invalid(format!(
                "account {} relates twice for {:?}",
                relation.recipient, relation.reason
            )));
        }
    }
    Ok(())
}

fn held(relations: &[Relation]) -> Held {
    relations
        .iter()
        .map(|relation| {
            (
                (relation.recipient, relation.reason.clone()),
                relation.detail.clone(),
            )
        })
        .collect()
}

fn kinds(
    before: &Held,
    after: &Held,
    transfers: &[Transfer],
) -> Result<BTreeMap<(AccountNumber, Reason), Kind>, Error> {
    let mut kinds = BTreeMap::new();
    for key in after.keys().filter(|key| !before.contains_key(*key)) {
        kinds.insert(key.clone(), Kind::Added);
    }
    for key in before.keys().filter(|key| !after.contains_key(*key)) {
        kinds.insert(key.clone(), Kind::Withdrawn);
    }
    for Transfer { reason, from, to } in transfers {
        let withdrawn = kinds.get(&(*from, reason.clone())) == Some(&Kind::Withdrawn);
        let added = kinds.get(&(*to, reason.clone())) == Some(&Kind::Added);
        let matches_the_diff = withdrawn && added;
        if !matches_the_diff {
            return Err(invalid(format!(
                "the transfer of {reason:?} from {from} to {to} matches no withdrawal and addition"
            )));
        }
        kinds.insert((*from, reason.clone()), Kind::TransferredOut { to: *to });
        kinds.insert((*to, reason.clone()), Kind::TransferredIn { from: *from });
    }
    Ok(kinds)
}

pub(crate) fn attribute(ctx: &ExecCtx, updates: Vec<Update>) -> Result<(), Error> {
    let program = ctx.env().sending_program()?;
    if updates.len() > MAX_BATCH {
        return Err(invalid(format!(
            "a batch attributes at most {MAX_BATCH} objects"
        )));
    }
    let subscribers = SUBSCRIBERS.all(ctx)?;
    for update in updates {
        attribute_one(ctx, &program, &subscribers, update)?;
    }
    Ok(())
}

fn attribute_one(
    ctx: &ExecCtx,
    program: &ProgramId,
    subscribers: &[(ProgramId, Vec<u8>)],
    update: Update,
) -> Result<(), Error> {
    well_formed(&update)?;
    let source = Source {
        program: program.clone(),
        object: update.object.clone(),
    };
    let key = object_key(&source);
    let previous = OBJECTS.get(ctx, &key)?;
    let reported = previous
        .as_ref()
        .is_some_and(|previous| update.revision <= previous.revision);
    if reported {
        return Err(stale(format!(
            "{program}/{}/{} was already reported at revision {}",
            source.object.kind, source.object.id, update.revision
        )));
    }
    let before = previous
        .map(|previous| held(&previous.relations))
        .unwrap_or_default();
    let after = held(&update.relations);
    let kinds = kinds(&before, &after, &update.transfers)?;
    let mut relations = update.relations;
    relations.sort_by(|a, b| (a.recipient, &a.reason).cmp(&(b.recipient, &b.reason)));
    let row = Relations {
        source: source.clone(),
        revision: update.revision,
        relations,
    };
    OBJECTS.put(ctx, &key, &row);
    let mut seq = SEQ.get(ctx)?.unwrap_or(0);
    for ((recipient, reason), kind) in kinds {
        seq += 1;
        let detail = after
            .get(&(recipient, reason.clone()))
            .cloned()
            .unwrap_or_default();
        let change = Change {
            seq,
            source: source.clone(),
            revision: update.revision,
            recipient,
            reason,
            kind,
            detail,
            actor: update.actor.clone(),
            cause: ctx.env().cause.clone(),
            height: ctx.env().height,
        };
        CHANGES.put(ctx, &seq, &change);
        TO.insert(ctx, &(recipient, seq));
        OF.insert(ctx, &(key.0.clone(), key.1.clone(), key.2.clone(), seq));
        for (subscriber, deliver) in subscribers {
            ctx.emit_raw(
                subscriber.clone(),
                Change::payload(deliver, &change),
                Reply::Wanted,
            );
        }
    }
    SEQ.put(ctx, &seq);
    Ok(())
}

pub(crate) fn subscribe(ctx: &ExecCtx, deliver: Vec<u8>) -> Result<(), Error> {
    let program = ctx.env().sending_program()?;
    if deliver.len() > MAX_DELIVER_BYTES {
        return Err(invalid(format!(
            "a delivery prefix is at most {MAX_DELIVER_BYTES} bytes"
        )));
    }
    let already = SUBSCRIBERS.has(ctx, &program);
    let full = SUBSCRIBERS.all(ctx)?.len() >= MAX_SUBSCRIBERS;
    if !already && full {
        return Err(::program::capacity(format!(
            "attribution tells at most {MAX_SUBSCRIBERS} programs"
        )));
    }
    SUBSCRIBERS.put(ctx, &program, &deliver);
    Ok(())
}

pub(crate) fn relations(ctx: &QueryCtx, source: &Source) -> Result<Option<Relations>, Error> {
    OBJECTS.get(ctx, &object_key(source))
}

pub(crate) fn changes(ctx: &QueryCtx, page: &PageRequest) -> Result<PageResponse<Change>, Error> {
    Ok(CHANGES.range(ctx, page)?.map(|(_, change)| change))
}

fn change(ctx: &QueryCtx, seq: u64) -> Result<Change, Error> {
    CHANGES
        .get(ctx, &seq)?
        .ok_or_else(|| ::program::corrupt("change/", &seq.to_be_bytes(), "indexed but missing"))
}

pub(crate) fn changes_to(
    ctx: &QueryCtx,
    recipient: AccountNumber,
    page: &PageRequest,
) -> Result<PageResponse<Change>, Error> {
    TO.range_of(ctx, &recipient, page)?
        .try_map(|(_, seq)| change(ctx, seq))
}

pub(crate) fn changes_of(
    ctx: &QueryCtx,
    source: &Source,
    page: &PageRequest,
) -> Result<PageResponse<Change>, Error> {
    OF.range_of(ctx, &object_key(source), page)?
        .try_map(|(_, _, _, seq)| change(ctx, seq))
}

pub(crate) fn subscribers(ctx: &QueryCtx) -> Result<Vec<ProgramId>, Error> {
    Ok(SUBSCRIBERS
        .all(ctx)?
        .into_iter()
        .map(|(program, _)| program)
        .collect())
}
