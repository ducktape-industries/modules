//! The camera and the microphone in a view ([`crate::Node::Camera`],
//! [`crate::Node::Microphone`]), and the people a call brings in
//! ([`crate::Node::Remote`]): which device a piece opens, which live track
//! it feeds or plays, what the view asks of it ([`MicCommand`]) and what
//! the host tells back ([`MediaState`]). No picture, sound or level
//! crosses: the host captures, encodes, publishes, subscribes, decodes,
//! paints and plays, and the view places the pieces and steers them. The
//! devices to pick from are methods (`camera.devices`,
//! `microphone.devices`), their types in [`crate::methods`].
use serde::{Deserialize, Serialize};

/// One thing the view asks of its microphone piece, applied once by the
/// host when the node's `revision` moves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicCommand {
    /// Record `seconds` from the microphone (`0` records one, as `1`
    /// does), then play the recording on the piece's `output`: the person
    /// hears themselves after they stop talking, with no feedback through
    /// the speakers. A recording or playback under way is replaced.
    Record { seconds: u8 },
    /// Ends a recording or its playback.
    Stop,
}

/// Where a camera or microphone piece's device is, or a remote piece's
/// playing, as the host holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaStatus {
    /// Opening the device, or waiting on the person's answer to the
    /// system's permission prompt; a remote piece's track before its first
    /// object.
    #[default]
    Starting,
    /// Capturing: a camera's pictures are drawn, a microphone's level
    /// moves; a remote piece's track flows.
    Live,
    /// The person (or the system's policy) refused this app the device.
    Denied,
    /// Another program holds the device.
    Busy,
    /// The device went away while it was open (unplugged, its driver
    /// unloaded); a remote piece's track ended ([`MediaState::track`] says
    /// why).
    Gone,
    /// There is no such device: none at all, or none by the piece's id.
    NoDevice,
    /// This host captures nothing: a build without camera and microphone
    /// support, or a platform it does not cover; for a remote picture, a
    /// build that decodes no video (a voice still plays).
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
    /// A microphone's, or a remote voice's ([`crate::RemoteMedia::Audio`]):
    /// the person is speaking (the level held over the host's threshold,
    /// with the host's hysteresis), else `false`. The level itself never
    /// crosses.
    pub speaking: bool,
    /// On [`MediaStatus::Denied`]: whether the host shows the system's page
    /// that allows the device, where a view can send the person with
    /// `host.show` of [`Place::CameraSettings`] or
    /// [`Place::MicrophoneSettings`] (so the view declares `host`, not
    /// `link`, for its button); `false` elsewhere.
    ///
    /// [`Place::CameraSettings`]: crate::methods::Place::CameraSettings
    /// [`Place::MicrophoneSettings`]: crate::methods::Place::MicrophoneSettings
    pub opens_settings: bool,
    /// A microphone's: the host stopped its monitor under the view (the
    /// sound fell to a loudspeaker, where it would howl, or the speaker
    /// failed) and holds it off, whatever the node's `monitor` says, until
    /// a frame sends `monitor: false`. A view turns its own switch off on
    /// it; a camera's is always `false`.
    pub monitor_stopped: bool,
    /// The live track the piece feeds (a camera's or a microphone's
    /// `publish`) or plays ([`crate::Node::Remote`]): where it stands.
    /// [`TrackState::None`] on a piece with no track.
    pub track: TrackState,
}

/// Where a piece's live track stands, as the node has it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackState {
    /// The piece has no track: a camera or a microphone that publishes
    /// nothing.
    #[default]
    None,
    /// The room's program has not granted the track yet: the node holds
    /// the ask and takes it on the block that writes the row. The view
    /// sends the op that writes it, once per `Waiting`, as it would for a
    /// `live.publish` of its own.
    Waiting,
    /// The node took the track. `subscribers` counts the listeners of a
    /// published track (every node counted); a remote piece's is 0.
    Up { subscribers: u32 },
    /// The track is gone, with the node's refusal code: `not_granted` (the
    /// grant was taken back), `publisher_gone` (the publisher lost its
    /// connection or its grant), `track_ended` (the publisher ended it:
    /// its piece left its frame), `unknown_track` (no one publishes it),
    /// `duplicate_track` (another publishes it), `live_unavailable` (no
    /// live connection). The piece stays: a publishing piece publishes
    /// again on the next frame that still places it; a remote piece
    /// subscribes again when the room announces the track up again, and,
    /// ended `not_granted`, on the next frame that still places it, where
    /// it waits for a row as any ask does.
    Ended { code: String },
}
