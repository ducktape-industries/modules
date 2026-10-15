//! The service loop beside a node: serve the configured services, then each time the provider
//! program writes, answer every request the chain elected this node for and expire every overdue
//! one, each once. Every op is a frame signed with the node's identity key.
use std::collections::BTreeSet;
use std::sync::Arc;

use commonware_cryptography::{Signer as _, ed25519};
use futures::StreamExt as _;
use host::Layer;
use node::Frame;
use noded::Client;
use provider::{BlobId, Op, PROGRAM, PageRequest, Query, Reply, RequestRow};
use tracing::{debug, info, warn};

use crate::call;
use crate::config::Config;

const TARGET: &str = "ducktape::provide";

pub struct Node {
    client: Client,
    key: ed25519::PrivateKey,
}

impl Node {
    pub fn new(client: Client, key: ed25519::PrivateKey) -> Node {
        Node { client, key }
    }

    fn identity(&self) -> Vec<u8> {
        self.key.public_key().as_ref().to_vec()
    }

    async fn frame(&self, payload: Vec<u8>) -> Result<Frame, String> {
        let status = self
            .client
            .status()
            .await
            .map_err(|error| error.to_string())?;
        let network = status.network.as_bytes();
        Ok(Frame::sign(
            &self.key,
            network,
            status.anchor(),
            None,
            PROGRAM,
            payload,
        ))
    }

    async fn submit(&self, op: &Op) -> Result<(), String> {
        let frame = self.frame(abi::encode(op)).await?;
        let receipt = self
            .client
            .submit(frame.encode())
            .await
            .map_err(|error| error.to_string())?;
        match receipt.outcome {
            abi::Outcome::Applied { .. } => Ok(()),
            abi::Outcome::Rejected(refusal) => {
                Err(format!("{}: {}", refusal.reason, refusal.sentence))
            }
        }
    }

    async fn ask(&self, query: &Query) -> Result<Reply, String> {
        let frame = self.frame(abi::encode(query)).await?;
        let answer = self
            .client
            .query(Layer::Confirmed, frame.encode())
            .await
            .map_err(|error| error.to_string())?;
        abi::decode(&answer).map_err(|refusal| refusal.sentence)
    }

    async fn rows(&self, query: impl Fn(PageRequest) -> Query) -> Result<Vec<RequestRow>, String> {
        let mut rows = Vec::new();
        let mut page = PageRequest::default();
        loop {
            let Reply::Requests(answered) = self.ask(&query(page)).await? else {
                return Err("provider answered a request page with something else".to_owned());
            };
            rows.extend(answered.items);
            let Some(after) = answered.next else {
                return Ok(rows);
            };
            page = PageRequest {
                after: Some(after),
                ..PageRequest::default()
            };
        }
    }

    async fn body(&self, parts: &[BlobId]) -> Result<Vec<u8>, String> {
        let found = self
            .client
            .blobs(parts)
            .await
            .map_err(|error| error.to_string())?;
        let mut body = Vec::new();
        for (part, bytes) in parts.iter().zip(found) {
            let Some(bytes) = bytes else {
                return Err(format!(
                    "part {} is not on this node",
                    abi::hex(part.digest())
                ));
            };
            body.extend(bytes);
        }
        Ok(body)
    }
}

pub fn fresh(tracked: &mut BTreeSet<u64>, rows: Vec<RequestRow>) -> Vec<RequestRow> {
    let open: BTreeSet<u64> = rows.iter().map(|row| row.id).collect();
    tracked.retain(|id| open.contains(id));
    rows.into_iter()
        .filter(|row| tracked.insert(row.id))
        .collect()
}

pub async fn run(node: Node, config: Config) -> Result<(), String> {
    let node = Arc::new(node);
    let config = Arc::new(config);
    let http = reqwest::Client::new();
    let mut writes = node
        .client
        .changes(PROGRAM)
        .await
        .map_err(|error| error.to_string())?;
    let services = config.services();
    node.submit(&Op::Serve {
        services: services.clone(),
    })
    .await?;
    info!(target: TARGET, event = "serving", ?services);
    let mut answering = BTreeSet::new();
    let mut expiring = BTreeSet::new();
    loop {
        let key = node.identity();
        let assigned = node
            .rows(|page| Query::Assigned {
                key: key.clone(),
                page,
            })
            .await?;
        for row in fresh(&mut answering, assigned) {
            tokio::spawn(answer(node.clone(), config.clone(), http.clone(), row));
        }
        let overdue = node.rows(|page| Query::Overdue { page }).await?;
        for row in fresh(&mut expiring, overdue) {
            tokio::spawn(expire(node.clone(), row.id));
        }
        match writes.next().await {
            Some(Ok(_)) => continue,
            Some(Err(error)) => return Err(error.to_string()),
            None => return Err("the node closed provider's change stream".to_owned()),
        }
    }
}

async fn answer(node: Arc<Node>, config: Arc<Config>, http: reqwest::Client, row: RequestRow) {
    debug!(target: TARGET, event = "answering", id = row.id, service = %row.service);
    let route = config.route(&row.service);
    let answer = match node.body(&row.parts).await {
        Ok(body) => call::answer(&http, route, &row.input, body).await,
        Err(reason) => provider::Answer::Failed(reason),
    };
    let op = Op::Answer { id: row.id, answer };
    if let Err(error) = node.submit(&op).await {
        warn!(target: TARGET, event = "answer_refused", id = row.id, %error);
    }
}

async fn expire(node: Arc<Node>, id: u64) {
    if let Err(error) = node.submit(&Op::Expire { id }).await {
        debug!(target: TARGET, event = "expire_refused", id, %error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u64) -> RequestRow {
        RequestRow {
            id,
            requester: "harness".into(),
            service: "anthropic".into(),
            input: Vec::new(),
            parts: Vec::new(),
            provider: vec![1; 32],
            made: 1,
            deadline: 65,
        }
    }

    #[test]
    fn each_request_is_taken_up_once_and_forgotten_when_it_closes() {
        let mut tracked = BTreeSet::new();
        let first = fresh(&mut tracked, vec![row(1), row(2)]);
        assert_eq!(first.iter().map(|row| row.id).collect::<Vec<_>>(), [1, 2]);
        let again = fresh(&mut tracked, vec![row(1), row(2), row(3)]);
        assert_eq!(again.iter().map(|row| row.id).collect::<Vec<_>>(), [3]);
        assert!(fresh(&mut tracked, vec![row(3)]).is_empty());
        assert_eq!(tracked, BTreeSet::from([3]));
    }
}
