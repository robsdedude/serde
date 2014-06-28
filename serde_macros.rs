#![crate_type = "dylib"]
#![crate_type = "rlib"]

#![feature(plugin_registrar)]

extern crate syntax;
extern crate rustc;

use std::gc::Gc;

use syntax::ast::{MetaItem, Item, Expr, MutMutable, LitNil};
use syntax::ast;
use syntax::codemap::Span;
use syntax::ext::base::{ExtCtxt, ItemDecorator};
use syntax::ext::build::AstBuilder;
use syntax::ext::deriving::generic::{MethodDef, EnumMatching, FieldInfo, Struct, Substructure, TraitDef, combine_substructure};
use syntax::ext::deriving::generic::ty::{Borrowed, LifetimeBounds, Literal, Path, Ptr, Tuple, borrowed_explicit_self};
use syntax::parse::token;

use rustc::plugin::Registry;

#[plugin_registrar]
#[doc(hidden)]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_syntax_extension(
        token::intern("deriving_serializable"),
        ItemDecorator(derive_serialize));
}

fn derive_serialize(cx: &mut ExtCtxt,
                    sp: Span,
                    mitem: Gc<MetaItem>,
                    item: Gc<Item>,
                    push: |Gc<ast::Item>|) {
    let inline = cx.meta_word(sp, token::InternedString::new("inline"));
    let attrs = vec!(cx.attribute(sp, inline));

    let trait_def = TraitDef {
        span: sp,
        attributes: vec!(),
        path: Path::new(vec!("serde", "ser", "Serializable")),
        additional_bounds: Vec::new(),
        generics: LifetimeBounds::empty(),
        methods: vec!(
            MethodDef {
                name: "serialize",
                generics: LifetimeBounds {
                    lifetimes: Vec::new(),
                    bounds: vec!(("__S", ast::StaticSize, vec!(Path::new_(
                                    vec!("serde", "ser", "Serializer"), None,
                                    vec!(box Literal(Path::new_local("__E"))), true))),
                                 ("__E", ast::StaticSize, vec!()))
                },
                explicit_self: borrowed_explicit_self(),
                args: vec!(Ptr(box Literal(Path::new_local("__S")),
                            Borrowed(None, MutMutable))),
                ret_ty: Literal(Path::new_(vec!("std", "result", "Result"),
                                           None,
                                           vec!(box Tuple(Vec::new()),
                                                box Literal(Path::new_local("__E"))),
                                           true)),
                attributes: attrs,
                const_nonmatching: true,
                combine_substructure: combine_substructure(|a, b, c| {
                    serializable_substructure(a, b, c)
                }),
            })
    };

    trait_def.expand(cx, mitem, item, push)
}

fn serializable_substructure(cx: &mut ExtCtxt, trait_span: Span,
                          substr: &Substructure) -> Gc<Expr> {
    let serializer = substr.nonself_args[0];

    return match *substr.fields {
        Struct(ref fields) => {
            if fields.is_empty() {
                // unit structs have no fields and need to return `Ok()`
                cx.expr_ok(trait_span, cx.expr_lit(trait_span, LitNil))
            } else {
                let mut stmts: Vec<Gc<syntax::codemap::Spanned<syntax::ast::Stmt_>>> = Vec::new();

                let call = cx.expr_method_call(
                    trait_span,
                    serializer,
                    cx.ident_of("serialize_struct_start"),
                    vec!(
                        cx.expr_str(trait_span, token::get_ident(substr.type_ident)),
                        cx.expr_uint(trait_span, fields.len()),
                    )
                );
                let call = cx.expr_try(trait_span, call);
                stmts.push(cx.stmt_expr(call));

                let emit_struct_sep = cx.ident_of("serialize_struct_sep");

                for (i, &FieldInfo {
                        name,
                        self_,
                        span,
                        ..
                    }) in fields.iter().enumerate() {
                    let name = match name {
                        Some(id) => token::get_ident(id),
                        None => {
                            token::intern_and_get_ident(format!("_field{}",
                                                                i).as_slice())
                        }
                    };
                    let call = cx.expr_method_call(span,
                                                   serializer,
                                                   emit_struct_sep,
                                                   vec!(
                                                       cx.expr_str(span, name),
                                                       cx.expr_addr_of(span, self_),
                                                    ));

                    let call = cx.expr_try(span, call);
                    stmts.push(cx.stmt_expr(call));
                }

                let call = cx.expr_method_call(trait_span,
                                               serializer,
                                               cx.ident_of("serialize_struct_end"),
                                               vec!());

                cx.expr_block(cx.block(trait_span, stmts, Some(call)))
            }
        }

        EnumMatching(_idx, variant, ref fields) => {
            let mut stmts = vec!();

            let call = cx.expr_method_call(
                trait_span,
                serializer,
                cx.ident_of("serialize_enum_start"),
                vec!(
                    cx.expr_str(trait_span, token::get_ident(substr.type_ident)),
                    cx.expr_str(trait_span, token::get_ident(variant.node.name)),
                    cx.expr_uint(trait_span, fields.len()),
                )
            );

            let call = cx.expr_try(trait_span, call);
            stmts.push(cx.stmt_expr(call));

            let serialize_struct_sep = cx.ident_of("serialize_enum_sep");

            for &FieldInfo { self_, span, .. } in fields.iter() {
                let call = cx.expr_method_call(
                    span,
                    serializer,
                    serialize_struct_sep,
                    vec!(
                        cx.expr_addr_of(span, self_),
                    )
                );

                let call = cx.expr_try(span, call);

                stmts.push(cx.stmt_expr(call));
            }

            let call = cx.expr_method_call(
                trait_span,
                serializer,
                cx.ident_of("serialize_enum_end"),
                vec!()
            );

            cx.expr_block(cx.block(trait_span, stmts, Some(call)))
        }

        _ => cx.bug("expected Struct or EnumMatching in deriving_serializable")
    }
}
