//! `#[derive(Parser)]` / `#[derive(Subcommand)]` の型検査専用スタブ。
//!
//! **実 clap ではない。** 引数のパース規則 (`#[arg(long, default_value=...)]`
//! 等) は一切解釈しない — 属性を受理して空 impl を吐くだけ。
//! **CLI の引数仕様の誤りはこのハーネスでは検出できない** (`../README.md`)。

extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};

fn item_name(input: TokenStream) -> Option<String> {
    let mut saw_keyword = false;
    for tree in input {
        if let TokenTree::Ident(ident) = tree {
            let s = ident.to_string();
            if saw_keyword {
                return Some(s);
            }
            if s == "struct" || s == "enum" || s == "union" {
                saw_keyword = true;
            }
        }
    }
    None
}

fn emit(input: TokenStream, tmpl: &str) -> TokenStream {
    match item_name(input) {
        Some(name) => tmpl.replace("@NAME@", &name).parse().unwrap(),
        None => TokenStream::new(),
    }
}

#[proc_macro_derive(Parser, attributes(command, arg, clap, group))]
pub fn derive_parser(input: TokenStream) -> TokenStream {
    emit(input, "impl ::clap::Parser for @NAME@ {}")
}

#[proc_macro_derive(Subcommand, attributes(command, arg, clap, group))]
pub fn derive_subcommand(input: TokenStream) -> TokenStream {
    emit(input, "impl ::clap::Subcommand for @NAME@ {}")
}

#[proc_macro_derive(Args, attributes(command, arg, clap, group))]
pub fn derive_args(input: TokenStream) -> TokenStream {
    emit(input, "impl ::clap::Args for @NAME@ {}")
}

#[proc_macro_derive(ValueEnum, attributes(command, arg, clap, value))]
pub fn derive_value_enum(input: TokenStream) -> TokenStream {
    emit(input, "impl ::clap::ValueEnum for @NAME@ {}")
}
