//! The model records: an id the first account to set it owns, the wire the model speaks, the
//! egress service that reaches it, the vendor's name for it, and the context window harness
//! learns from the vendor. Only its owner changes or removes a record.
use ::program::{Error, ExecCtx, invalid, not_found, unauthorized};

use crate::state::MODELS;
use crate::{ModelRow, Window, Wire};

fn lowercase_name(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
}

fn checked(id: &str, service: &str, name: &str) -> Result<(), Error> {
    if !lowercase_name(id) {
        return Err(invalid(
            "a model id is lowercase letters, digits, -, _ and .",
        ));
    }
    let reserved = abi::role::egress::RESERVED.contains(&service);
    if !lowercase_name(service) || reserved {
        return Err(invalid(
            "the model is reached through a registered egress service, not http or relay",
        ));
    }
    let vendor_named = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._@:".contains(&byte));
    if !vendor_named {
        return Err(invalid(
            "a vendor's model name is letters, digits, -, ., _, @ and :",
        ));
    }
    Ok(())
}

pub(crate) fn set_model(
    ctx: &ExecCtx,
    id: String,
    wire: Wire,
    service: String,
    name: String,
) -> Result<(), Error> {
    let owner = ctx.sender_account()?;
    checked(&id, &service, &name)?;
    let existing = MODELS.get(ctx, &id)?;
    if let Some(row) = existing.as_ref().filter(|row| row.owner != owner) {
        return Err(unauthorized(format!(
            "model {id} is account {}'s to change",
            row.owner
        )));
    }
    let same_model = |row: &ModelRow| row.wire == wire && row.name == name;
    let window = existing
        .filter(same_model)
        .map_or(Window::Unasked, |row| row.window);
    let row = ModelRow {
        id: id.clone(),
        owner,
        wire,
        service,
        name,
        window,
    };
    MODELS.put(ctx, &id, &row);
    Ok(())
}

pub(crate) fn remove_model(ctx: &ExecCtx, id: String) -> Result<(), Error> {
    let owner = ctx.sender_account()?;
    let Some(row) = MODELS.get(ctx, &id)? else {
        return Err(not_found(format!("no model {id}")));
    };
    if row.owner != owner {
        return Err(unauthorized(format!(
            "model {id} is account {}'s to remove",
            row.owner
        )));
    }
    MODELS.remove(ctx, &id);
    Ok(())
}

pub(crate) fn learned(ctx: &ExecCtx, asked: &ModelRow, window: Window) -> Result<(), Error> {
    let Some(mut row) = MODELS.get(ctx, &asked.id)? else {
        return Ok(());
    };
    let still_asked = row.wire == asked.wire && row.name == asked.name;
    if still_asked {
        row.window = window;
        MODELS.put(ctx, &asked.id, &row);
    }
    Ok(())
}
