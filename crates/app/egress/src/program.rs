//! The program: every op and every query, each handed to its rule.
use ::program::{Error, ExecCtx, Program, QueryCtx};

use crate::rules::{
    answer, assign, assigned, expire, node, nodes, overdue, register, request, request_row, serve,
    service, services, serving,
};
use crate::{Op, PROGRAM, Query, Reply};

pub struct Egress;

impl Program for Egress {
    const NAME: &'static str = PROGRAM;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;

    fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
        match op {
            Op::Register { service: name } => register(ctx, name),
            Op::Assign {
                service: name,
                nodes: keys,
            } => assign(ctx, name, keys),
            Op::Serve { services: names } => serve(ctx, names),
            Op::Request(asked) => request(ctx, asked),
            Op::Answer { id, answer: given } => answer(ctx, id, given),
            Op::Expire { id } => expire(ctx, id),
        }
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        match query {
            Query::Assigned { key } => Ok(Reply::Requests(assigned(ctx, &key)?)),
            Query::Overdue => Ok(Reply::Requests(overdue(ctx)?)),
            Query::Node { key } => Ok(Reply::Node(node(ctx, &key)?)),
            Query::Request { id } => Ok(Reply::Request(request_row(ctx, id)?)),
            Query::Nodes { page } => Ok(Reply::Nodes(nodes(ctx, &page)?)),
            Query::Service { name } => Ok(Reply::Service(service(ctx, &name)?)),
            Query::Services { page } => Ok(Reply::Services(services(ctx, &page)?)),
            Query::Serving { service: name } => Ok(Reply::Serving(serving(ctx, &name)?)),
        }
    }
}

#[cfg(feature = "program")]
::program::export!(Egress);
