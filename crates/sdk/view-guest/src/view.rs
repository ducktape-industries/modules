//! A serializable root view and small loading conveniences.
use crate::host::Error;
use crate::{Context, IntoElement, Task, Window};
use futures::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::future::Future;

pub trait Render: 'static + Sized {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement;
}
/// A root view: everything the host reads about it (the manifest
/// `export_view!` writes: [`NAME`](View::NAME), [`DESCRIPTION`](View::DESCRIPTION),
/// [`CAPABILITIES`](View::CAPABILITIES), [`MIN_WINDOW_WIDTH`](View::MIN_WINDOW_WIDTH),
/// [`TARGETS`](View::TARGETS)) and its two lifecycle entries.
pub trait View: Render + Serialize + DeserializeOwned {
    /// The name on the tab and in the catalog, `1..=64` bytes.
    const NAME: &'static str;
    /// One line for the catalog, at most 256 bytes.
    const DESCRIPTION: &'static str = "";
    /// The `<capability>` halves of the method kinds this view asks
    /// through, and the only ones the host lets it reach: a method whose
    /// capability is not here is refused `undeclared_capability`, by the
    /// app and by `TestAppContext` alike.
    const CAPABILITIES: &'static [crate::methods::Capability] = &[];
    /// The programs this view addresses (`op.submit`, `module.query`,
    /// `module.changes`), by the name each `Program` carries
    /// (`&[chat::Chat::NAME, program::role::Identity::NAME]`), and the only
    /// ones the host signs or subscribes for it: a method naming another
    /// is refused `undeclared_target`, by the app and by `TestAppContext`
    /// alike. A view with the `op` or `module` capability names at least
    /// one, or it does not compile.
    const TARGETS: &'static [&'static str] = &[];
    /// The narrowest content width, in logical px, at which every essential
    /// element is visible and nothing is clipped. The app never lays the
    /// view out narrower, and never sizes a window holding it narrower
    /// unless the desk or screen itself is narrower. `export_view!` writes
    /// it into the manifest:
    ///
    /// ```
    /// # use serde::{Deserialize, Serialize};
    /// # use view_guest::{Context, IntoElement, Render, View, Window, div};
    /// #[derive(Serialize, Deserialize)]
    /// struct Wide;
    /// impl View for Wide {
    ///     const NAME: &'static str = "Wide";
    ///     const MIN_WINDOW_WIDTH: u32 = 640;
    ///     fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
    ///         Wide
    ///     }
    /// }
    /// # impl Render for Wide {
    /// #     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    /// #         div()
    /// #     }
    /// # }
    /// view_guest::export_view!(Wide);
    /// # fn main() {}
    /// ```
    ///
    /// Outside `1..=8192` the view does not compile:
    ///
    /// ```compile_fail,E0080
    /// # use serde::{Deserialize, Serialize};
    /// # use view_guest::{Context, IntoElement, Render, View, Window, div};
    /// #[derive(Serialize, Deserialize)]
    /// struct Zero;
    /// impl View for Zero {
    ///     const NAME: &'static str = "Zero";
    ///     const MIN_WINDOW_WIDTH: u32 = 0;
    ///     fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
    ///         Zero
    ///     }
    /// }
    /// # impl Render for Zero {
    /// #     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    /// #         div()
    /// #     }
    /// # }
    /// view_guest::export_view!(Zero);
    /// # fn main() {}
    /// ```
    const MIN_WINDOW_WIDTH: u32 = 480;
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self;
    fn restored(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}
/// Data the view asked for, in the four states it can be in. `Loading`
/// carries the abort handle: dropping the slot cancels the load.
/// Serialises `Loading` as `Idle`, so a restored view reloads it.
#[derive(Default)]
pub enum Loadable<T> {
    #[default]
    Idle,
    Loading(Task<()>),
    Ready(T),
    Failed(Error),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
    pub fn ready_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
    pub fn failed(&self) -> Option<&Error> {
        match self {
            Self::Failed(refusal) => Some(refusal),
            _ => None,
        }
    }
    pub fn take(&mut self) -> Self {
        std::mem::replace(self, Self::Idle)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LoadableSnapshot<T> {
    Idle,
    Ready(T),
    Failed { code: String, message: String },
}

/// A finished load: its value, or the refusal in its place.
impl<T> From<Result<T, Error>> for Loadable<T> {
    fn from(result: Result<T, Error>) -> Self {
        match result {
            Ok(value) => Self::Ready(value),
            Err(refusal) => Self::Failed(refusal),
        }
    }
}

impl<T: Serialize> Serialize for Loadable<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Idle | Self::Loading(_) => LoadableSnapshot::<&T>::Idle,
            Self::Ready(value) => LoadableSnapshot::Ready(value),
            Self::Failed(refusal) => LoadableSnapshot::Failed {
                code: refusal.code.clone(),
                message: refusal.message.clone(),
            },
        }
        .serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Loadable<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match LoadableSnapshot::deserialize(deserializer)? {
            LoadableSnapshot::Idle => Self::Idle,
            LoadableSnapshot::Ready(value) => Self::Ready(value),
            LoadableSnapshot::Failed { code, message } => Self::Failed(Error::new(code, message)),
        })
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Loadable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("Idle"),
            Self::Loading(_) => f.write_str("Loading"),
            Self::Ready(value) => f.debug_tuple("Ready").field(value).finish(),
            Self::Failed(refusal) => f.debug_tuple("Failed").field(refusal).finish(),
        }
    }
}

impl<V: View> Context<'_, V> {
    pub fn load<T: 'static>(
        &mut self,
        work: impl Future<Output = Result<T, Error>> + 'static,
        at: impl Fn(&mut V) -> &mut Loadable<T> + 'static,
    ) -> Loadable<T> {
        let task = self.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |view, cx| {
                *at(view) = Loadable::from(result);
                cx.notify();
            });
        });
        Loadable::Loading(task)
    }
    /// Runs `each` on every item `stream` yields, in order, until the
    /// stream ends or the view is gone; the view is re-rendered after each.
    /// Keep the task: dropping it unsubscribes. A refused item is handed to
    /// `each` like any other and does not end the stream, so each follower
    /// says what a refusal means to it.
    pub fn for_each<T: 'static>(
        &mut self,
        mut stream: impl Stream<Item = T> + Unpin + 'static,
        mut each: impl FnMut(&mut V, T, &mut Window, &mut Context<V>) + 'static,
    ) -> Task<()> {
        self.spawn(async move |this, cx| {
            while let Some(item) = stream.next().await {
                let landed = this.update_in(cx, |view, window, cx| {
                    each(view, item, window, cx);
                    cx.notify();
                });
                if landed.is_err() {
                    break;
                }
            }
        })
    }

    /// Re-reads what is already on screen: the old value stays until
    /// `land` takes the new one, and stays if the read is refused. A
    /// refusal lands in the host's log (`host.log`, so the view declares
    /// `host`), named by the view, so a failed re-read is never silent.
    pub fn refresh<T: 'static>(
        &mut self,
        work: impl Future<Output = Result<T, Error>> + 'static,
        land: impl FnOnce(&mut V, T, &mut Context<V>) + 'static,
    ) {
        self.spawn(async move |this, cx| {
            let result = work.await;
            // the view is gone: nothing is waiting for the read
            let _ = this.update(cx, |view, cx| match result {
                Ok(value) => {
                    land(view, value, cx);
                    cx.notify();
                }
                Err(refusal) => cx.host().log(format!(
                    "{}: a refresh was refused: {refusal}",
                    std::any::type_name::<V>()
                )),
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod follow_tests {
    use crate::methods::Changes;
    use crate::testing::Probe;
    use crate::testing::TestAppContext;
    use crate::{Context, IntoElement, ParentElement, Render, Task, View, Window};
    use serde::{Deserialize, Serialize};

    /// Counts the heads a program's live stream announces.
    #[derive(Default, Serialize, Deserialize)]
    struct Heads {
        seen: usize,
        #[serde(skip)]
        live: Option<Task<()>>,
    }
    impl View for Heads {
        const NAME: &'static str = "Heads";
        const CAPABILITIES: &'static [crate::methods::Capability] = &[
            crate::methods::Capability::Module,
            crate::methods::Capability::Host,
        ];
        const TARGETS: &'static [&'static str] = &[<Probe as crate::methods::Program>::NAME];
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            let live = cx.host().subscribe::<Changes<Probe>>(());
            Self {
                seen: 0,
                live: Some(cx.for_each(live, |view: &mut Heads, _, _, _| view.seen += 1)),
            }
        }
    }
    impl Render for Heads {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            crate::div().child(self.seen.to_string())
        }
    }

    #[test]
    fn a_follower_hears_every_item_until_its_task_is_dropped() {
        let mut cx = TestAppContext::new();
        let feed = cx.host().stream::<Changes<Probe>>();
        let view = cx.open::<Heads>();
        cx.run_until_parked();
        feed.send(None);
        feed.send(None);
        cx.run_until_parked();
        view.read(|heads| assert_eq!(heads.seen, 2));
        assert!(cx.has_text("2"), "each item re-renders the view");
        view.update(&mut cx, |heads, _, _| heads.live = None);
        cx.run_until_parked();
        feed.send(None);
        cx.run_until_parked();
        view.read(|heads| assert_eq!(heads.seen, 2));
    }

    #[test]
    fn a_refused_refresh_keeps_the_value_and_logs_why() {
        use crate::methods::Query;
        let mut cx = TestAppContext::new();
        cx.host()
            .refuse::<Query<Probe>>("stale", "the probe moved on");
        let _feed = cx.host().stream::<Changes<Probe>>();
        let view = cx.open::<Heads>();
        view.update(&mut cx, |heads, _, cx| {
            heads.seen = 7;
            cx.notify();
            cx.refresh(cx.host().ask::<Query<Probe>>(()), |heads, (), _| {
                heads.seen = 0
            });
        });
        cx.run_until_parked();
        view.read(|heads| assert_eq!(heads.seen, 7));
        let logs = cx.host().logs();
        assert!(
            logs.iter()
                .any(|line| line.contains("refresh was refused")
                    && line.contains("the probe moved on")),
            "{logs:?}"
        );
    }
}
