use std::fmt::Debug;

use cddl::ast::{
    GenericArg, GenericArgs, Group, GroupChoice, GroupEntry, Identifier, MemberKey, Occur,
    Occurrence, Operator, OptionalComma, RangeCtlOp, Rule, Type, Type1, Type2, TypeChoice,
    TypeRule, ValueMemberKeyEntry, CDDL,
};
use cddl::lexer::Lexer;
use cddl::parser::Parser;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validate_cbor_from_slice;

use crate::message::{MessageFields, MessageFieldsError};

use super::error::SchemaError;

pub enum FieldTypes {
    Str,
    Int,
    Float,
    Bool,
    Relation,
}

// Helper function for creating CDDL schema document
pub fn create_cddl(entries: Vec<(GroupEntry<'static>, OptionalComma<'static>)>) -> CDDL {
    CDDL {
        comments: None,
        rules: vec![Rule::Type {
            span: (0, 0, 0),
            comments_after_rule: None,
            rule: TypeRule {
                is_type_choice_alternate: false,
                name: Identifier {
                    ident: "my_rule".into(),
                    socket: None,
                    span: (0, 0, 0),
                },
                generic_params: None,
                value: Type {
                    type_choices: vec![TypeChoice {
                        type1: Type1 {
                            type2: create_map(entries),
                            operator: None,
                            span: (0, 0, 0),
                            comments_after_type: None,
                        },

                        comments_before_type: None,
                        comments_after_type: None,
                    }],
                    span: (0, 0, 0),
                },
                comments_before_assignt: None,
                comments_after_assignt: None,
            },
        }],
    }
}

// Helper function for creating CDDL map
// The cddl crate terminology is a little confusing,
// There are `Type` `Type1` and `Type2` structs and they represent
// a deeper level of nesting, in that order. A `Type2` struct represents
// the values in a key value pair.
pub fn create_map(entries: Vec<(GroupEntry<'static>, OptionalComma<'static>)>) -> Type2<'static> {
    Type2::Map {
        group: Group {
            group_choices: vec![GroupChoice {
                group_entries: entries.to_owned(),
                comments_before_grpchoice: None,
                span: (0, 0, 0),
            }],
            span: (0, 0, 0),
        },
        span: (0, 0, 0),
        comments_before_group: None,
        comments_after_group: None,
    }
}

pub fn create_regex_operator(regex_str: &'static str) -> Operator<'static> {
    Operator {
        operator: RangeCtlOp::CtlOp {
            ctrl: ".regex",
            span: (0, 0, 0),
        },
        type2: Type2::TextValue {
            value: regex_str,
            span: (0, 0, 0),
        },
        comments_before_operator: None,
        comments_after_operator: None,
    }
}

// Helper function for creating CDDL entries, an ident string, `Type2` object and optionally an
// `Occur` object are passed in.
pub fn create_entry(
    ident: &'static str,
    value: Type2<'static>,
    occur: Option<Occur>,
    operator: Option<Operator<'static>>,
) -> (GroupEntry<'static>, OptionalComma<'static>) {
    let occurrence = match occur {
        Some(o) => Some(Occurrence {
            occur: o,
            comments: None,
        }),
        None => None,
    };
    (
        GroupEntry::ValueMemberKey {
            ge: Box::from(ValueMemberKeyEntry {
                occur: occurrence,
                member_key: Some(MemberKey::Bareword {
                    ident: ident.into(),
                    comments: None,
                    comments_after_colon: None,
                    span: (0, 0, 0),
                }),
                entry_type: Type {
                    type_choices: vec![TypeChoice {
                        type1: Type1 {
                            type2: value,
                            operator: operator,
                            comments_after_type: None,
                            span: (0, 0, 0),
                        },
                        comments_before_type: None,
                        comments_after_type: None,
                    }],
                    span: (0, 0, 0),
                },
            }),
            leading_comments: None,
            trailing_comments: None,
            span: (0, 0, 0),
        },
        OptionalComma {
            optional_comma: true,
            trailing_comments: None,
        },
    )
}

pub fn create_text_value(text_value: &str) -> Type2 {
    Type2::TextValue {
        value: text_value,
        span: (0, 0, 0),
    }
}

pub fn create_typename(type_name: &str) -> Type2 {
    Type2::Typename {
        ident: Identifier {
            ident: type_name,
            socket: None,
            span: (0, 0, 0),
        },
        generic_args: None,
        span: (0, 0, 0),
    }
}

// Helper function for creating a CDDL entry in the correct form for p2panda message fields.
// These are a map containing 2 fields; `type` and `value`.
pub fn create_message_field(
    field_type: FieldTypes,
) -> Vec<(GroupEntry<'static>, OptionalComma<'static>)> {
    // Match passed type and map it to our MessageFields type and CDDL types (do we still need the
    // MessageFields type key when we are using schemas?)
    let (type_value, type_name, operator) = match field_type {
        FieldTypes::Str => ("str", "tstr", None),
        FieldTypes::Int => ("int", "int", None),
        FieldTypes::Float => ("float", "float", None),
        FieldTypes::Bool => ("bool", "bool", None),
        FieldTypes::Relation => (
            "relation",
            "hash",
            Some(create_regex_operator("[0-9a-fa-f]{132}")),
        ),
    };
    // Create an array of message_fields
    let mut message_fields = Vec::new();
    message_fields.push(create_entry(
        "type",
        create_text_value(type_value),
        None,
        None,
    ));
    message_fields.push(create_entry(
        "value",
        create_typename(type_name),
        None,
        operator,
    ));
    message_fields
}

#[derive(Debug)]
pub struct UserSchema {
    entries: Vec<(GroupEntry<'static>, OptionalComma<'static>)>,
    schema: Option<String>,
}

impl UserSchema {
    pub fn new() -> Self {
        UserSchema {
            entries: Vec::new(),
            schema: None,
        }
    }
    pub fn new_from_string(schema: &String) -> Result<Self, SchemaError> {
        let mut l = Lexer::new(schema);
        // Need to fix proper error checking, not uwrap()
        let mut p = Parser::new(l.iter(), schema).unwrap();
        Ok(Self {
            entries: Vec::new(),
            schema: Some(p.parse_cddl().unwrap().to_string()),
        })
    }
    // Add a message field to the schema passing in field name and type
    pub fn add_message_field(&mut self, name: &'static str, field_type: FieldTypes) {
        // Create an array of message fields
        let message_fields = create_message_field(field_type);

        // Add a named message fields entry (of type map) to the schema
        self.entries
            .push(create_entry(name, create_map(message_fields), None, None));
    }
    // Add an optional message field to the schema passing in field name and type
    pub fn add_optional_message_field(&mut self, name: &'static str, field_type: FieldTypes) {
        // Create an array of message fields
        let message_fields = create_message_field(field_type);

        // Add a named message fields entry (of type map) to the schema
        self.entries.push(create_entry(
            name,
            create_map(message_fields),
            Some(Occur::Optional((0, 0, 0))),
            None,
        ));
    }
    // Add message field with custom occurence param to the schema passing in field name, type and [`Occur`]
    pub fn add_custom_message_field(
        &mut self,
        name: &'static str,
        field_type: FieldTypes,
        occur: Occur,
        operator: Option<Operator<'static>>,
    ) {
        // Create an array of message fields
        let message_fields = create_message_field(field_type);

        // Add a named message fields entry (of type map) to the schema
        self.entries.push(create_entry(
            name,
            create_map(message_fields),
            Some(occur),
            operator,
        ));
    }
    // Returns schema string if schema exists
    pub fn get_schema(&self) -> Option<String> {
        match &self.schema {
            Some(schema) => Some(schema.to_owned()),
            None if self.entries.len() == 0 => None, // schema must contain some entries
            None => Some(create_cddl(self.entries.clone()).to_string()),
        }
    }
    pub fn create<T>(values: Vec<(String, T)>) -> Result<MessageFields, MessageFieldsError> {
        // Create a new MessageFields instance based on passed array of (key, value) tuples
        // Validate against UserSchema CDDL schema
        // I got stuck here trying to use generic parameters and type checking / where clauses / traits
        // Feels like there is a nice way to do this but didn't find it yet....
        Ok(MessageFields::new())
    }

    /// Validate a message against this user schema
    #[cfg(not(target_arch = "wasm32"))]
    pub fn validate_message(&self, bytes: Vec<u8>) -> Result<(), cddl::validator::cbor::Error> {
        validate_cbor_from_slice(&self.get_schema().unwrap(), &bytes)
    }
}

trait SchemaMessage<T = String> {
    // ...
}

#[cfg(test)]
mod tests {
    use super::{FieldTypes, UserSchema};
    use crate::message::{MessageFields, MessageValue};
    use cddl::ast::Occur;

    #[test]
    pub fn add_message_fields() {
        let mut schema = UserSchema::new();
        schema.add_message_field("first-name", FieldTypes::Str);
        schema.add_message_field("last-name", FieldTypes::Str);
        schema.add_optional_message_field("age", FieldTypes::Int);
        let cddl_str = "my_rule = { first-name: { type: \"str\", value: tstr, }, last-name: { type: \"str\", value: tstr, }, ? age: { type: \"int\", value: int, }, }\n";
        assert_eq!(cddl_str, schema.get_schema().unwrap())
    }

    #[test]
    pub fn add_custom_message_fields() {
        let mut schema = UserSchema::new();
        schema.add_custom_message_field(
            "first-name",
            FieldTypes::Str,
            Occur::OneOrMore((0, 0, 0)),
            None,
        );
        schema.add_custom_message_field(
            "last-name",
            FieldTypes::Str,
            Occur::Exact {
                lower: Some(1),
                upper: Some(3),
                span: (0, 0, 0),
            },
            None,
        );
        schema.add_custom_message_field("age", FieldTypes::Int, Occur::ZeroOrMore((0, 0, 0)), None);
        let cddl_str = "my_rule = { + first-name: { type: \"str\", value: tstr, }, 1*3 last-name: { type: \"str\", value: tstr, }, * age: { type: \"int\", value: int, }, }\n";
        assert_eq!(cddl_str, schema.get_schema().unwrap())
    }
    #[test]
    pub fn add_message_fields_with_relation() {
        let mut schema = UserSchema::new();
        schema.add_message_field("first-name", FieldTypes::Str);
        schema.add_message_field("last-name", FieldTypes::Str);
        schema.add_message_field("member-of", FieldTypes::Relation);
        let cddl_str = "my_rule = { first-name: { type: \"str\", value: tstr, }, last-name: { type: \"str\", value: tstr, }, member-of: { type: \"relation\", value: hash .regex \"[0-9a-fa-f]{132}\", }, }\n";
        assert_eq!(cddl_str, schema.get_schema().unwrap())
    }

    #[test]
    pub fn new_from_string() {
        // Create new empty schema
        let mut schema_1 = UserSchema::new();
        //Add message fields
        schema_1.add_message_field("first-name", FieldTypes::Str);
        schema_1.add_message_field("last-name", FieldTypes::Str);
        schema_1.add_optional_message_field("age", FieldTypes::Int);
        // Matching CDDL schema string
        let cddl_str = "my_rule = { 
            first-name: { type: \"str\", value: tstr, }, 
            last-name: { type: \"str\", value: tstr, }, 
            ? age: { type: \"int\", value: int, }, 
        }\n";
        // Create new schema from CDDL string
        let schema_2 = UserSchema::new_from_string(&cddl_str.to_string()).unwrap();
        // should be equal
        assert_eq!(schema_2.get_schema(), schema_1.get_schema());

        // Empty schema should return None
        let empty_schema = UserSchema::new();
        assert_eq!(empty_schema.get_schema(), None)
    }

    #[test]
    pub fn validate_message_fields() {
        let mut person_schema = UserSchema::new();
        person_schema.add_message_field("first-name", FieldTypes::Str);
        person_schema.add_message_field("last-name", FieldTypes::Str);
        person_schema.add_optional_message_field("age", FieldTypes::Int);

        // Build "person" message fields
        let mut person = MessageFields::new();
        person
            .add("first-name", MessageValue::Text("Park".to_owned()))
            .unwrap();
        person
            .add("last-name", MessageValue::Text("Saeroyi".to_owned()))
            .unwrap();
        person.add("age", MessageValue::Integer(32)).unwrap();

        // Encode message fields
        let me_encoded = serde_cbor::to_vec(&person).unwrap();

        // Validate message fields against person schema
        assert!(person_schema.validate_message(me_encoded).is_ok());

        person
            .add("favorite-number", MessageValue::Integer(3))
            .unwrap();

        let me_encoded_again = serde_cbor::to_vec(&person).unwrap();

        // Should throw error because of extra field
        assert!(person_schema.validate_message(me_encoded_again).is_err());
    }
}
