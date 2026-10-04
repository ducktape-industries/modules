//! `#[derive(Ask)]` on a program's `Query`: one type per variant, asked
//! alone, each knowing the reply that answers it (`program::Ask`), and
//! which tables each variant reads (`program::Reads`, from `#[reads(..)]`).
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, Ident, Path, Token, Type, braced, parenthesized,
    parse_macro_input,
};

#[proc_macro_derive(Ask, attributes(ask, reads))]
pub fn derive_ask(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    asks(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// What answers a variant asked alone: the reply variant and what it
/// carries, one value (`Reply::List(PageResponse<Row>)`) or named fields
/// (`Reply::Thread { root: Option<Row>, replies: PageResponse<Row> }`,
/// answered as the tuple of them, in order).
enum Answer {
    One(Path, Type),
    Fields(Path, Vec<(Ident, Type)>),
}

impl Parse for Answer {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let variant = Path::parse_mod_style(input)?;
        let content;
        if input.peek(syn::token::Paren) {
            parenthesized!(content in input);
            return Ok(Answer::One(variant, content.parse()?));
        }
        if input.peek(syn::token::Brace) {
            braced!(content in input);
            let fields = content.parse_terminated(
                |field| {
                    let name: Ident = field.parse()?;
                    field.parse::<Token![:]>()?;
                    Ok((name, field.parse::<Type>()?))
                },
                Token![,],
            )?;
            return Ok(Answer::Fields(variant, fields.into_iter().collect()));
        }
        Err(input.error(
            "name the reply that answers it: `Reply::List(PageResponse<Row>)` \
             or `Reply::Thread { root: Option<Row>, replies: PageResponse<Row> }`",
        ))
    }
}

fn ask_attr(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|attr| attr.path().is_ident("ask"))
}

/// One `#[reads(..)]`: the tables a block must have written to for the
/// answer to have moved, and whose program they are. The query's own
/// (`#[reads(MESSAGES, REACTIONS)]`), or another program's, named first
/// (`#[reads(chat::Chat: chat::tables::ANSWERED, chat::tables::MESSAGES)]`)
/// for a question answered partly from its tables.
struct Group {
    program: Option<Path>,
    tables: Vec<Path>,
}

impl Parse for Group {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut group = Group {
            program: None,
            tables: Vec::new(),
        };
        if input.is_empty() {
            return Ok(group);
        }
        let first = Path::parse_mod_style(input)?;
        if input.parse::<Option<Token![:]>>()?.is_some() {
            group.program = Some(first);
        } else {
            group.tables.push(first);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        let rest = Punctuated::<Path, Token![,]>::parse_terminated(input)?;
        group.tables.extend(rest);
        Ok(group)
    }
}

/// What the `#[reads(..)]` attributes on a variant say a block must have
/// written to for the answer to have moved: a block of a group's program
/// that wrote a key one of the group's tables owns, for any group. With no
/// attribute, any block of any program (`true`); a group with no tables
/// never.
fn touched_by(attrs: &[Attribute], own: &Path) -> syn::Result<Tokens> {
    let mut groups = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("reads")) {
        let group: Group = attr.parse_args()?;
        if group.tables.is_empty() {
            groups.push(quote!(false));
            continue;
        }
        let program = group.program.as_ref().unwrap_or(own);
        let owns = group.tables.iter().map(|table| quote!(#table.owns(key)));
        groups.push(quote! {
            (program == <#program as ::program::Program>::NAME
                && keys.iter().any(|key| #(#owns)||*))
        });
    }
    if groups.is_empty() {
        return Ok(quote!(true));
    }
    Ok(quote!(#(#groups)||*))
}

fn docs(attrs: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attrs.iter().filter(|attr| attr.path().is_ident("doc"))
}

fn asks(input: &DeriveInput) -> syn::Result<Tokens> {
    let query = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(Error::new_spanned(
            query,
            "`#[derive(Ask)]` goes on a program's `Query` enum",
        ));
    };
    let program: Path = ask_attr(&input.attrs)
        .ok_or_else(|| {
            Error::new_spanned(
                query,
                "`#[ask(Program)]` names the program this is the query of",
            )
        })?
        .parse_args()?;
    let mut types = Vec::new();
    let mut impls = Vec::new();
    let mut touched = Vec::new();
    for variant in &data.variants {
        let name = &variant.ident;
        let touched_by = touched_by(&variant.attrs, &program)?;
        touched.push(quote!(#query::#name { .. } => #touched_by));
        let Some(attr) = ask_attr(&variant.attrs) else {
            continue;
        };
        let answer: Answer = attr.parse_args()?;
        let variant_docs = docs(&variant.attrs);
        let (shape, unpack, pack) = match &variant.fields {
            Fields::Named(fields) => {
                let names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                let declared = fields.named.iter().map(|f| {
                    let (field_docs, field, ty) = (docs(&f.attrs), &f.ident, &f.ty);
                    quote!(#(#field_docs)* pub #field: #ty)
                });
                (
                    quote!({ #(#declared),* }),
                    quote!(ask::#name { #(#names),* }),
                    quote!(#query::#name { #(#names),* }),
                )
            }
            Fields::Unnamed(fields) => {
                let names: Vec<_> = (0..fields.unnamed.len())
                    .map(|at| format_ident!("field{at}"))
                    .collect();
                let declared = fields.unnamed.iter().map(|f| {
                    let ty = &f.ty;
                    quote!(pub #ty)
                });
                (
                    quote!((#(#declared),*);),
                    quote!(ask::#name(#(#names),*)),
                    quote!(#query::#name(#(#names),*)),
                )
            }
            Fields::Unit => (quote!(;), quote!(ask::#name), quote!(#query::#name)),
        };
        types.push(quote! {
            #(#variant_docs)*
            #[derive(Debug)]
            pub struct #name #shape
        });
        let (reply, arm) = match answer {
            Answer::One(variant, ty) => (
                quote!(#ty),
                quote!(#variant(answer) => ::core::option::Option::Some(answer)),
            ),
            Answer::Fields(variant, fields) => {
                let names = fields.iter().map(|(name, _)| name);
                let tys = fields.iter().map(|(_, ty)| ty);
                let answered = names.clone();
                (
                    quote!((#(#tys,)*)),
                    quote!(#variant { #(#names),* } => ::core::option::Option::Some((#(#answered,)*))),
                )
            }
        };
        impls.push(quote! {
            impl ::core::convert::From<ask::#name> for #query {
                fn from(#unpack: ask::#name) -> #query {
                    #pack
                }
            }

            impl ::program::Ask for ask::#name {
                type Program = #program;
                type Reply = #reply;
                fn answer(
                    reply: <#program as ::program::Program>::Reply,
                ) -> ::core::option::Option<Self::Reply> {
                    #[allow(unreachable_patterns)]
                    match reply {
                        #arm,
                        _ => ::core::option::Option::None,
                    }
                }
            }

            impl ::program::Reads for ask::#name {
                // an undeclared variant answers `true` without reading either
                #[allow(unused_variables)]
                fn touched_by(&self, program: &str, keys: &[::std::vec::Vec<u8>]) -> bool {
                    #touched_by
                }
            }
        });
    }
    let doc = format!(
        "Each [`{query}`] a caller asks alone, as its own type: built like the \
         variant, it is that variant on the wire, and [`program::Ask`] names \
         the reply that answers it."
    );
    // a `Query` no caller asks alone (only `#[reads]`) has no `ask` module
    let asks = (!types.is_empty()).then(|| {
        quote! {
            #[doc = #doc]
            pub mod ask {
                #[allow(unused_imports)]
                use super::*;
                #(#types)*
            }
        }
    });
    Ok(quote! {
        #asks

        impl ::program::Reads for #query {
            // a `Query` with no `#[reads]` answers `true` without reading either
            #[allow(unused_variables)]
            fn touched_by(&self, program: &str, keys: &[::std::vec::Vec<u8>]) -> bool {
                match self {
                    #(#touched,)*
                }
            }
        }

        #(#impls)*
    })
}
