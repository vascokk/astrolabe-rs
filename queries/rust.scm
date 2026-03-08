; Function definitions
(function_item
  name: (identifier) @name) @definition.function

; Struct definitions
(struct_item
  name: (type_identifier) @name) @definition.struct

; Enum definitions
(enum_item
  name: (type_identifier) @name) @definition.enum

; Trait definitions
(trait_item
  name: (type_identifier) @name) @definition.trait

; Impl blocks
(impl_item
  type: (type_identifier) @name) @definition.impl

; Module definitions
(mod_item
  name: (identifier) @name) @definition.module
