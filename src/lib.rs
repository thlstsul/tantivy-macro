use darling::{ast, FromDeriveInput, FromField};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, parse_quote, DeriveInput, Type};

/// #[derive(Schema)]
/// struct Struct {
///     #[field(name="field_name", stored, indexed, coerce, norm)]
///     field: String
/// }
/// detail in test mod.
#[proc_macro_derive(Schema, attributes(field))]
pub fn derive_tantivy_schema(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let receiver = InputReceiver::from_derive_input(&input).unwrap();
    quote!(#receiver).into()
}

impl ToTokens for InputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();
        let name = &self.ident;
        let fields = self
            .data
            .as_ref()
            .take_struct()
            .expect("Should never be enum")
            .fields;

        let mut field_entrys = Vec::new();
        let mut field_values = Vec::new();
        for (id, field) in fields.iter().enumerate() {
            let (entry, value) = field.get_field_type_and_value(id as u32);
            field_entrys.push(entry);
            field_values.push(value);
        }

        tokens.extend(quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                pub fn schema() -> tantivy::schema::Schema {
                    let mut builder = tantivy::schema::Schema::builder();
                    #(
                        #field_entrys
                        builder.add_field(entry);
                    )*
                    builder.build()
                }
            }

            impl #impl_generics std::convert::Into<tantivy::schema::Document> for #name #ty_generics #where_clause {
                fn into(self) -> tantivy::schema::Document {
                    let mut document = tantivy::schema::Document::new();
                    #(
                        #field_values
                        document.add_field_value(field, value);
                    )*
                    document
                }
            }
        });
    }
}

#[derive(Debug, FromDeriveInput)]
#[darling(supports(struct_named))]
struct InputReceiver {
    ident: syn::Ident,
    generics: syn::Generics,
    data: ast::Data<(), FieldReceiver>,
}

/// #[field(name="name",fast,stored,indexed,coerce,norm,token)]
#[derive(Debug, FromField)]
#[darling(attributes(field))]
struct FieldReceiver {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    name: Option<String>,
    #[darling(default)]
    fast: bool,
    #[darling(default)]
    stored: bool,
    #[darling(default)]
    indexed: bool,
    #[darling(default)]
    coerce: bool,
    #[darling(default)]
    norm: bool,
    #[darling(default)]
    tokenized: bool,
}

impl FieldReceiver {
    /// -> (entry: tantivy::schema::FieldEntry, field: tantivy::schema::Field + value: tantivy::schema::Value)
    fn get_field_type_and_value(&self, id: u32) -> (TokenStream, TokenStream) {
        let FieldReceiver {
            ref ident,
            ref ty,
            ref name,
            fast,
            stored,
            indexed,
            coerce,
            norm,
            tokenized,
        } = *self;

        let set_stored = if stored {
            quote! { let options = options.set_stored(); }
        } else {
            TokenStream::new()
        };

        let set_fast = if fast {
            quote! { let options = options.set_fast(); }
        } else {
            TokenStream::new()
        };

        let set_coerce = if coerce {
            quote! { let options = options.set_coerce(); }
        } else {
            TokenStream::new()
        };

        let set_indexed = if indexed {
            quote! { let options = options.set_indexed(); }
        } else {
            TokenStream::new()
        };

        let set_fieldnorm = if norm {
            quote! { let options = options.set_fieldnorm(); }
        } else {
            TokenStream::new()
        };

        let create_field = quote! {
            let field = tantivy::schema::Field::from_field_id(#id);
        };

        let field_name = ident
            .as_ref()
            .expect("only supported named struct")
            .to_string();
        let field_name = name.as_ref().unwrap_or(&field_name);

        let str_ty: Type = parse_quote!(String);
        let json_ty: Type = parse_quote!(Map<String, Value>);
        if *ty == str_ty || *ty == json_ty {
            // indexed default
            let set_tokenizer = if tokenized {
                quote! { let index_options = index_options.set_tokenizer("default").set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions); }
            } else {
                quote! { let index_options = index_options.set_tokenizer("raw").set_index_option(tantivy::schema::IndexRecordOption::Basic); }
            };
            let set_fast = if fast {
                quote! { let options = options.set_fast(None); }
            } else {
                TokenStream::new()
            };
            return if *ty == str_ty {
                (
                    quote! {
                        let index_options = tantivy::schema::TextFieldIndexing::default().set_fieldnorms(#norm);
                        #set_tokenizer
                        let options = tantivy::schema::TextOptions::default().set_indexing_options(index_options);
                        #set_stored
                        #set_fast
                        #set_coerce
                        let entry = tantivy::schema::FieldEntry::new(
                            String::from(#field_name),
                            tantivy::schema::FieldType::Str(options)
                        );
                    },
                    quote! {
                        #create_field
                        let value = tantivy::schema::Value::Str(self.#ident);
                    },
                )
            } else {
                // json
                (
                    quote! {
                        let index_options = tantivy::schema::TextFieldIndexing::default().set_fieldnorms(#norm);
                        #set_tokenizer
                        let options = tantivy::schema::JsonObjectOptions::default().set_indexing_options(index_options);
                        #set_stored
                        #set_fast
                        let entry = tantivy::schema::FieldEntry::new(
                            String::from(#field_name),
                            tantivy::schema::FieldType::JsonObject(options)
                        );
                    },
                    quote! {
                        #create_field
                        let value = tantivy::schema::Value::JsonObject(self.#ident);
                    },
                )
            };
        }
        let u64_ty: Type = parse_quote!(u64);
        let i64_ty: Type = parse_quote!(i64);
        let f64_ty: Type = parse_quote!(f64);
        let bool_ty: Type = parse_quote!(bool);
        if *ty == u64_ty || *ty == i64_ty || *ty == f64_ty || *ty == bool_ty {
            let options = quote! {
                let options = tantivy::schema::NumericOptions::default();
                #set_stored
                #set_fast
                #set_coerce
                #set_indexed
                #set_fieldnorm
            };
            return if *ty == u64_ty {
                (
                    quote! {
                        #options
                        let entry = tantivy::schema::FieldEntry::new(
                            String::from(#field_name),
                            tantivy::schema::FieldType::U64(options)
                        );
                    },
                    quote! {
                        #create_field
                        let value = tantivy::schema::Value::U64(self.#ident);
                    },
                )
            } else if *ty == i64_ty {
                (
                    quote! {
                        #options
                        let ty = tantivy::schema::FieldType::I64(options);
                        let entry = tantivy::schema::FieldEntry::new(
                            String::from(#field_name),
                            options
                        );
                    },
                    quote! {
                        #create_field
                        let value = tantivy::schema::Value::I64(self.#ident);
                    },
                )
            } else if *ty == f64_ty {
                (
                    quote! {
                        #options
                        let entry = tantivy::schema::FieldEntry::new(
                            String::from(#field_name),
                            tantivy::schema::FieldType::F64(options)
                        );
                    },
                    quote! {
                        #create_field
                        let value = tantivy::schema::Value::F64(self.#ident);
                    },
                )
            } else {
                // bool
                (
                    quote! {
                        #options
                        let entry = tantivy::schema::FieldEntry::new(
                            String::from(#field_name),
                            tantivy::schema::FieldType::Bool(options)
                        );
                    },
                    quote! {
                        #create_field
                        let value = tantivy::schema::Value::Bool(self.#ident);
                    },
                )
            };
        }
        let bytes_ty: Type = parse_quote!(Vec<u8>);
        if *ty == bytes_ty {
            return (
                quote! {
                    let options = tantivy::schema::BytesOptions::default();
                    #set_stored
                    #set_fast
                    #set_indexed
                    #set_fieldnorm
                    let entry = tantivy::schema::FieldEntry::new(
                        String::from(#field_name),
                        tantivy::schema::FieldType::Bytes(options)
                    );
                },
                quote! {
                    #create_field
                    let value = tantivy::schema::Value::Bytes(self.#ident);
                },
            );
        }
        let facet_ty: Type = parse_quote!(Facet);
        if *ty == facet_ty {
            return (
                quote! {
                    let options = tantivy::schema::FacetOptions::default();
                    #set_stored
                    let entry = tantivy::schema::FieldEntry::new(
                        String::from(#field_name),
                        tantivy::schema::FieldType::Facet(options)
                    );
                },
                quote! {
                    #create_field
                    let value = tantivy::schema::Value::Facet(#ident);
                },
            );
        }
        let date_ty: Type = parse_quote!(DateTime);
        if *ty == date_ty {
            return (
                quote! {
                    let options = tantivy::schema::DateOptions::default();
                    #set_stored
                    #set_fast
                    #set_indexed
                    #set_fieldnorm
                    let entry = tantivy::schema::FieldEntry::new(
                        String::from(#field_name),
                        tantivy::schema::FieldType::Date(options)
                    );
                },
                quote! {
                    #create_field
                    let value = tantivy::schema::Value::Date(self.#ident);
                },
            );
        }
        let ip_ty: Type = parse_quote!(Ipv6Addr);
        if *ty == ip_ty {
            return (
                quote! {
                    let options = tantivy::schema::IpAddrOptions::default();
                    #set_stored
                    #set_fast
                    #set_indexed
                    #set_fieldnorm
                    let entry = tantivy::schema::FieldEntry::new(
                        String::from(#field_name),
                        tantivy::schema::FieldType::IpAddr(options)
                    );
                },
                quote! {
                    #create_field
                    let value = tantivy::schema::Value::IpAddr(self.#ident);
                },
            );
        }

        match ty {
            _ => panic!("unsupported field type"),
        }
    }
}

mod test {
    #[test]
    fn should_work() {
        let input = r#"#[derive(Schema)]
pub struct Doc {
    #[field(name = "str", stored, tokenized)]
    text: String,
    #[field(fast)]
    id: String,
    #[field(fast, norm, coerce)]
    num: u64,
    #[field(stored, fast)]
    date: DateTime,
    #[field(stored, indexed)]
    facet: Facet,
    #[field(stored, indexed)]
    bytes: Vec<u8>,
    #[field(stored, indexed)]
    json: Map<String, Value>,
    #[field(fast)]
    ip: Ipv6Addr
}"#;

        let parsed = syn::parse_str(input).unwrap();
        use darling::FromDeriveInput;
        let receiver = crate::InputReceiver::from_derive_input(&parsed).unwrap();
        let tokens = quote::quote!(#receiver);

        println!(
            r#"
INPUT:

{}

PARSED AS:

{:?}

EMITS:

{}
    "#,
            input, receiver, tokens
        );

        let result = r#"impl Doc { pub fn schema () -> tantivy :: schema :: Schema { let mut builder = tantivy :: schema :: Schema :: builder () ; let index_options = tantivy :: schema :: TextFieldIndexing :: default () . set_fieldnorms (false) ; let index_options = index_options . set_tokenizer ("default") . set_index_option (tantivy :: schema :: IndexRecordOption :: WithFreqsAndPositions) ; let options = tantivy :: schema :: TextOptions :: default () . set_indexing_options (index_options) ; let options = options . set_stored () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("str") , tantivy :: schema :: FieldType :: Str (options)) ; builder . add_field (entry) ; let index_options = tantivy :: schema :: TextFieldIndexing :: default () . set_fieldnorms (false) ; let index_options = index_options . set_tokenizer ("raw") . set_index_option (tantivy :: schema :: IndexRecordOption :: Basic) ; let options = tantivy :: schema :: TextOptions :: default () . set_indexing_options (index_options) ; let options = options . set_fast (None) ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("id") , tantivy :: schema :: FieldType :: Str (options)) ; builder . add_field (entry) ; let options = tantivy :: schema :: NumericOptions :: default () ; let options = options . set_fast () ; let options = options . set_coerce () ; let options = options . set_fieldnorm () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("num") , tantivy :: schema :: FieldType :: U64 (options)) ; builder . add_field (entry) ; let options = tantivy :: schema :: DateOptions :: default () ; let options = options . set_stored () ; let options = options . set_fast () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("date") , tantivy :: schema :: FieldType :: Date (options)) ; builder . add_field (entry) ; let options = tantivy :: schema :: FacetOptions :: default () ; let options = options . set_stored () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("facet") , tantivy :: schema :: FieldType :: Facet (options)) ; builder . add_field (entry) ; let options = tantivy :: schema :: BytesOptions :: default () ; let options = options . set_stored () ; let options = options . set_indexed () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("bytes") , tantivy :: schema :: FieldType :: Bytes (options)) ; builder . add_field (entry) ; let index_options = tantivy :: schema :: TextFieldIndexing :: default () . set_fieldnorms (false) ; let index_options = index_options . set_tokenizer ("default") . set_index_option (tantivy :: schema :: IndexRecordOption :: WithFreqsAndPositions) ; let options = tantivy :: schema :: JsonObjectOptions :: default () . set_indexing_options (index_options) ; let options = options . set_stored () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("json") , tantivy :: schema :: FieldType :: JsonObject (options)) ; builder . add_field (entry) ; let options = tantivy :: schema :: IpAddrOptions :: default () ; let options = options . set_fast () ; let entry = tantivy :: schema :: FieldEntry :: new (String :: from ("ip") , tantivy :: schema :: FieldType :: IpAddr (options)) ; builder . add_field (entry) ; builder . build () } } impl std :: convert :: Into < tantivy :: schema :: Document > for Doc { fn into (self) -> tantivy :: schema :: Document { let mut document = tantivy :: schema :: Document :: new () ; let field = tantivy :: schema :: Field :: from_field_id (0u32) ; let value = tantivy :: schema :: Value :: Str (self . text) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (1u32) ; let value = tantivy :: schema :: Value :: Str (self . id) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (2u32) ; let value = tantivy :: schema :: Value :: U64 (self . num) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (3u32) ; let value = tantivy :: schema :: Value :: Date (self . date) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (4u32) ; let value = tantivy :: schema :: Value :: Facet (facet) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (5u32) ; let value = tantivy :: schema :: Value :: Bytes (self . bytes) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (6u32) ; let value = tantivy :: schema :: Value :: JsonObject (self . json) ; document . add_field_value (field , value) ; let field = tantivy :: schema :: Field :: from_field_id (7u32) ; let value = tantivy :: schema :: Value :: IpAddr (self . ip) ; document . add_field_value (field , value) ; document } }"#;
        assert_eq!(result, tokens.to_string())
    }
}
