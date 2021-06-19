use cddl::ast::{
    Group, GroupChoice, GroupEntry, Identifier, MemberKey, OptionalComma, Rule, Type, Type1, Type2,
    TypeChoice, TypeRule, ValueMemberKeyEntry, CDDL,
};

pub enum FieldTypes {
    Str,
    Int,
    Float,
    Bool,
    Relation,
}

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

pub fn create_entry(
    ident: &'static str,
    value: Type2<'static>,
) -> (GroupEntry<'static>, OptionalComma<'static>) {
    (
        GroupEntry::ValueMemberKey {
            ge: Box::from(ValueMemberKeyEntry {
                occur: None,
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
                            operator: None,
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

pub fn create_message_field(field_type: FieldTypes) -> (Type2<'static>, Type2<'static>) {
    // Match passed type and map it to our MessageFields type and CDDL types (do we still need the 
    // MessageFields type key when we are using schemas?)
    let (value, value_type) = match field_type {
        FieldTypes::Str => ("str", "tstr"),
        FieldTypes::Int => ("int", "int"),
        FieldTypes::Float => ("float", "float"),
        FieldTypes::Bool => ("bool", "bool"),
        FieldTypes::Relation => ("relation", "hash"),
    };
    // Return a tuple of message field values
    (
        Type2::TextValue {
            value: value,
            span: (0, 0, 0),
        },
        Type2::Typename {
            ident: Identifier {
                ident: value_type,
                socket: None,
                span: (0, 0, 0),
            },
            generic_args: None,
            span: (0, 0, 0),
        },
    )
}

#[derive(Debug)]
pub struct UserSchema {
    entries: Vec<(GroupEntry<'static>, OptionalComma<'static>)>,
}

impl UserSchema {
    pub fn new() -> Self {
        UserSchema {
            entries: Vec::new(),
        }
    }
    // Add a message field to the schema passing in field name and type
    pub fn add_message_field(&mut self, name: &'static str, field_type: FieldTypes) {
        // Create a message field of passed type
        let (value_1, value_2) = create_message_field(field_type);
        
        // Create an array of message_fields
        let mut message_fields = Vec::new();
        message_fields.push(create_entry("type", value_1));
        message_fields.push(create_entry("value", value_2));
        
        // Add a named message fields entry (of type map) to the schema
        self.entries
            .push(create_entry(name, create_map(message_fields)));
    }
    pub fn get_cddl(&self) -> CDDL {
        create_cddl(self.entries.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldTypes, UserSchema};

    #[test]
    pub fn add_message_fields() {
        let mut schema = UserSchema::new();
        schema.add_message_field("first-name", FieldTypes::Str);
        schema.add_message_field("last-name", FieldTypes::Str);
        schema.add_message_field("age", FieldTypes::Int);
        let cddl_str = "my_rule = { first-name: { type: \"str\", value: tstr, }, last-name: { type: \"str\", value: tstr, }, age: { type: \"int\", value: int, }, }\n";
        assert_eq!(cddl_str, schema.get_cddl().to_string())
    }
}
