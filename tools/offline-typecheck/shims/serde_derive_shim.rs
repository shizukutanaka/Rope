//! `#[derive(Serialize)]` / `#[derive(Deserialize)]` の実装 (JSON 専用)。
//!
//! 空 impl を吐くだけだった旧版と違い、**フィールドごとの実コードを生成する**。
//! おかげで `core/` の serde round-trip / 後方互換 / 破損復旧テストが
//! この環境で**実際に走る**。
//!
//! 対応する属性はこのリポジトリが実際に使う 6 形のみ:
//! - コンテナ: `rename_all = "snake_case" | "lowercase"`, `tag = "..."`
//! - フィールド: `default`, `rename = "..."`
//!
//! 🔴 **実 serde の完全な再実装ではない。** 未対応の属性は**黙って無視される**
//! ので、新しい属性を使ったら CI で落ちる可能性がある (`../README.md`)。

extern crate proc_macro;

use proc_macro::{Delimiter, TokenStream, TokenTree};

// ============================================================================
// 最小のパーサ
// ============================================================================

#[derive(Default, Clone)]
struct Attrs {
    rename_all: Option<String>,
    rename: Option<String>,
    tag: Option<String>,
    default: bool,
}

/// `#[serde(...)]` の中身から必要なものだけ拾う。
fn parse_attrs(trees: &[TokenTree]) -> Attrs {
    let mut a = Attrs::default();
    let mut i = 0;
    while i < trees.len() {
        // `#` + `[serde(...)]`
        if let TokenTree::Punct(p) = &trees[i] {
            if p.as_char() == '#' {
                if let Some(TokenTree::Group(g)) = trees.get(i + 1) {
                    if g.delimiter() == Delimiter::Bracket {
                        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                        if let Some(TokenTree::Ident(id)) = inner.first() {
                            if id.to_string() == "serde" {
                                if let Some(TokenTree::Group(args)) = inner.get(1) {
                                    absorb(&mut a, &args.stream().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }
    a
}

fn absorb(a: &mut Attrs, s: &str) {
    for part in s.split(',') {
        let p = part.trim();
        if p == "default" {
            a.default = true;
        } else if let Some(v) = kv(p, "rename_all") {
            a.rename_all = Some(v);
        } else if let Some(v) = kv(p, "rename") {
            a.rename = Some(v);
        } else if let Some(v) = kv(p, "tag") {
            a.tag = Some(v);
        }
    }
}

fn kv(part: &str, key: &str) -> Option<String> {
    let mut it = part.splitn(2, '=');
    let k = it.next()?.trim();
    if k != key {
        return None;
    }
    Some(it.next()?.trim().trim_matches('"').to_string())
}

fn rename_all(name: &str, style: &Option<String>) -> String {
    match style.as_deref() {
        Some("snake_case") => {
            let mut out = String::new();
            for (i, c) in name.chars().enumerate() {
                if c.is_uppercase() {
                    if i > 0 {
                        out.push('_');
                    }
                    out.extend(c.to_lowercase());
                } else {
                    out.push(c);
                }
            }
            out
        }
        Some("lowercase") => name.to_lowercase(),
        _ => name.to_string(),
    }
}

struct Field {
    name: String,
    key: String,
    default: bool,
}

struct Variant {
    name: String,
    key: String,
    fields: Option<Vec<Field>>, // None = unit variant
}

enum Item {
    Struct { name: String, fields: Vec<Field> },
    Enum { name: String, tag: Option<String>, variants: Vec<Variant> },
}

/// 属性群 → 次の実体、という並びを 1 つ切り出す。
fn split_attrs(trees: &[TokenTree], i: &mut usize) -> Attrs {
    let start = *i;
    while *i < trees.len() {
        match &trees[*i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                *i += 2; // '#' と [..]
            }
            _ => break,
        }
    }
    parse_attrs(&trees[start..*i])
}

fn parse_fields(body: &[TokenTree], style: &Option<String>) -> Vec<Field> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let attrs = split_attrs(body, &mut i);
        // `pub` を読み飛ばす (pub(crate) 等は Group が続く)
        while let Some(TokenTree::Ident(id)) = body.get(i) {
            if id.to_string() == "pub" {
                i += 1;
                if let Some(TokenTree::Group(g)) = body.get(i) {
                    if g.delimiter() == Delimiter::Parenthesis {
                        i += 1;
                    }
                }
            } else {
                break;
            }
        }
        let name = match body.get(i) {
            Some(TokenTree::Ident(id)) => id.to_string(),
            _ => {
                i += 1;
                continue;
            }
        };
        i += 1;
        // `:` の後、次のトップレベル `,` まで型なので読み飛ばす
        if !matches!(body.get(i), Some(TokenTree::Punct(p)) if p.as_char() == ':') {
            continue;
        }
        i += 1;
        while i < body.len() {
            if let TokenTree::Punct(p) = &body[i] {
                if p.as_char() == ',' {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
        let key = attrs
            .rename
            .clone()
            .unwrap_or_else(|| rename_all(&name, style));
        out.push(Field {
            name,
            key,
            default: attrs.default,
        });
    }
    out
}

fn parse_item(input: TokenStream) -> Option<Item> {
    let trees: Vec<TokenTree> = input.into_iter().collect();
    let container = parse_attrs(&trees);

    let mut i = 0;
    let mut kind = None;
    while i < trees.len() {
        if let TokenTree::Ident(id) = &trees[i] {
            let s = id.to_string();
            if s == "struct" || s == "enum" {
                kind = Some(s);
                i += 1;
                break;
            }
        }
        i += 1;
    }
    let kind = kind?;
    let name = match trees.get(i)? {
        TokenTree::Ident(id) => id.to_string(),
        _ => return None,
    };
    i += 1;
    let body = trees.iter().skip(i).find_map(|t| match t {
        TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => Some(g.stream()),
        _ => None,
    })?;
    let body: Vec<TokenTree> = body.into_iter().collect();

    if kind == "struct" {
        // 構造体のフィールド名は rename_all の対象にしない
        // (このリポジトリは構造体側で rename_all を使っていない)
        Some(Item::Struct {
            name,
            fields: parse_fields(&body, &None),
        })
    } else {
        let mut variants = Vec::new();
        let mut j = 0;
        while j < body.len() {
            let attrs = split_attrs(&body, &mut j);
            let vname = match body.get(j) {
                Some(TokenTree::Ident(id)) => id.to_string(),
                _ => {
                    j += 1;
                    continue;
                }
            };
            j += 1;
            let fields = match body.get(j) {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                    j += 1;
                    Some(parse_fields(&inner, &None))
                }
                _ => None,
            };
            // 変種の後の `,`
            if matches!(body.get(j), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
                j += 1;
            }
            let key = attrs
                .rename
                .clone()
                .unwrap_or_else(|| rename_all(&vname, &container.rename_all));
            variants.push(Variant {
                name: vname,
                key,
                fields,
            });
        }
        Some(Item::Enum {
            name,
            tag: container.tag.clone(),
            variants,
        })
    }
}

// ============================================================================
// コード生成
// ============================================================================

fn gen_serialize(item: &Item) -> String {
    match item {
        Item::Struct { name, fields } => {
            let mut body = String::new();
            for f in fields {
                body.push_str(&format!(
                    "v.push((\"{}\".to_string(), ::serde::Serialize::to_json(&self.{})));",
                    f.key, f.name
                ));
            }
            format!(
                "impl ::serde::Serialize for {name} {{
                    fn to_json(&self) -> ::serde::Json {{
                        let mut v: Vec<(String, ::serde::Json)> = Vec::new();
                        {body}
                        ::serde::Json::Obj(v)
                    }}
                }}"
            )
        }
        Item::Enum { name, tag, variants } => {
            let mut arms = String::new();
            for var in variants {
                match &var.fields {
                    // 内部タグ付き enum のユニット変種は、実 serde と同じく
                    // `{"kind":"..."}` になる。裸の文字列にすると、同じ enum の
                    // 構造体変種と形が揃わず往復できない (実際に一度壊した)。
                    None => {
                        let expr = match tag {
                            Some(t) => format!(
                                "::serde::Json::Obj(vec![(\"{t}\".to_string(), ::serde::Json::Str(\"{k}\".to_string()))])",
                                k = var.key
                            ),
                            None => format!(
                                "::serde::Json::Str(\"{k}\".to_string())",
                                k = var.key
                            ),
                        };
                        arms.push_str(&format!("{name}::{v} => {expr},", v = var.name));
                    }
                    Some(fields) => {
                        let binds: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                        let mut push = String::new();
                        if let Some(t) = tag {
                            push.push_str(&format!(
                                "v.push((\"{t}\".to_string(), ::serde::Json::Str(\"{k}\".to_string())));",
                                k = var.key
                            ));
                        }
                        for f in fields {
                            push.push_str(&format!(
                                "v.push((\"{}\".to_string(), ::serde::Serialize::to_json({})));",
                                f.key, f.name
                            ));
                        }
                        let inner = format!(
                            "{{ let mut v: Vec<(String, ::serde::Json)> = Vec::new(); {push} ::serde::Json::Obj(v) }}"
                        );
                        // tag 無しは serde 既定の externally tagged
                        let expr = if tag.is_some() {
                            inner
                        } else {
                            format!(
                                "::serde::Json::Obj(vec![(\"{k}\".to_string(), {inner})])",
                                k = var.key
                            )
                        };
                        arms.push_str(&format!(
                            "{name}::{v} {{ {b} }} => {expr},",
                            v = var.name,
                            b = binds.join(", ")
                        ));
                    }
                }
            }
            format!(
                "impl ::serde::Serialize for {name} {{
                    fn to_json(&self) -> ::serde::Json {{
                        match self {{ {arms} }}
                    }}
                }}"
            )
        }
    }
}

fn field_read(f: &Field, src: &str) -> String {
    let missing = if f.default {
        "Default::default()".to_string()
    } else {
        format!("return Err(format!(\"必須フィールドが無い: {}\"))", f.key)
    };
    format!(
        "match {src}.get(\"{k}\") {{
            Some(x) => ::serde::de::DeserializeOwned::from_json(x)?,
            None => {missing},
        }}",
        k = f.key
    )
}

fn gen_deserialize(item: &Item) -> String {
    match item {
        Item::Struct { name, fields } => {
            let mut inits = String::new();
            for f in fields {
                inits.push_str(&format!("{}: {},", f.name, field_read(f, "v")));
            }
            format!(
                "impl<'de> ::serde::Deserialize<'de> for {name} {{
                    fn from_json(v: &::serde::Json) -> Result<Self, String> {{
                        if !matches!(v, ::serde::Json::Obj(_)) {{
                            return Err(\"オブジェクトを期待\".to_string());
                        }}
                        Ok({name} {{ {inits} }})
                    }}
                }}"
            )
        }
        Item::Enum { name, tag, variants } => {
            let mut unit_arms = String::new();
            let mut struct_arms = String::new();
            for var in variants {
                match &var.fields {
                    None => unit_arms.push_str(&format!(
                        "\"{k}\" => return Ok({name}::{v}),",
                        k = var.key,
                        v = var.name
                    )),
                    Some(fields) => {
                        let src = if tag.is_some() { "v" } else { "inner" };
                        let mut inits = String::new();
                        for f in fields {
                            inits.push_str(&format!("{}: {},", f.name, field_read(f, src)));
                        }
                        let pre = if tag.is_some() {
                            String::new()
                        } else {
                            format!(
                                "let inner = v.get(\"{k}\").ok_or(\"変種の中身が無い\")?;",
                                k = var.key
                            )
                        };
                        struct_arms.push_str(&format!(
                            "\"{k}\" => {{ {pre} return Ok({name}::{v} {{ {inits} }}); }},",
                            k = var.key,
                            v = var.name
                        ));
                    }
                }
            }
            let discriminator = match tag {
                Some(t) => format!(
                    "let key = v.get(\"{t}\").and_then(|x| x.as_str()).ok_or(\"tag が無い\")?.to_string();"
                ),
                None => "let key = match v {
                        ::serde::Json::Str(s) => s.clone(),
                        ::serde::Json::Obj(kv) => kv.first().map(|(k, _)| k.clone())
                            .ok_or(\"空のオブジェクト\")?,
                        _ => return Err(\"文字列かオブジェクトを期待\".to_string()),
                    };"
                .to_string(),
            };
            format!(
                "impl<'de> ::serde::Deserialize<'de> for {name} {{
                    fn from_json(v: &::serde::Json) -> Result<Self, String> {{
                        {discriminator}
                        match key.as_str() {{
                            {unit_arms}
                            {struct_arms}
                            other => Err(format!(\"未知の変種: {{}}\", other)),
                        }}
                    }}
                }}"
            )
        }
    }
}

#[proc_macro_derive(Serialize, attributes(serde))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    match parse_item(input) {
        Some(item) => gen_serialize(&item).parse().unwrap(),
        None => TokenStream::new(),
    }
}

#[proc_macro_derive(Deserialize, attributes(serde))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    match parse_item(input) {
        Some(item) => gen_deserialize(&item).parse().unwrap(),
        None => TokenStream::new(),
    }
}
