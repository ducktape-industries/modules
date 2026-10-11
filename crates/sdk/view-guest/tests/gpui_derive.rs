//! `#[derive(IntoElement)]` as a view crate outside this one writes it: the
//! expansion names the SDK by its package name.
use ducktape_view_guest::ViewElement;
use ducktape_view_guest::prelude::*;

#[derive(IntoElement)]
struct DerivedComponent;

impl RenderOnce for DerivedComponent {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        "derived"
    }
}

#[derive(IntoElement)]
enum DerivedEnum {
    Unit,
}

impl RenderOnce for DerivedEnum {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        "enum"
    }
}

#[derive(IntoElement)]
struct GenericComponent<T>
where
    T: Clone + 'static,
{
    value: T,
}

impl<T: Clone + 'static> RenderOnce for GenericComponent<T> {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let _ = self.value;
        "generic"
    }
}

fn assert_view_element<T: RenderOnce>(_: ViewElement<T>) {}

#[test]
fn derive_and_any_element_use_the_internal_lowering_boundary() {
    let _: AnyElement = DerivedComponent.into_any_element();
    let _: AnyElement = div().child(DerivedComponent).into_any_element();
}

#[test]
fn derive_matches_gpui_component_element_shape() {
    assert_view_element(DerivedComponent.into_element());
    assert_view_element(DerivedEnum::Unit.into_element());
    assert_view_element(GenericComponent { value: 7_u8 }.into_element());
}

#[test]
fn derived_components_and_wrappers_have_fluent_builders() {
    let _: DerivedComponent = DerivedComponent.when(true, |component| component);
    let _: ViewElement<DerivedComponent> = DerivedComponent
        .into_element()
        .when(true, |element| element);
}
