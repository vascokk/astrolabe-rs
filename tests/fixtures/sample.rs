mod my_mod {
    pub struct MyStruct {
        pub value: u32,
    }

    impl MyStruct {
        pub fn my_func(&self) -> u32 {
            self.value
        }
    }
}

pub fn top_level_fn() -> &'static str {
    "hello"
}

pub enum MyEnum {
    VariantA,
    VariantB,
}
