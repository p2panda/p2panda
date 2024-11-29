use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(Topic)]
pub fn topic_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_topic_macro(&ast)
}

fn impl_topic_macro(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let gen = quote! {
        impl p2panda_sync::Topic for #name {}
    };
    gen.into()
}

#[proc_macro_derive(TopicId)]
pub fn topic_id_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_topic_id_macro(&ast)
}

fn impl_topic_id_macro(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let field_member = match &ast.data {
        syn::Data::Struct(data_struct) => data_struct.fields.members(),
        _ => {
            panic!("TopicId derive macro can only be used on structs");
        }
    };

    let gen = quote! {
        impl p2panda_net::TopicId for #name {
            fn id(&self) -> [u8; 32] {
                pub use common_traits::AsBytes;
                let mut bytes = Vec::new();
                #(
                    bytes.extend_from_slice(self.#field_member.as_bytes());
                    bytes.extend_from_slice(&[0]);
                )*
                let hash = p2panda_core::Hash::new(&bytes);
                *hash.as_bytes()
            }
        }
    };
    gen.into()
}
