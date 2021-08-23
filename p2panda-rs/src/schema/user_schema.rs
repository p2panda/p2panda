/// All user schema
pub const USER_SCHEMA: &str = r#"
    userSchema = {
        address // 
        person
    }
    
    address = (
        city: { type: "str", value: tstr },
        street: { type: "str", value: tstr },
        house-number: { type: "int", value: int }, 
    )
    
    person = (
        name: { type: "str", value: tstr },
        age: { type: "int", value: int }, 
    )
"#;
