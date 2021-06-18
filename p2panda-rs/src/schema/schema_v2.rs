use cddl::ast::{
    Group, GroupChoice, GroupEntry, Identifier, MemberKey, OptionalComma, Rule, Type,
    Type1, Type2, TypeChoice, TypeRule, ValueMemberKeyEntry, CDDL,
};

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
    pub fn add_message_field(
        &mut self,
        name: &'static str,
        value_type: &'static str,
    ) {
        let str_value = Type2::TextValue {
            value: value_type,
            span: (0, 0, 0),
        };
        let tstr_value = Type2::Typename {
            ident: Identifier {
                // Need to do some matching so that this 
                // is dynamic with the passed value_type
                // right now it is always "tstr".
                ident: "tstr".into(),
                socket: None,
                span: (0, 0, 0),
            },
            generic_args: None,
            span: (0, 0, 0),
        };
        
        // Create an array of message_fields
        let mut message_fields = Vec::new();
        message_fields.push(create_entry("type", str_value));
        message_fields.push(create_entry("value", tstr_value));
        // Add a named message fields entry (of type map) to the schema
        self.entries.push(create_entry(name, create_map(message_fields)));
    }
    pub fn get_cddl(&self) -> CDDL {
        create_cddl(self.entries.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::UserSchema;

    #[test]
    pub fn add_group() {
        let mut schema = UserSchema::new();
        schema.add_message_field("first-name", "str");
        schema.add_message_field("last-name", "str");
        let cddl_str = "my_rule = { first-name: { type: \"str\", value: tstr, }, last-name: { type: \"str\", value: tstr, }, }\n";
        assert_eq!(cddl_str, schema.get_cddl().to_string())
    }
}
