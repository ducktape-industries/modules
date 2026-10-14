//! A page over a walk of names or paths: the cursor is the last name or path the
//! previous page reached, bound to the listing and the snapshot it walked.
use ::program::{Error, QueryCtx, invalid};
use borsh::BorshSerialize;
use store::{Cursor, PageRequest, PageResponse};

pub(crate) struct Walk {
    pub after: Option<String>,
    pub limit: usize,
    scope: Vec<u8>,
    height: u64,
}

impl Walk {
    pub(crate) fn open(
        ctx: &QueryCtx,
        page: &PageRequest,
        scope: &impl BorshSerialize,
    ) -> Result<Walk, Error> {
        let scope = PageRequest::scope_of(scope);
        let after = page
            .open(&scope)?
            .map(|cursor| String::from_utf8(cursor.after))
            .transpose()
            .map_err(|_| invalid("a page cursor does not decode"))?;
        Ok(Walk {
            after,
            limit: page.limit() as usize,
            scope,
            height: ctx.env().height,
        })
    }

    pub(crate) fn page<T>(&self, items: Vec<T>, next: Option<String>) -> PageResponse<T> {
        let next = next.map(|after| {
            abi::encode(&Cursor {
                height: self.height,
                scope: self.scope.clone(),
                after: after.into_bytes(),
            })
        });
        PageResponse {
            height: self.height,
            items,
            next,
        }
    }
}
