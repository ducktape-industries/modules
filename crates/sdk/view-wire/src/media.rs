//! The camera and the microphone in a view ([`crate::Node::Camera`],
//! [`crate::Node::Microphone`]): which device a piece opens, what the view
//! asks of it ([`MicCommand`]) and what the host tells back
//! ([`MediaState`]). No picture, sound or level crosses: the host captures,
//! paints and plays, and the view places the pieces and steers them. The
//! devices to pick from are methods (`camera.devices`,
//! `microphone.devices`), their types in [`crate::methods`].
use serde::{Deserialize, Serialize};

/// One thing the view asks of its microphone piece, applied once by the
/// host when the node's `revision` moves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicCommand {
    /// Record `seconds` from the microphone, then play the recording on the
    /// piece's `output`: the person hears themselves after they stop
    /// talking, with no feedback through the speakers. A recording or
    /// playback under way is replaced.
    Record { seconds: u8 },
    /// Ends a recording or its playback.
    Stop,
}

/// Where a camera or microphone piece's device is, as the host holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaStatus {
    /// Opening the device, or waiting on the person's answer to the
    /// system's permission prompt.
    #[default]
    Starting,
    /// Capturing: a camera's pictures are drawn, a microphone's level moves.
    Live,
    /// The person (or the system's policy) refused this app the device.
    Denied,
    /// Another program holds the device.
    Busy,
    /// The device went away while it was open (unplugged, its driver
    /// unloaded).
    Gone,
    /// There is no such device: none at all, or none by the piece's id.
    NoDevice,
    /// This host captures nothing: a build without camera and microphone
    /// support, or a platform it does not cover.
    Unsupported,
    /// Any other failure; [`MediaState::detail`] says what.
    Failed,
}

/// A microphone piece's recording, as [`MicCommand::Record`] runs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback {
    #[default]
    Idle,
    Recording,
    Playing,
}

/// A piece's device as the host holds it, sent as
/// [`crate::Event::Media`] whenever any of it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaState {
    pub status: MediaStatus,
    /// The name of the device the piece opened (the default one's own name
    /// when the piece names none), or empty while none is.
    pub device: String,
    /// A microphone piece's recording; a camera's is always `Idle`.
    pub playback: Playback,
    /// The host's words for a [`MediaStatus::Failed`], else empty: one
    /// line, at most 200 bytes.
    pub detail: String,
}

/// Whether `frame` carries a camera piece. A host refuses such a frame
/// from a view whose manifest does not declare
/// [`Capability::Camera`](crate::methods::Capability::Camera).
pub fn carries_camera(frame: &crate::Frame) -> bool {
    frame.carries(&|node| matches!(node, crate::Node::Camera { .. }))
}

/// Whether `frame` carries a microphone piece. A host refuses such a frame
/// from a view whose manifest does not declare
/// [`Capability::Microphone`](crate::methods::Capability::Microphone).
pub fn carries_microphone(frame: &crate::Frame) -> bool {
    frame.carries(&|node| matches!(node, crate::Node::Microphone { .. }))
}
