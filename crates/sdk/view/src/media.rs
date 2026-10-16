//! The camera and microphone pieces: a camera's picture and a microphone's
//! level, captured and drawn by the host ([`wire::Node::Camera`],
//! [`wire::Node::Microphone`]), for a view that declares
//! [`Capability::Camera`](crate::methods::Capability::Camera) or
//! [`Capability::Microphone`](crate::methods::Capability::Microphone). The
//! view never sees a picture, a sound or a level: it places the pieces,
//! steers them, and hears how their devices are ([`MediaState`]).
use crate::element::wire_id;
use crate::interactivity::EventListener;
use crate::revised::Revised;
use crate::{App, Element, ElementId, IntoElement, Lowering, Window, wire};
use gpui::{ObjectFit, StyleRefinement, Styled};

/// What a camera or microphone piece tells its view: whether its device is
/// live, refused, busy or gone, the device's name, a microphone's
/// recording and whether the person is speaking.
pub use wire::{MediaState, MediaStatus, Playback};

/// A camera's picture the host draws in this element's box: the whole
/// picture, centred, or as [`Camera::object_fit`] says; flipped left
/// to right with [`Camera::mirror`]. The camera opens the first time the
/// host sees the element under `id` and is released (its light off) when a
/// frame no longer has it: a view turns its camera off by not placing the
/// piece. Only a view whose manifest declares
/// [`Capability::Camera`](crate::methods::Capability::Camera) may place
/// one: the host refuses any other's frame.
///
/// ```
/// # use view::prelude::*;
/// let preview = camera("preview")
///     .mirror(true)
///     .on_state(|state: &MediaState, _, _| println!("{:?}", state.status))
///     .size_full();
/// ```
pub struct Camera {
    id: ElementId,
    device: Option<String>,
    mirror: bool,
    object_fit: ObjectFit,
    on_state: Option<EventListener<MediaState>>,
    style: Box<StyleRefinement>,
}

pub fn camera(id: impl Into<ElementId>) -> Camera {
    Camera {
        id: id.into(),
        device: None,
        mirror: false,
        object_fit: ObjectFit::Contain,
        on_state: None,
        style: Box::default(),
    }
}

impl Camera {
    /// The camera to open, by an id `camera.devices` listed; without it,
    /// the system's default. A new id opens the new camera.
    pub fn device(mut self, id: impl Into<String>) -> Self {
        self.device = Some(id.into());
        self
    }

    /// Flips the picture left to right, as a mirror shows a face.
    pub fn mirror(mut self, mirror: bool) -> Self {
        self.mirror = mirror;
        self
    }

    /// How the picture fills the box, as an image's does: whole
    /// (`ObjectFit::Contain`, the default) or covering it, cut centred
    /// (`ObjectFit::Cover`).
    pub fn object_fit(mut self, object_fit: ObjectFit) -> Self {
        self.object_fit = object_fit;
        self
    }

    /// Hears the camera: starting, live, refused, busy, gone.
    pub fn on_state(
        mut self,
        listener: impl Fn(&MediaState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_state = Some(Box::new(listener));
        self
    }
}

/// What a view asks of a [`microphone`] piece, kept until the frame that
/// carries it: give it to the piece with [`Microphone::track`], call
/// [`record`](Self::record) or [`stop`](Self::stop), and notify. Each
/// reaches the host once.
#[derive(Clone, Default)]
pub struct MicHandle(std::rc::Rc<std::cell::RefCell<Revised<wire::MicCommand>>>);

impl MicHandle {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&self, command: wire::MicCommand) {
        self.0.borrow_mut().push(command);
    }

    /// Records `seconds`, then plays the recording on the piece's output.
    pub fn record(&self, seconds: u8) {
        self.push(wire::MicCommand::Record { seconds });
    }

    /// Ends a recording or its playback.
    pub fn stop(&self) {
        self.push(wire::MicCommand::Stop);
    }
}

/// A microphone's level the host draws as a meter in this element's box:
/// a bar from the left in the element's text colour over its background,
/// moving as the person speaks, with no frame of the view's. The
/// microphone opens and is released as a [`camera`]'s camera is: a view
/// mutes by not placing the piece. Only a view whose manifest declares
/// [`Capability::Microphone`](crate::methods::Capability::Microphone) may
/// place one.
///
/// ```
/// # use view::prelude::*;
/// let mic = MicHandle::new();
/// let meter = microphone("meter").track(&mic).h(px(8.)).w_full();
/// mic.record(5);
/// ```
pub struct Microphone {
    id: ElementId,
    device: Option<String>,
    output: Option<String>,
    monitor: bool,
    handle: Option<MicHandle>,
    on_state: Option<EventListener<MediaState>>,
    style: Box<StyleRefinement>,
}

pub fn microphone(id: impl Into<ElementId>) -> Microphone {
    Microphone {
        id: id.into(),
        device: None,
        output: None,
        monitor: false,
        handle: None,
        on_state: None,
        style: Box::default(),
    }
}

impl Microphone {
    /// The microphone to open, by an input `microphone.devices` listed;
    /// without it, the system's default.
    pub fn device(mut self, id: impl Into<String>) -> Self {
        self.device = Some(id.into());
        self
    }

    /// The speaker a recording plays on and the monitor loops to, by an
    /// output `microphone.devices` listed; without it, the system's
    /// default.
    pub fn output(mut self, id: impl Into<String>) -> Self {
        self.output = Some(id.into());
        self
    }

    /// Plays what the microphone hears straight back while set. Through
    /// speakers it feeds back: set it only on the person's explicit choice,
    /// and tell them to wear headphones.
    pub fn monitor(mut self, monitor: bool) -> Self {
        self.monitor = monitor;
        self
    }

    /// The piece takes its recordings from `handle`.
    pub fn track(mut self, handle: &MicHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// Hears the microphone and its recording.
    pub fn on_state(
        mut self,
        listener: impl Fn(&MediaState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_state = Some(Box::new(listener));
        self
    }
}

impl Element for Camera {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        wire::Node::Camera {
            id: wire_id(self.id),
            style: lowering.style(&self.style),
            device: self.device,
            mirror: self.mirror,
            object_fit: self.object_fit.into(),
            on_state: self
                .on_state
                .map(|listener| lowering.route(crate::slots::Kind::MediaState, listener)),
        }
    }
}

impl Element for Microphone {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let (revision, commands) = self
            .handle
            .map(|handle| handle.0.borrow_mut().take())
            .unwrap_or_default();
        wire::Node::Microphone {
            id: wire_id(self.id),
            style: lowering.style(&self.style),
            device: self.device,
            output: self.output,
            monitor: self.monitor,
            revision,
            commands,
            on_state: self
                .on_state
                .map(|listener| lowering.route(crate::slots::Kind::MediaState, listener)),
        }
    }
}

impl IntoElement for Camera {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl IntoElement for Microphone {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Styled for Camera {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Styled for Microphone {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl gpui::prelude::FluentBuilder for Camera {}
impl gpui::prelude::FluentBuilder for Microphone {}

impl crate::testing::TestAppContext {
    /// The camera or microphone piece `name` reports `state`, as the app
    /// tells it when its device starts, fails or goes, or a recording
    /// moves.
    pub fn simulate_media_state(&mut self, name: &str, state: MediaState) {
        let (wire::Node::Camera {
            on_state: Some(handler),
            ..
        }
        | wire::Node::Microphone {
            on_state: Some(handler),
            ..
        }) = self.node(name)
        else {
            panic!("{name:?} is no camera or microphone piece with a state route");
        };
        let handler = *handler;
        self.simulate_event(wire::Event::Media { handler, state });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods::Capability;
    use crate::prelude::*;
    use crate::testing::TestAppContext;
    use crate::{View, div};

    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct Check {
        #[serde(skip)]
        mic: MicHandle,
        heard: Vec<MediaStatus>,
    }

    impl View for Check {
        const NAME: &'static str = "Check";
        const CAPABILITIES: &'static [Capability] = &[Capability::Camera, Capability::Microphone];
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self::default()
        }
    }

    impl Render for Check {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let heard = cx.listener(|view: &mut Self, state: &MediaState, _, cx| {
                view.heard.push(state.status.clone());
                cx.notify();
            });
            div()
                .child(
                    camera("camera")
                        .device("1")
                        .mirror(true)
                        .object_fit(ObjectFit::Cover)
                        .on_state(heard),
                )
                .child(
                    microphone("mic")
                        .output("headphones")
                        .monitor(true)
                        .track(&self.mic),
                )
        }
    }

    /// The pieces carry their devices and flags; a recording crosses once,
    /// under a revision the host has not seen; a camera's state reaches the
    /// view's listener.
    #[test]
    fn media_pieces_carry_their_devices_and_a_recording_once() {
        let mut cx = TestAppContext::new();
        let view = cx.open::<Check>();
        let wire::Node::Camera {
            device,
            mirror: true,
            object_fit: wire::ImageObjectFit::Cover,
            on_state: Some(_),
            ..
        } = cx.node("camera").clone()
        else {
            panic!("no mirrored camera with a state route: {:?}", cx.root());
        };
        assert_eq!(device.as_deref(), Some("1"));
        let wire::Node::Microphone {
            device: None,
            output,
            monitor: true,
            revision,
            commands,
            on_state: None,
            ..
        } = cx.node("mic").clone()
        else {
            panic!("no monitoring microphone: {:?}", cx.root());
        };
        assert_eq!(output.as_deref(), Some("headphones"));
        assert!(commands.is_empty());
        cx.update(&view, |view, _, cx| {
            view.mic.record(5);
            cx.notify();
        });
        cx.run_until_parked();
        let wire::Node::Microphone {
            revision: moved,
            commands,
            ..
        } = cx.node("mic").clone()
        else {
            unreachable!()
        };
        assert_eq!(moved, revision + 1);
        assert_eq!(commands, vec![wire::MicCommand::Record { seconds: 5 }]);
        cx.simulate_media_state(
            "camera",
            MediaState {
                status: MediaStatus::Denied,
                ..Default::default()
            },
        );
        let wire::Node::Microphone { commands, .. } = cx.node("mic").clone() else {
            unreachable!()
        };
        assert!(commands.is_empty(), "the recording went with its frame");
        cx.update(&view, |view, _, _| {
            assert_eq!(view.heard, [MediaStatus::Denied]);
        });
    }

    /// Places a microphone and keeps what it hears.
    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct Meter {
        heard: Vec<(MediaStatus, Playback, bool)>,
    }

    impl View for Meter {
        const NAME: &'static str = "Meter";
        const CAPABILITIES: &'static [Capability] = &[Capability::Microphone];
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self::default()
        }
    }

    impl Render for Meter {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            microphone("mic").on_state(cx.listener(|view, state: &MediaState, _, cx| {
                view.heard
                    .push((state.status.clone(), state.playback.clone(), state.speaking));
                cx.notify();
            }))
        }
    }

    /// A microphone's state route carries its device, its recording and
    /// whether the person speaks to the view's listener.
    #[test]
    fn a_microphone_hears_its_device_and_its_recording() {
        let mut cx = TestAppContext::new();
        let view = cx.open::<Meter>();
        cx.simulate_media_state(
            "mic",
            MediaState {
                status: MediaStatus::Live,
                playback: Playback::Recording,
                speaking: true,
                ..Default::default()
            },
        );
        cx.update(&view, |view, _, _| {
            assert_eq!(view.heard, [(MediaStatus::Live, Playback::Recording, true)]);
        });
    }

    /// Places a camera, which its manifest leaves out.
    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct NoCamera;

    impl View for NoCamera {
        const NAME: &'static str = "NoCamera";
        const CAPABILITIES: &'static [Capability] = &[Capability::Microphone];
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self
        }
    }

    impl Render for NoCamera {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(camera("camera"))
        }
    }

    #[test]
    #[should_panic(expected = "undeclared_capability")]
    fn a_camera_the_manifest_leaves_out_fails_the_test() {
        TestAppContext::new().open::<NoCamera>();
    }

    /// Places a microphone, which its manifest leaves out.
    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct NoMicrophone;

    impl View for NoMicrophone {
        const NAME: &'static str = "NoMicrophone";
        const CAPABILITIES: &'static [Capability] = &[Capability::Camera];
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self
        }
    }

    impl Render for NoMicrophone {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(microphone("mic"))
        }
    }

    #[test]
    #[should_panic(expected = "undeclared_capability")]
    fn a_microphone_the_manifest_leaves_out_fails_the_test() {
        TestAppContext::new().open::<NoMicrophone>();
    }
}
