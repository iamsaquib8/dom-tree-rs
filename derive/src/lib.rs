//! `#[derive(FromHtml)]` for [domq](https://docs.rs/domq):
//! declarative extraction of structs from HTML with CSS selectors.
//!
//! Use it through the core crate's `derive` feature, which re-exports this
//! macro next to the `FromHtml` trait it implements:
//!
//! ```toml
//! [dependencies]
//! domq = { version = "0.4", features = ["derive"] }
//! ```
//!
//! ```ignore
//! use domq::FromHtml;
//!
//! #[derive(FromHtml)]
//! struct Product {
//!     #[html(select = "h1")]
//!     name: String,
//!     #[html(select = ".price", attr = "data-value")]
//!     price: f64,
//!     #[html(select = ".tag")]
//!     tags: Vec<String>,
//! }
//!
//! let product: Product = domq::parse(html).extract()?;
//! ```
//!
//! The attribute grammar, the container rules (`Option`, `Vec`, nested
//! structs) and the errors are documented in the core crate under
//! [Declarative extraction](https://docs.rs/domq/latest/domq/#declarative-extraction).
//!
//! The generated code refers to everything through `::domq::__private`
//! (never `std::`), so it works in `no_std` crates.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_docs)]

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Field, Fields, GenericArgument, Ident, LitStr, Meta,
    PathArguments, Token, Type, WherePredicate, parse_macro_input, parse_quote,
};

/// Implement `domq::FromHtml` for a struct with named fields.
///
/// Every field carries `#[html(...)]` saying what to select and what to read:
///
/// | key | meaning |
/// |---|---|
/// | `select = "css"` | the elements to read (absent: the scope itself) |
/// | `attr = "name"` | read an attribute (a missing one is an error, or `None`) |
/// | `html` / `outer_html` / `markdown` | read the inner HTML / outer HTML / Markdown |
/// | *(no source key)* | read the readable text, or a nested `FromHtml` type |
/// | `default` | no match or no attribute → `Default::default()` |
/// | `with = "path::to::fn"` | convert the value with `fn(&str) -> Result<T, E>` |
/// | `skip` | don't read anything: `Default::default()` |
///
/// On the struct, `#[html(select = "css")]` sets `FromHtml::SELECTOR`, the
/// elements `Dom::extract_all` reads one value from each.
///
/// The field type decides how many matches are read: `Option<T>` is `None`
/// without a match, `Vec<T>` takes every match in document order, any other
/// type takes the first match and fails without one.
#[proc_macro_derive(FromHtml, attributes(html))]
pub fn derive_from_html(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

const KEYS: &str = "`select`, `attr`, `html`, `outer_html`, `markdown`, `default`, `with`, `skip`";

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let fields = match &input.data {
        Data::Struct(data) => &data.fields,
        Data::Enum(data) => {
            let token = &data.enum_token;
            return Err(syn::Error::new_spanned(
                quote!(#token #name),
                "`#[derive(FromHtml)]` supports structs with named fields, not enums; \
                 implement `FromHtml` by hand to decide which variant a page holds",
            ));
        }
        Data::Union(data) => {
            let token = &data.union_token;
            return Err(syn::Error::new_spanned(
                quote!(#token #name),
                "`#[derive(FromHtml)]` supports structs with named fields, not unions",
            ));
        }
    };
    let type_selector = parse_type_attrs(&input.attrs)?;

    let specs = match fields {
        Fields::Named(named) => {
            let mut specs = Vec::new();
            let mut errors: Option<syn::Error> = None;
            for field in &named.named {
                match FieldSpec::parse(field) {
                    Ok(spec) => specs.push(spec),
                    Err(e) => match &mut errors {
                        Some(all) => all.combine(e),
                        None => errors = Some(e),
                    },
                }
            }
            if let Some(errors) = errors {
                return Err(errors);
            }
            Some(specs)
        }
        Fields::Unit => None,
        Fields::Unnamed(unnamed) => {
            return Err(syn::Error::new_spanned(
                quote!(#name #unnamed),
                "`#[derive(FromHtml)]` supports structs with named fields, not tuple structs: \
                 name the fields so each can say what it reads with `#[html(...)]`",
            ));
        }
    };

    let type_name = name.unraw().to_string();
    let type_params: Vec<Ident> = input
        .generics
        .type_params()
        .map(|p| p.ident.clone())
        .collect();
    let mut generics = input.generics.clone();
    let mut bounds: Vec<WherePredicate> = Vec::new();

    let selector = type_selector.map(|lit| {
        quote! {
            const SELECTOR: ::domq::__private::Option<&'static str> =
                ::domq::__private::Some(#lit);
        }
    });

    let mut checks = TokenStream2::new();
    let body = match specs {
        None => quote!(::domq::__private::Ok(Self)),
        Some(specs) => {
            let mut inits = Vec::new();
            for spec in &specs {
                let (init, bound, check) = spec.expand(&type_name);
                let ident = &spec.ident;
                inits.push(quote!(#ident: #init));
                if let Some((ty, bound)) = bound
                    && mentions_any(&ty, &type_params)
                {
                    bounds.push(parse_quote!(#ty: #bound));
                }
                checks.extend(check);
            }
            quote!(::domq::__private::Ok(Self { #(#inits),* }))
        }
    };
    if !bounds.is_empty() {
        generics.make_where_clause().predicates.extend(bounds);
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::domq::FromHtml for #name #ty_generics #where_clause {
            #selector

            fn from_html(
                __scope: ::domq::Scope<'_>,
            ) -> ::domq::__private::Result<Self, ::domq::ExtractError> {
                #checks
                #body
            }
        }
    })
}

/// The struct-level `#[html(select = "…")]`.
fn parse_type_attrs(attrs: &[Attribute]) -> syn::Result<Option<LitStr>> {
    let mut select: Option<LitStr> = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("html")) {
        if let Meta::Path(_) = attr.meta {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("select") {
                if select.is_some() {
                    return Err(meta.error("duplicate `select`"));
                }
                select = Some(selector_lit(&meta)?);
                Ok(())
            } else {
                let key = key_name(&meta);
                let mut msg = format!("unknown key `{key}` on a struct: only `select` goes here");
                if FIELD_KEYS.contains(&key.as_str()) {
                    msg.push_str(&format!(" (`{key}` belongs on a field)"));
                }
                Err(meta.error(msg))
            }
        })?;
    }
    Ok(select)
}

const FIELD_KEYS: &[&str] = &[
    "select",
    "attr",
    "html",
    "outer_html",
    "markdown",
    "default",
    "with",
    "skip",
];

fn key_name(meta: &ParseNestedMeta<'_>) -> String {
    match meta.path.get_ident() {
        Some(ident) => ident.unraw().to_string(),
        None => meta.path.to_token_stream().to_string().replace(' ', ""),
    }
}

/// A non-empty string literal after `select =`.
fn selector_lit(meta: &ParseNestedMeta<'_>) -> syn::Result<LitStr> {
    let lit: LitStr = meta.value()?.parse()?;
    if lit.value().trim().is_empty() {
        return Err(syn::Error::new(
            lit.span(),
            "`select` needs a CSS selector; leave it out to read the scope itself",
        ));
    }
    Ok(lit)
}

/// A key that takes no value (`html`, `default`, …).
fn flag(meta: &ParseNestedMeta<'_>, key: &str) -> syn::Result<()> {
    if meta.input.peek(Token![=]) || meta.input.peek(syn::token::Paren) {
        return Err(meta.error(format!("`{key}` takes no value: write just `{key}`")));
    }
    Ok(())
}

fn unknown_key(key: &str) -> String {
    let hint = match key {
        "selector" | "css" | "query" | "sel" => "did you mean `select`?",
        "attribute" | "attrs" => "did you mean `attr = \"name\"`?",
        "inner_html" | "inner" => "did you mean `html` (the inner HTML)?",
        "outer" | "outerhtml" | "outerHTML" => "did you mean `outer_html`?",
        "md" => "did you mean `markdown`?",
        "text" => "the readable text is what a field reads by default: leave the key out",
        "optional" => "make the field an `Option<T>` instead",
        "parse" | "map" | "convert" | "parse_with" => "did you mean `with = \"path::to::fn\"`?",
        "ignore" => "did you mean `skip`?",
        _ => "",
    };
    if hint.is_empty() {
        format!("unknown key `{key}`; expected one of {KEYS}")
    } else {
        format!("unknown key `{key}`: {hint}")
    }
}

/// What a field reads from the element it selects.
enum Source {
    Attr(String),
    Html,
    OuterHtml,
    Markdown,
}

impl Source {
    fn key(&self) -> &'static str {
        match self {
            Source::Attr(_) => "attr",
            Source::Html => "html",
            Source::OuterHtml => "outer_html",
            Source::Markdown => "markdown",
        }
    }
}

/// How many matches a field takes, from its type.
enum Container<'a> {
    One,
    Option(&'a Type),
    Vec(&'a Type),
}

/// `Option<T>` / `Vec<T>` (any path ending in those names) and their `T`.
fn container(ty: &Type) -> Container<'_> {
    match wrapper(ty) {
        Some(("Option", inner)) => Container::Option(inner),
        Some(("Vec", inner)) => Container::Vec(inner),
        _ => Container::One,
    }
}

fn wrapper(ty: &Type) -> Option<(&'static str, &Type)> {
    let ty = peel(ty);
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }
    let last = path.path.segments.last()?;
    let name = if last.ident == "Option" {
        "Option"
    } else if last.ident == "Vec" {
        "Vec"
    } else {
        return None;
    };
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    });
    match (types.next(), types.next()) {
        (Some(inner), None) => Some((name, inner)),
        _ => None,
    }
}

/// Look through invisible groups (types passed through `macro_rules!`) and
/// parentheses.
fn peel(mut ty: &Type) -> &Type {
    loop {
        match ty {
            Type::Group(g) => ty = &g.elem,
            Type::Paren(p) => ty = &p.elem,
            _ => return ty,
        }
    }
}

/// Whether `ty` mentions any of the struct's type parameters.
fn mentions_any(ty: &Type, params: &[Ident]) -> bool {
    fn walk(tokens: TokenStream2, params: &[Ident]) -> bool {
        tokens.into_iter().any(|tt| match tt {
            TokenTree::Ident(ident) => params.contains(&ident),
            TokenTree::Group(group) => walk(group.stream(), params),
            _ => false,
        })
    }
    !params.is_empty() && walk(ty.to_token_stream(), params)
}

/// One field and its `#[html(...)]` keys.
struct FieldSpec {
    ident: Ident,
    ty: Type,
    select: Option<LitStr>,
    source: Option<(Source, Span)>,
    default: Option<Span>,
    with: Option<(syn::Path, Span)>,
    skip: Option<Span>,
    /// Spans of the keys other than `skip` (to point at when combined).
    others: Vec<Span>,
}

impl FieldSpec {
    fn parse(field: &Field) -> syn::Result<FieldSpec> {
        let Some(ident) = field.ident.clone() else {
            return Err(syn::Error::new(field.span(), "expected a named field"));
        };
        let mut spec = FieldSpec {
            ident,
            ty: field.ty.clone(),
            select: None,
            source: None,
            default: None,
            with: None,
            skip: None,
            others: Vec::new(),
        };
        let mut seen = false;
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("html")) {
            seen = true;
            if let Meta::Path(_) = attr.meta {
                continue;
            }
            attr.parse_nested_meta(|meta| spec.key(&meta))?;
        }
        if !seen {
            let name = spec.ident.unraw();
            return Err(syn::Error::new(
                spec.ident.span(),
                format!(
                    "field `{name}` needs `#[html(...)]`: every field says what it reads, \
                     e.g. `#[html(select = \"h1\")]`; use `#[html(skip)]` to fill it with \
                     `Default::default()` instead"
                ),
            ));
        }
        spec.validate()?;
        Ok(spec)
    }

    /// Record one key of `#[html(...)]`.
    fn key(&mut self, meta: &ParseNestedMeta<'_>) -> syn::Result<()> {
        let key = key_name(meta);
        let span = meta.path.span();
        let duplicate = || meta.error(format!("duplicate `{key}`"));
        match key.as_str() {
            "select" => {
                if self.select.is_some() {
                    return Err(duplicate());
                }
                self.select = Some(selector_lit(meta)?);
            }
            "attr" => {
                let lit: LitStr = meta.value()?.parse()?;
                let name = lit.value();
                if name.trim().is_empty() || name.contains(|c: char| c.is_whitespace()) {
                    return Err(syn::Error::new(
                        lit.span(),
                        "`attr` needs an attribute name, like `attr = \"href\"`",
                    ));
                }
                // The parser lower-cases attribute names, as HTML does.
                self.set_source(meta, Source::Attr(name.to_ascii_lowercase()))?;
            }
            "html" => {
                flag(meta, &key)?;
                self.set_source(meta, Source::Html)?;
            }
            "outer_html" => {
                flag(meta, &key)?;
                self.set_source(meta, Source::OuterHtml)?;
            }
            "markdown" => {
                flag(meta, &key)?;
                self.set_source(meta, Source::Markdown)?;
            }
            "default" => {
                flag(meta, &key)?;
                if self.default.is_some() {
                    return Err(duplicate());
                }
                self.default = Some(span);
            }
            "skip" => {
                flag(meta, &key)?;
                if self.skip.is_some() {
                    return Err(duplicate());
                }
                self.skip = Some(span);
                return Ok(());
            }
            "with" => {
                if self.with.is_some() {
                    return Err(duplicate());
                }
                let lit: LitStr = meta.value()?.parse()?;
                let path: syn::Path = lit.parse().map_err(|_| {
                    syn::Error::new(
                        lit.span(),
                        "`with` takes the path of a function `fn(&str) -> Result<T, E>`, \
                         like `with = \"parse_price\"` or `with = \"my::parse\"`",
                    )
                })?;
                self.with = Some((path, lit.span()));
            }
            _ => return Err(meta.error(unknown_key(&key))),
        }
        self.others.push(span);
        Ok(())
    }

    fn set_source(&mut self, meta: &ParseNestedMeta<'_>, source: Source) -> syn::Result<()> {
        if let Some((first, _)) = &self.source {
            let (a, b) = (first.key(), source.key());
            let msg = if a == b {
                format!("duplicate `{b}`")
            } else {
                format!(
                    "a field reads one value, but it has both `{a}` and `{b}`; \
                     without either it reads the text"
                )
            };
            return Err(meta.error(msg));
        }
        self.source = Some((source, meta.path.span()));
        Ok(())
    }

    fn validate(&self) -> syn::Result<()> {
        if self.skip.is_some() {
            if let Some(&other) = self.others.first() {
                return Err(syn::Error::new(
                    other,
                    "`skip` fills the field with `Default::default()` and reads nothing, \
                     so it can't be combined with other keys",
                ));
            }
            return Ok(());
        }
        match container(&self.ty) {
            Container::Option(inner) | Container::Vec(inner) if wrapper(inner).is_some() => {
                Err(syn::Error::new_spanned(
                    &self.ty,
                    "nested `Option`/`Vec` field types are not supported: a field is one value \
                     (`T`), an optional one (`Option<T>`) or every match (`Vec<T>`); to collect \
                     only elements that have an attribute, select them with `[name]`",
                ))
            }
            Container::Option(_) => match self.default {
                Some(span) => Err(syn::Error::new(
                    span,
                    "`default` has no effect on an `Option` field: no match or a missing \
                     attribute is already `None`",
                )),
                None => Ok(()),
            },
            Container::Vec(_) => {
                if let Some(span) = self.default {
                    return Err(syn::Error::new(
                        span,
                        "`default` has no effect on a `Vec` field: no match is already an \
                         empty `Vec`",
                    ));
                }
                if self.select.is_none() {
                    return Err(syn::Error::new_spanned(
                        &self.ty,
                        "a `Vec` field needs `select = \"…\"`: the elements to read one item from",
                    ));
                }
                Ok(())
            }
            Container::One => Ok(()),
        }
    }

    /// The field's initializer, the trait bound it needs on its value type
    /// (for generic structs), and statements to emit before the struct
    /// literal.
    fn expand(
        &self,
        type_name: &str,
    ) -> (TokenStream2, Option<(Type, TokenStream2)>, TokenStream2) {
        let pv = quote!(::domq::__private);
        if self.skip.is_some() {
            let ty = &self.ty;
            return (
                quote!(<#ty as #pv::Default>::default()),
                Some((self.ty.clone(), quote!(#pv::Default))),
                TokenStream2::new(),
            );
        }

        let field_name = self.ident.unraw().to_string();
        let select = match &self.select {
            Some(lit) => quote!(#pv::Some(#lit)),
            None => quote!(#pv::None),
        };
        let mut check = TokenStream2::new();
        let src = match (&self.source, &self.with) {
            (Some((Source::Attr(name), _)), _) => Some(quote!(#pv::Src::Attr(#name))),
            (Some((Source::Html, _)), _) => Some(quote!(#pv::Src::InnerHtml)),
            (Some((Source::OuterHtml, _)), _) => Some(quote!(#pv::Src::OuterHtml)),
            (Some((Source::Markdown, span)), _) => {
                check = quote_spanned!(*span=> ::domq::__private::require_markdown!(););
                Some(quote!(#pv::Src::Markdown))
            }
            (None, Some(_)) => Some(quote!(#pv::Src::Text)),
            (None, None) => None,
        };
        let source = match &src {
            Some(src) => quote!(#pv::Some(#src)),
            None => quote!(#pv::None),
        };
        let ctx = quote! {
            &#pv::Ctx {
                ty: #type_name,
                field: #field_name,
                select: #select,
                source: #source,
            }
        };

        let container = container(&self.ty);
        let value_ty = match container {
            Container::One => &self.ty,
            Container::Option(inner) | Container::Vec(inner) => inner,
        };
        // Type errors (a type without `FromHtml`, a `with` function of the
        // wrong type) point at the field type or at the `with` value. Only
        // paths and closure-local names get that span: `__scope` keeps the
        // call-site span it was declared with, so hygiene can't separate them.
        let span = match &self.with {
            Some((_, span)) => *span,
            None => value_ty.span(),
        };
        let scope = Ident::new("__scope", Span::call_site());
        let (convert, bound) = match (&self.with, &src) {
            (Some((path, _)), _) => {
                // Errors name the function by the last segment of its path.
                let function = path
                    .segments
                    .last()
                    .map(|s| s.ident.unraw().to_string())
                    .unwrap_or_default();
                (
                    quote_spanned! {span=>
                        |__s, __c| ::domq::__private::with::<#value_ty, _>(
                            __s, __c, #function, #path,
                        )
                    },
                    None,
                )
            }
            (None, Some(_)) => (
                quote_spanned!(span=> ::domq::__private::value::<#value_ty>),
                Some((value_ty.clone(), quote!(#pv::FromHtmlValue))),
            ),
            (None, None) => (
                quote_spanned!(span=> ::domq::__private::nested::<#value_ty>),
                Some((value_ty.clone(), quote!(#pv::FromHtml))),
            ),
        };

        let init = match container {
            Container::Option(_) => {
                quote_spanned!(span=> ::domq::__private::opt(#scope, #ctx, #convert)?)
            }
            Container::Vec(_) => {
                quote_spanned!(span=> ::domq::__private::all(#scope, #ctx, #convert)?)
            }
            Container::One if self.default.is_some() => quote_spanned! {span=>
                ::domq::__private::Option::unwrap_or_default(
                    ::domq::__private::opt(#scope, #ctx, #convert)?
                )
            },
            Container::One => {
                quote_spanned!(span=> ::domq::__private::one(#scope, #ctx, #convert)?)
            }
        };
        let bound = match (bound, self.default.is_some()) {
            // `default` also needs `T: Default`.
            (Some((ty, b)), true) => Some((ty, quote!(#b + #pv::Default))),
            (None, true) => Some((self.ty.clone(), quote!(#pv::Default))),
            (bound, false) => bound,
        };
        (init, bound, check)
    }
}
