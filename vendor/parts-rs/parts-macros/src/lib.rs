use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Fields, FnArg, ItemFn, LitInt, Pat, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct NativeFunctionAttr {
    arity: Option<u8>,
}

impl Parse for NativeFunctionAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut arity = None;
        if !input.is_empty() {
            let id: syn::Ident = input.parse()?;
            if id == "arity" {
                input.parse::<Token![=]>()?;
                let lit: LitInt = input.parse()?;
                arity = Some(lit.base10_parse()?);
            }
        }
        Ok(NativeFunctionAttr { arity })
    }
}

#[proc_macro_attribute]
pub fn native_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as NativeFunctionAttr);
    let input = parse_macro_input!(item as ItemFn);

    let name = &input.sig.ident;
    let vis = &input.vis;
    let body = &input.block;
    let sig_args = &input.sig.inputs;

    let arity = attr.arity.unwrap_or(sig_args.len() as u8);
    let internal_name = format_ident!("{}_internal", name);

    // Check if it's a "raw" function: fn(args: &[Value]) or fn(args: Vec<Value>)
    let is_raw = if sig_args.len() == 1 {
        if let FnArg::Typed(pat_type) = &sig_args[0] {
            match &*pat_type.ty {
                syn::Type::Reference(_) => true,
                syn::Type::Path(type_path) => {
                    let seg = &type_path.path.segments.last().unwrap().ident;
                    seg == "Vec" || seg == "slice"
                }
                _ => false,
            }
        } else {
            false
        }
    } else {
        false
    };
    let wrapper_body = if is_raw {
        if let Some(expected_arity) = attr.arity {
            quote! {
                if args.len() != #expected_arity as usize {
                    return Err(format!("Expected {} arguments, got {}", #expected_arity, args.len()));
                }
                #body
            }
        } else {
            quote! { #body }
        }
    } else {
        let arg_extractions = sig_args.iter().enumerate().map(|(i, arg)| {
            if let FnArg::Typed(pat_type) = arg {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let arg_name = &pat_ident.ident;
                    quote! {
                        let #arg_name = args[#i].clone();
                    }
                } else {
                    quote! {}
                }
            } else {
                quote! {}
            }
        });
        quote! {
            if args.len() != #arity as usize {
                return Err(format!("Expected {} arguments, got {}", #arity, args.len()));
            }
            #(#arg_extractions)*
            #body
        }
    };

    let generated = quote! {
        #[allow(non_snake_case)]
        #vis fn #internal_name(args: &[crate::value::Value]) -> Result<crate::value::Value, String> {
            #wrapper_body
        }

        // ZMIANA: Zamiast 'pub const', generujemy funkcję zwracającą strukturę.
        #[allow(non_snake_case)]
        #vis fn #name() -> crate::value::NativeFunction {
            crate::value::NativeFunction {
                name: stringify!(#name),
                arity: #arity,
                // Pakujemy wygenerowaną funkcję internal w Rc
                call: std::sync::Arc::new(#internal_name),
            }
        }
    };

    generated.into()
}

extern crate proc_macro;

#[proc_macro_derive(IntoPartsObject)]
pub fn derive_into_parts_object(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let fields = match &ast.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("IntoPartsObject can only be derived for structs with named fields"),
        },
        _ => panic!("IntoPartsObject can only be derived for structs"),
    };

    let inserts = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_name_str = field_name.as_ref().unwrap().to_string();

        quote! {
            map.insert(
                ::parts::value::Value::String(#field_name_str.to_string().into()).get_hash(),
                ::parts::value::IntoValue::into_value(self.#field_name)
            );
        }
    });

    let expanded = quote! {
        impl ::parts::value::IntoValue for #name {
            fn into_value(self) -> ::parts::value::Value {
                let mut map = ::rustc_hash::FxHashMap::default();

                #(#inserts)*

                ::parts::value::Value::Object(::std::rc::Rc::new(::std::cell::RefCell::new(map)))
            }
        }
    };

    expanded.into()
}
#[proc_macro_derive(FromPartsObject)]
pub fn derive_from_parts_object(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let fields = match &ast.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("FromPartsObject can only be derived for structs with named fields"),
        },
        _ => panic!("FromPartsObject can only be derived for structs"),
    };

    let field_inits = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_name_str = field_name.as_ref().unwrap().to_string();

        quote! {
            #field_name: ::parts::value::FromValue::from_value(&get_val(#field_name_str))?
        }
    });

    let expanded = quote! {
        impl ::parts::value::FromValue for #name {
            fn from_value(val: &::parts::value::Value) -> ::std::result::Result<Self, ::std::string::String> {
                if let ::parts::value::Value::Object(obj_ref) = val {
                    let obj = obj_ref.borrow();

                    let get_val = |key: &str| {
                        let hash = ::parts::value::Value::String(key.to_string().into()).get_hash();
                        obj.get(&hash).cloned().unwrap_or(::parts::value::Value::Bool(false))
                    };

                    ::std::result::Result::Ok(#name {
                        #(#field_inits),*
                    })
                } else {
                    ::std::result::Result::Err(::std::format!("Expected an object to map to {}", stringify!(#name)))
                }
            }
        }
    };

    expanded.into()
}

#[proc_macro_attribute]
pub fn parts_native(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input as a standard Rust function
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let block = &input_fn.block;
    let ret_type = &input_fn.sig.output;

    let mut arg_extractors = Vec::new();
    let mut arg_names = Vec::new();
    let mut arg_types = Vec::new();
    let arg_count = input_fn.sig.inputs.len();

    // Iterate over the function arguments (e.g., `url: String`)
    for (i, arg) in input_fn.sig.inputs.iter().enumerate() {
        if let syn::FnArg::Typed(pat_type) = arg {
            let pat = &pat_type.pat; // The variable name
            let ty = &pat_type.ty; // The variable type

            arg_names.push(pat.clone());
            arg_types.push(ty.clone());

            // Generate the FromValue extraction logic for this specific argument
            arg_extractors.push(quote! {
                let #pat = <#ty as ::parts::value::FromValue>::from_value(
                    args.get(#i).ok_or_else(|| ::std::format!("Missing argument at index {}", #i))?
                )?;
            });
        } else {
            panic!("self methods are not supported for native functions");
        }
    }

    // Generate the final wrapper function
    let expanded = quote! {
        #fn_vis fn #fn_name(args: &[::parts::value::Value]) -> ::std::result::Result<::parts::value::Value, ::std::string::String> {
            // 1. Validate argument count
            if args.len() < #arg_count {
                return ::std::result::Result::Err(::std::format!(
                    "{} requires {} arguments, but got {}",
                    ::std::stringify!(#fn_name),
                    #arg_count,
                    args.len()
                ));
            }

            // 2. Extract arguments into native Rust types
            #(#arg_extractors)*

            // 3. Define the original logic as an inner function to preserve type inference
            fn inner(#(#arg_names: #arg_types),*) #ret_type {
                #block
            }

            // 4. Execute the logic and pack the result into a parts Value
            let result = inner(#(#arg_names),*)?;
            ::std::result::Result::Ok(::parts::value::IntoValue::into_value(result))
        }
    };

    expanded.into()
}
