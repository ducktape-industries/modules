//! The camera and the microphone in a view ([`crate::Node::Camera`],
//! [`crate::Node::Microphone`]): which device a piece opens, what the view
//! asks of it ([`MicCommand`]) and what the host tells back
//! ([`MediaState`]). No picture, sound or level crosses: the host captures,
//! paints and plays, and the view places the pieces and steers them. The
//! devices to pick from are methods (`camera.devices`,
//! `microphone.devices`), their types in [`crate::methods`].
use serde::{Deserialize, Serialize};

/// The system's privacy settings for the camera and for the microphone
/// (macOS): the only links besides `duck://` and `https://` a host opens,
/// and only where [`MediaState::settings`] says it can.
pub const SETTINGS_CAMERA: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera";
pub const SETTINGS_MICROPHONE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

/// How a [`crate::Node::Camera`]'s picture fills its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fit {
    /// The whole picture, centred; the box's background shows where the
    /// picture's shape leaves it (a person checking their own frame).
    #[default]
    Contain,
    /// The box filled, the picture scaled to cover it and cut centred (a
    /// call's tile, which never shows bars).
    Cover,
}

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
    /// The host's words for a [`MediaStatus::Failed`], and for a
    /// [`MediaStatus::Denied`] how to allow the device on this platform;
    /// else empty: one line, at most 200 bytes.
    pub detail: String,
    /// A microphone's: the person is speaking (the level held over the
    /// host's threshold, with the host's hysteresis), else `false`. The
    /// level itself never crosses.
    pub speaking: bool,
    /// On [`MediaStatus::Denied`]: the host can open the system's settings
    /// page that allows the device (`link.open` of [`SETTINGS_CAMERA`] or
    /// [`SETTINGS_MICROPHONE`]); `false` elsewhere.
    pub settings: bool,
}
