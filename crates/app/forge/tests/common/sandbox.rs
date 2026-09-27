//! forge and chat on one [`MockChain`]: chat is the sibling forge queries,
//! and where its emissions land, in the frame that emitted them. The
//! chain's roster is identity's: each key the account it holds, the
//! harness keys ([`HOLDERS`](super::HOLDERS)) from the start, each module
//! its account ([`MODULES`]) and each agent its standing ([`agent`](MemorySandbox::agent)).

use abi::role::identity::{Category, Kind, Profile, Standing};
use guest::{Env, Error, MockChain, Origin, Principal};
use guest::{ExecCtx, MockHost, QueryCtx};

pub struct MemorySandbox {
    pub chain: MockChain,
    pub forge: MockHost,
    pub chat: MockHost,
}

/// The person every agent of the roster is managed by.
pub const AGENT_MANAGER: u64 = 11;

/// The account identity registered for each module.
pub const MODULES: [(&str, u64); 2] = [("forge", 900), ("chat", 901)];

/// The module whose account `number` is.
pub fn module_of(number: u64) -> Option<&'static str> {
    MODULES
        .iter()
        .find(|(_, account)| *account == number)
        .map(|(module, _)| *module)
}

impl Default for MemorySandbox {
    fn default() -> Self {
        let mut chain = MockChain::default();
        let forge = chain.seat::<forge::Forge>("forge");
        let chat = chain.seat::<chat::Chat>("chat");
        for (key, account) in super::HOLDERS {
            chain.hold(key, account);
        }
        for (module, number) in MODULES {
            chain.register(module, number);
        }
        MemorySandbox { chain, forge, chat }
    }
}

impl MemorySandbox {
    /// Seats `key` in `account`, as identity would.
    pub fn hold(&self, key: &[u8], account: u64) {
        self.chain.hold(key, account);
    }

    /// An agent [`AGENT_MANAGER`] manages, with its standing.
    pub fn agent(&self, number: u64, standing: Standing) {
        self.chain.profile(Profile {
            number,
            name: format!("account {number}"),
            kind: Kind::Managed {
                manager: AGENT_MANAGER,
                category: Category::Agent,
                standing,
            },
        });
    }

    /// forge's env in a block at `height`, called by the chain itself.
    fn env(&self, height: u64) -> Env {
        Env {
            height,
            time: super::TIME,
            ..MockHost::env("forge")
        }
    }

    /// A write to forge's host at `height`, signed by the system.
    pub fn exec(&self, height: u64) -> ExecCtx {
        self.forge.exec(self.env(height))
    }

    /// A read of forge's host at `height`.
    pub fn reads(&self, height: u64) -> QueryCtx {
        self.forge.query(Env {
            sender: None,
            ..self.env(height)
        })
    }

    /// Who `key` signs as: the account it holds, if any.
    pub fn principal(&self, key: &[u8]) -> Option<Principal> {
        let account = self.chain.roster().keys.get(key).copied();
        account.map(Principal::Account)
    }

    /// Runs one chat message as a frame of its own, signed by `origin`.
    pub fn chat_execute(
        &self,
        origin: Origin,
        height: u64,
        time: u64,
        msg: chat::Op,
    ) -> Result<(), Error> {
        self.chain.at(height, time);
        self.chain.submit(origin, "chat", &msg)?;
        Ok(())
    }

    pub fn chat_query(&self, q: chat::Query) -> Result<chat::Reply, Error> {
        self.chain.query("chat", &q)
    }

    /// forge's op signed by `origin` in a block at `height`, and what it
    /// emitted, in one frame as the kernel runs it: forge's output, or the
    /// refusal that failed the frame (which left every host as it was).
    pub fn frame(
        &self,
        origin: Origin,
        height: u64,
        time: u64,
        op: &forge::Op,
    ) -> Result<Vec<u8>, Error> {
        self.chain.at(height, time);
        self.chain.submit(origin, "forge", op)
    }

    pub fn blob_count(&self) -> usize {
        self.forge.borrow().blobs.len()
    }
}
