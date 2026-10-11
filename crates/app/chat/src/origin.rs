//! What the identity role says about who is asking: the role's profiles as
//! chat's views read them. The sender itself is the host's, read first thing in
//! [`Chat::execute`](crate::Chat).
use abi::role::identity as role;
use guest::{Error, QueryCtx, invalid, unexpected_reply};
use store::{PageRequest, PageResponse};

use crate::Profile;

/// One page of the identity role's profiles (asked of the module genesis
/// bound to the role), so a view links one module. The cursor is the last
/// account number, big-endian.
pub(crate) fn accounts(ctx: &QueryCtx, page: PageRequest) -> Result<PageResponse<Profile>, Error> {
    let after = page
        .after
        .as_deref()
        .map(|bytes| <[u8; 8]>::try_from(bytes).map(u64::from_be_bytes))
        .transpose()
        .map_err(|_| invalid("an account cursor is an account number"))?;
    let profiles = role::Query::Profiles {
        after,
        limit: page.limit() as u32,
    };
    let identity = &ctx.env().roles.identity;
    let role::Reply::Profiles { profiles, next } =
        ctx.query::<role::Query, role::Reply>(identity, &profiles)?
    else {
        return Err(unexpected_reply(identity, "Profiles", &"another reply"));
    };
    Ok(PageResponse {
        height: ctx.env().height,
        items: profiles,
        next: next.map(|number| number.to_be_bytes().to_vec()),
    })
}
