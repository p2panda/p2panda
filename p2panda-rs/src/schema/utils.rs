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

pub const USER_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
