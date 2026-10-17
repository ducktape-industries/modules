//! A program's interface as one borsh schema: its op, query and reply under the named fields of
//! [`Interface`]. A founding packs it as the [`SECTION`] custom section of the program's code.
use borsh::BorshSchema;
use borsh::schema::{BorshSchemaContainer, Definition, Fields};

use crate::Program;

pub const SECTION: &str = "ducktape.schema";

#[derive(BorshSchema)]
pub struct Interface<O, Q, R> {
    pub op: O,
    pub query: Q,
    pub reply: R,
}

pub fn interface<P: Program>() -> BorshSchemaContainer
where
    P::Op: BorshSchema,
    P::Query: BorshSchema,
    P::Reply: BorshSchema,
{
    BorshSchemaContainer::for_type::<Interface<P::Op, P::Query, P::Reply>>()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roots<'s> {
    pub op: &'s str,
    pub query: &'s str,
    pub reply: &'s str,
}

pub fn roots(schema: &BorshSchemaContainer) -> Option<Roots<'_>> {
    let Some(Definition::Struct {
        fields: Fields::NamedFields(fields),
    }) = schema.get_definition(schema.declaration())
    else {
        return None;
    };
    let field = |name: &str| {
        fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, declaration)| declaration.as_str())
    };
    Some(Roots {
        op: field("op")?,
        query: field("query")?,
        reply: field("reply")?,
    })
}
