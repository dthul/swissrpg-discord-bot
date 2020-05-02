#![deny(rust_2018_idioms)]

// #[allow(unused_extern_crates)]
// extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Attribute, ItemFn, Token,
};

#[inline]
fn into_stream(e: syn::Error) -> TokenStream {
    e.to_compile_error().into()
}

macro_rules! propagate_err {
    ($res:expr) => {{
        match $res {
            Ok(v) => v,
            Err(e) => return into_stream(e),
        }
    }};
}

#[derive(Debug)]
struct CommandFun {
    /// `#[...]`-style attributes
    pub attributes: Vec<Attribute>,
    /// The function itself
    pub fun: ItemFn,
}

impl Parse for CommandFun {
    fn parse(input: ParseStream<'_>) -> syn::parse::Result<Self> {
        let attributes = input.call(Attribute::parse_outer)?;
        Ok(CommandFun {
            attributes,
            fun: input.parse()?,
        })
    }
}

struct RegexAttribute {
    format_str: syn::LitStr,
    format_args: Vec<syn::Ident>,
}

impl Parse for RegexAttribute {
    fn parse(input: ParseStream<'_>) -> syn::parse::Result<Self> {
        let format_str: syn::LitStr = input.parse()?;
        let format_args = if !input.is_empty() {
            let _: Token![,] = input.parse()?;
            let idents = Punctuated::<syn::Ident, Token![,]>::parse_terminated(input)?;
            idents.into_iter().collect()
        } else {
            vec![]
        };
        Ok(RegexAttribute {
            format_str,
            format_args,
        })
    }
}

#[proc_macro_attribute]
pub fn command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let command_fun = parse_macro_input!(item as CommandFun);
    // for input in &command_fun.fun.sig.inputs {
    //     println!("Input: {:#?}", input);
    //     if let syn::FnArg::Typed(syn::PatType { ty, .. }) = input {
    //         if let syn::Type::Path(syn::TypePath { path, .. }) = &**ty {
    //             let redis_client_path: syn::TypePath = syn::parse_str("redis::Client")
    //                 .expect("Path redis::Client could not be parsed");
    //             println!("Path: {:#?}", path);
    //             if path == &redis_client_path.path {
    //                 println!("Matches!");
    //             }
    //         }
    //     }
    // }
    // println!("attr: \"{}\"", attr.to_string());
    // println!("fun attr: \"{:#?}\"", command_fun.attributes);
    // println!("fun: \"{:#?}\"", command_fun.fun);
    // println!("item: \"{}\"", item.to_string());

    let mut command_regex = None;
    let mut command_level = None;
    let mut unknown_attrs = vec![];
    let mut help_text = None;
    for attribute in &command_fun.attributes {
        // let meta_attribute = propagate_err!(attribute.parse_meta());
        let attr_ident = match attribute.path.get_ident() {
            Some(ident) => ident,
            None => {
                unknown_attrs.push(attribute);
                continue;
            }
        };
        match attr_ident.to_string().as_ref() {
            "level" => {
                let parser = |input: ParseStream<'_>| -> syn::Result<_> {
                    // let _: Token![=] = input.parse()?;
                    let level: syn::Ident = input.parse()?;
                    Ok(level)
                };
                let level = propagate_err!(attribute.parse_args_with(parser));
                if command_level.is_some() {
                    panic!("Multiple levels specified for the same command");
                }
                command_level = Some(level.to_string());
            }
            "regex" => {
                let regex_attribute = propagate_err!(attribute.parse_args::<RegexAttribute>());
                if command_regex.is_some() {
                    panic!("Multiple command regexes specified for the same command");
                }
                command_regex = Some((regex_attribute.format_str, regex_attribute.format_args));
            }
            "help" => {
                let parser = |input: ParseStream<'_>| -> syn::Result<syn::LitStr> { input.parse() };
                let text = propagate_err!(attribute.parse_args_with(parser));
                if help_text.is_some() {
                    panic!("Multiple help texts specified for the same command");
                }
                help_text = Some(text);
            }
            _ => {
                unknown_attrs.push(attribute);
                continue;
            }
        }
    }

    let (command_regex_format_str, command_regex_format_args) =
        command_regex.expect("Command specifies no regex");

    // Check whether this is a valid regex
    // if let Err(err) = regex::Regex::new(&command_regex) {
    //     panic!("Invalid regex \"{}\":\n{:#?}", command_regex, err);
    // }

    let command_level = match command_level.as_ref().map(String::as_str) {
        None => quote!(crate::discord::commands::CommandLevel::Everybody),
        Some("admin") => quote!(crate::discord::commands::CommandLevel::AdminOnly),
        Some("host") => quote!(crate::discord::commands::CommandLevel::HostAndAdminOnly),
        Some(level) => panic!("Invalid command level \"{}\"", level),
    };

    let fun_ident = command_fun.fun.sig.ident.clone();
    let regex_fun_ident = format_ident!("{}_regex", fun_ident.to_string());
    let static_instance_name = format_ident!("{}_COMMAND", fun_ident.to_string().to_uppercase());
    let fun = command_fun.fun;
    let command_struct_path = quote!(crate::discord::commands::Command);
    let help_text = if let Some(help_text) = help_text {
        quote! { Some(#help_text) }
    } else {
        quote! { None }
    };
    let output = quote! {
        #(#unknown_attrs)*
        #fun

        pub(crate) fn #regex_fun_ident(regex_parts: &crate::discord::commands::RegexParts) -> String {
            format!(#command_regex_format_str, #(#command_regex_format_args = regex_parts.#command_regex_format_args,)*)
        }

        pub(crate) static #static_instance_name: #command_struct_path = #command_struct_path {
            regex: #regex_fun_ident,
            level: #command_level,
            fun: #fun_ident,
            help: #help_text,
        };
    };
    // println!("{}", output.to_string());
    output.into()
}
