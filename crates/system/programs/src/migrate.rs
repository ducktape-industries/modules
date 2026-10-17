//! The one-time move of a network founded before programs declared their roles: the vote
//! copies the kernel's role table into this program's. Deleted once every network ran it.
use ::program::{Error, ExecCtx, invalid};

use crate::rules::ROLES;

pub(crate) fn migrate(ctx: &ExecCtx) -> Result<(), Error> {
    let env = ctx.env();
    env.authority()?;
    if let Some(role) = env.roles.unfilled() {
        return Err(invalid(format!(
            "the kernel's table leaves the {role} role unfilled: bind it before the move"
        )));
    }
    for (role, program) in env.roles.iter() {
        ROLES.put(ctx, &role.to_owned(), program);
    }
    Ok(())
}
