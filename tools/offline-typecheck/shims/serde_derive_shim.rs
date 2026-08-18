//! `#[derive(Serialize)]` / `#[derive(Deserialize)]` の型検査専用スタブ。
//!
//! **実 serde ではない。** 本物の derive はフィールド 1 つずつに
//! `T: Serialize` 境界を課すが、ここは空 impl を吐くだけなので
//! **「フィールドが serde 不可」というエラーは検出できない**。
//! それ以外の型エラー (名前解決・型不一致・借用・網羅性) は本物と同じように
//! 検出される。詳細は `../README.md` の「検出できるもの / できないもの」。
//!
//! proc_macro は rustc に同梱されており crates.io を必要としない —
//! これがこのハーネス全体の成立条件。

extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};

/// トップレベルのトークン列から `struct` / `enum` / `union` の直後の識別子を拾う。
///
/// 属性 (`#[serde(...)]` 等) は `#` + Group で表現され、Group の中身は
/// トップレベル走査には現れないため、属性内の `struct` 等を誤検出しない。
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

fn emit(input: TokenStream, trait_impl: &str) -> TokenStream {
    match item_name(input) {
        Some(name) => trait_impl.replace("@NAME@", &name).parse().unwrap(),
        // 名前が取れない形 (ジェネリック型等) は静かに何も吐かない。
        // 本物なら impl が付くので、その型を境界付きで使っている箇所だけが
        // エラーになる — 黙って通すよりは「取りこぼした」と分かる方がよい。
        None => TokenStream::new(),
    }
}

#[proc_macro_derive(Serialize, attributes(serde))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    emit(input, "impl ::serde::Serialize for @NAME@ {}")
}

#[proc_macro_derive(Deserialize, attributes(serde))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    emit(input, "impl<'de> ::serde::Deserialize<'de> for @NAME@ {}")
}
