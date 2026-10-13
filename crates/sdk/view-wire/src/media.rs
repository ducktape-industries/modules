//! The camera and the microphone in a view ([`crate::Node::Camera`],
//! [`crate::Node::Microphone`]): which device a piece opens, what the view
//! asks of it ([`MicCommand`]) and what the host tells back
//! ([`MediaState`]). No picture, sound or level crosses: the host captures,
//! paints and plays, and the view places the pieces and steers them. The
//! devices to pick from are methods (`camera.devices`,
//! `microphone.devices`), their types in [`crate::methods`].
use serde::{Deserialize, Serialize};

/// The longest device id a piece names, in bytes. A longer one is no id:
/// the host opens the default device for it.
pub const MAX_DEVICE_BYTES: usize = 256;
/// The most commands one frame carries for one microphone piece.
pub const MAX_MIC_COMMANDS: usize = 4;
/// The longest recording a [`MicCommand::Record`] asks for, in seconds.
pub const MAX_RECORD_SECONDS: u8 = 10;

/// One thing the view asks of its microphone piece, applied once by the
/// host when the node's `revision` moves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicCommand {
    /// Record `seconds` (1 to [`MAX_RECORD_SECONDS`]) from the microphone,
    /// then play the recording on the piece's `output`: the person hears
    /// themselves after they stop talking, with no feedback through the
    /// speakers. A recording or playback under way is replaced.
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

pub(crate) fn decode_commands<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<MicCommand>, D::Error> {
    crate::bounded_vec(
        d,
        MAX_MIC_COMMANDS,
        "more microphone commands than the host applies",
    )
}

/// A device id inside the bound, or none: one cut short names another
/// device.
pub(crate) fn sanitize_device(device: &mut Option<String>, budgets: &mut crate::Budgets) {
    if device
        .as_ref()
        .is_some_and(|id| id.len() > MAX_DEVICE_BYTES)
    {
        *device = None;
        budgets.cut(|cuts| &mut cuts.strings, 1);
    }
}

/// A microphone piece's commands inside the frame's bounds: at most
/// [`MAX_MIC_COMMANDS`], each recording 1 to [`MAX_RECORD_SECONDS`] long.
pub(crate) fn sanitize_commands(commands: &mut Vec<MicCommand>) {
    commands.truncate(MAX_MIC_COMMANDS);
    for command in commands {
        if let MicCommand::Record { seconds } = command {
            *seconds = (*seconds).clamp(1, MAX_RECORD_SECONDS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementIdWire, Node, StyleId};

    /// A device id past the bound is no id (the default device), never one
    /// cut short; commands are bounded and a recording's length clamped.
    #[test]
    fn a_long_device_is_none_and_commands_are_bounded() {
        let long = "d".repeat(MAX_DEVICE_BYTES + 1);
        let mut node = Node::Microphone {
            id: ElementIdWire::Name("mic".into()),
            style: StyleId(0),
            device: Some(long.clone()),
            output: Some("speakers".into()),
            monitor: false,
            revision: 1,
            commands: std::iter::repeat_n(
                MicCommand::Record { seconds: 200 },
                MAX_MIC_COMMANDS + 1,
            )
            .chain([MicCommand::Record { seconds: 0 }])
            .collect(),
            on_state: None,
        };
        let report = crate::sanitize_tree(&mut node, &crate::styles::testing::held()).unwrap();
        let Node::Microphone {
            device,
            output,
            commands,
            ..
        } = node
        else {
            unreachable!()
        };
        assert_eq!(device, None);
        assert_eq!(output.as_deref(), Some("speakers"));
        assert_eq!(commands.len(), MAX_MIC_COMMANDS);
        assert!(commands.iter().all(|c| *c
            == MicCommand::Record {
                seconds: MAX_RECORD_SECONDS
            }));
        assert_eq!(report.strings, 1);
        let mut camera = Node::Camera {
            id: ElementIdWire::Name("camera".into()),
            style: StyleId(0),
            device: Some(long),
            mirror: true,
            on_state: None,
        };
        crate::sanitize_tree(&mut camera, &crate::styles::testing::held()).unwrap();
        assert!(matches!(camera, Node::Camera { device: None, .. }));
    }
}
