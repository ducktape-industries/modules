//! State transfer into a fresh root entity without replaying construction.
//! The bytes are the view's own serde as the wire's named MessagePack.
use crate::{App, Driver, View, slots, wire};
impl<V: View> Driver<V> {
    pub fn snapshot(&self) -> Result<Vec<u8>, String> {
        if slots::editor_pending(&self.app.inner.slots)
            || slots::editor_transferring(&self.app.inner.slots)
            || self.busy
            || self.host().pending_requests()
            || !crate::executor::snapshot_ready(&self.app.inner.tasks.borrow(), &self.host())
        {
            return Err("guest has pending work; snapshot after it settles".into());
        }
        self.entity.read(|view| wire::try_encode(view))
    }
    pub(crate) fn from_snapshot_in(app: App, bytes: &[u8]) -> Result<Self, String> {
        let view = wire::decode(bytes)?;
        Ok(Self::initialize_in(app, Some(view)))
    }
}
