//! A completeness gate the byte fixtures owe the schema: `schema.txt` pins
//! every field and variant the wire reaches, but `frame.bin` only pins the
//! bytes of what its samples happen to build. This test closes that gap
//! mechanically, off the same serde-reflection registry `schema.rs` already
//! traces -- never a hand-kept list, which drifts the moment someone adds a
//! variant and forgets to update it.
//!
//! THE RULE. A unit variant (`enum X { A, B }`) carries no bytes an
//! attribute can hide a change in, and `schema.txt` already pins its name
//! (a rename moves the schema); so it needs no byte sample and is exempt.
//! Every OTHER variant (newtype, tuple, or struct-shaped) must be entered
//! at least once. Every struct field -- a plain struct's, or a struct-shaped
//! variant's -- must be seen PRESENT and non-null at least once: this is
//! the same requirement for a required field (trivially true the moment its
//! container is sampled) and an `Option`/`skip_serializing_if` one (true
//! only once some sample sets it), so there is no separate "optional field"
//! bookkeeping to keep in sync by hand.
//!
//! Expected is read straight off the registry: every container reachable
//! from `Frame`, `Event` and `WidgetCommand` (following struct fields and
//! enum variants, cutting cycles by name), so an incidentally-traced type
//! schema.rs carries for its own reasons (`methods::Call`, sampled only on
//! the borsh side) is correctly never asked for here. Observed is read off
//! the built sample's JSON twin -- not `serde_reflection::trace_value`,
//! which unifies repeat containers by exact field count and refuses the
//! moment two `Aria`s skip different fields; JSON keeps field names and
//! represents a skipped field the same as an explicit `null`, which is
//! exactly "not present" either way.
//!
//! `grid` -- `schema.rs`'s second registry, gpui's `GridLocation` carve-out
//! for a `Range` name that collides with the tree's own -- is not required
//! to build `expected` here: every container it holds besides `Range` itself
//! already appears, identically, in `tree`. But that same name collision
//! means this gate cannot see `GridPlacement` at all: `reachable()` walking
//! `tree` resolves a `Range` type name to the WIRE's own `Range` (its
//! `start`/`end` fields), never to gpui's `Range<GridPlacement>`, so
//! `GridPlacement`'s variants (`Line`, `Span`, `Auto`) never enter `expected`
//! and this test cannot fail if one goes unsampled. `full_style()` samples
//! all three anyway (`grid_location: Line(1)..Span(2)` and `Auto..Auto`), so
//! the byte fixture is complete here -- only this gate's mechanical coverage
//! of it is not, and that gap is accepted rather than closed (the `grid`
//! registry has no `Frame`/`Event`/`WidgetCommand` root of its own to BFS
//! from without hand-listing `GridLocation`, which is the "never a
//! hand-maintained list" rule this whole file exists to avoid breaking).
use std::collections::BTreeSet;

use serde_json::Value as Json;
use serde_reflection::{ContainerFormat, Format, Registry, VariantFormat};

use super::schema::tree;

/// Every container name reachable from `roots`: a struct's field types, an
/// enum's variant field types, of every shape, followed until every name is
/// visited once. A name the registry does not carry (an unreachable stand-in)
/// is skipped rather than followed.
fn reachable(registry: &Registry, roots: &[&str]) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<String> = roots.iter().map(|name| (*name).to_string()).collect();
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(container) = registry.get(&name) else {
            continue;
        };
        for format in container_formats(container) {
            collect_type_names(format, &mut stack);
        }
    }
    seen
}

fn container_formats(container: &ContainerFormat) -> Vec<&Format> {
    match container {
        ContainerFormat::UnitStruct => vec![],
        ContainerFormat::NewTypeStruct(format) => vec![format.as_ref()],
        ContainerFormat::TupleStruct(formats) => formats.iter().collect(),
        ContainerFormat::Struct(named) => named.iter().map(|field| &field.value).collect(),
        ContainerFormat::Enum(variants) => variants
            .values()
            .flat_map(|variant| match &variant.value {
                VariantFormat::Unit | VariantFormat::Variable(_) => vec![],
                VariantFormat::NewType(format) => vec![format.as_ref()],
                VariantFormat::Tuple(formats) => formats.iter().collect(),
                VariantFormat::Struct(named) => named.iter().map(|field| &field.value).collect(),
            })
            .collect(),
    }
}

fn collect_type_names(format: &Format, out: &mut Vec<String>) {
    match format {
        Format::TypeName(name) => out.push(name.clone()),
        Format::Option(inner) | Format::Seq(inner) => collect_type_names(inner, out),
        Format::Map { key, value } => {
            collect_type_names(key, out);
            collect_type_names(value, out);
        }
        Format::Tuple(formats) => formats
            .iter()
            .for_each(|format| collect_type_names(format, out)),
        Format::TupleArray { content, .. } => collect_type_names(content, out),
        _ => {}
    }
}

/// What a set of samples must, or does, show: a `(container, variant)` for
/// every non-unit variant entered, and a `container.field` (or
/// `container::variant.field`) for every struct field seen present.
#[derive(Default)]
struct Shown {
    variants: BTreeSet<(String, String)>,
    fields: BTreeSet<String>,
}

/// What the registry obliges: every reachable enum's non-unit variants, and
/// every reachable struct's (or struct-shaped variant's) fields.
fn expected(registry: &Registry, reachable: &BTreeSet<String>) -> Shown {
    let mut out = Shown::default();
    for name in reachable {
        let Some(container) = registry.get(name) else {
            continue;
        };
        match container {
            ContainerFormat::Struct(named) => {
                for field in named {
                    out.fields.insert(format!("{name}.{}", field.name));
                }
            }
            ContainerFormat::Enum(variants) => {
                for variant in variants.values() {
                    match &variant.value {
                        VariantFormat::Unit | VariantFormat::Variable(_) => {}
                        VariantFormat::Struct(named) => {
                            out.variants.insert((name.clone(), variant.name.clone()));
                            for field in named {
                                out.fields
                                    .insert(format!("{name}::{}.{}", variant.name, field.name));
                            }
                        }
                        VariantFormat::NewType(_) | VariantFormat::Tuple(_) => {
                            out.variants.insert((name.clone(), variant.name.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Walks `value` (a container named `name`'s JSON twin) against the
/// registry, recording every variant tag it carries and every field it
/// shows present and non-null.
fn observe(registry: &Registry, name: &str, value: &Json, out: &mut Shown) {
    let Some(container) = registry.get(name) else {
        return;
    };
    match container {
        ContainerFormat::Struct(named) => {
            let Json::Object(map) = value else { return };
            for field in named {
                let child = map.get(&field.name).unwrap_or(&Json::Null);
                if !child.is_null() {
                    out.fields.insert(format!("{name}.{}", field.name));
                }
                observe_format(registry, &field.value, child, out);
            }
        }
        ContainerFormat::Enum(variants) => {
            let (variant_name, inner): (&str, &Json) = match value {
                Json::String(tag) => (tag.as_str(), &Json::Null),
                Json::Object(map) if map.len() == 1 => {
                    let (tag, inner) = map.iter().next().expect("checked len == 1");
                    (tag.as_str(), inner)
                }
                _ => return,
            };
            let Some(variant) = variants
                .values()
                .find(|variant| variant.name == variant_name)
            else {
                return;
            };
            match &variant.value {
                VariantFormat::Unit | VariantFormat::Variable(_) => {}
                VariantFormat::NewType(format) => {
                    out.variants
                        .insert((name.to_owned(), variant_name.to_owned()));
                    observe_format(registry, format, inner, out);
                }
                VariantFormat::Tuple(formats) => {
                    out.variants
                        .insert((name.to_owned(), variant_name.to_owned()));
                    if let Json::Array(items) = inner {
                        for (format, item) in formats.iter().zip(items) {
                            observe_format(registry, format, item, out);
                        }
                    }
                }
                VariantFormat::Struct(named) => {
                    out.variants
                        .insert((name.to_owned(), variant_name.to_owned()));
                    let Json::Object(map) = inner else { return };
                    for field in named {
                        let child = map.get(&field.name).unwrap_or(&Json::Null);
                        if !child.is_null() {
                            out.fields
                                .insert(format!("{name}::{variant_name}.{}", field.name));
                        }
                        observe_format(registry, &field.value, child, out);
                    }
                }
            }
        }
        ContainerFormat::NewTypeStruct(format) => observe_format(registry, format, value, out),
        ContainerFormat::TupleStruct(formats) => {
            if let Json::Array(items) = value {
                for (format, item) in formats.iter().zip(items) {
                    observe_format(registry, format, item, out);
                }
            }
        }
        ContainerFormat::UnitStruct => {}
    }
}

fn observe_format(registry: &Registry, format: &Format, value: &Json, out: &mut Shown) {
    match format {
        Format::TypeName(name) => observe(registry, name, value, out),
        Format::Option(inner) => {
            if !value.is_null() {
                observe_format(registry, inner, value, out);
            }
        }
        Format::Seq(inner) => {
            if let Json::Array(items) = value {
                for item in items {
                    observe_format(registry, inner, item, out);
                }
            }
        }
        Format::Map { value: inner, .. } => {
            if let Json::Object(map) = value {
                for item in map.values() {
                    observe_format(registry, inner, item, out);
                }
            }
        }
        Format::Tuple(formats) => {
            if let Json::Array(items) = value {
                for (format, item) in formats.iter().zip(items) {
                    observe_format(registry, format, item, out);
                }
            }
        }
        Format::TupleArray { content, .. } => {
            if let Json::Array(items) = value {
                for item in items {
                    observe_format(registry, content, item, out);
                }
            }
        }
        _ => {}
    }
}

/// What `expected` names that `observed` never showed, one line each.
fn missing(expected: &Shown, observed: &Shown) -> Vec<String> {
    let mut gaps: Vec<String> = expected
        .variants
        .difference(&observed.variants)
        .map(|(container, variant)| format!("variant {container}::{variant}"))
        .chain(
            expected
                .fields
                .difference(&observed.fields)
                .map(|field| format!("field {field}")),
        )
        .collect();
    gaps.sort();
    gaps
}

/// One JSON value per element of a JSON array; panics on anything else,
/// which a `Vec<T>`'s twin never is.
fn elements(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected a JSON array"),
    }
}

#[test]
fn every_reachable_variant_and_field_is_sampled() {
    let (registry, _grid) = tree();
    let names = reachable(&registry, &["Frame", "Event", "WidgetCommand"]);
    let expected = expected(&registry, &names);

    let frame = super::events_fixture::every_frame();
    let events = super::events_fixture::every_event();
    let widget_commands = super::widget_fixture::every_widget_command();

    let mut observed = Shown::default();
    observe(
        &registry,
        "Frame",
        &serde_json::to_value(&frame).unwrap(),
        &mut observed,
    );
    for event in elements(&serde_json::to_value(&events).unwrap()) {
        observe(&registry, "Event", event, &mut observed);
    }
    for command in elements(&serde_json::to_value(&widget_commands).unwrap()) {
        observe(&registry, "WidgetCommand", command, &mut observed);
    }

    let gaps = missing(&expected, &observed);
    assert!(
        gaps.is_empty(),
        "the golden fixtures do not sample: {gaps:#?}"
    );
}
