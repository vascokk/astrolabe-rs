; Function definitions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

; Class definitions
(class_specifier
  name: (type_identifier) @name) @definition.class

; Struct definitions
(struct_specifier
  name: (type_identifier) @name) @definition.struct

; Enum definitions
(enum_specifier
  name: (type_identifier) @name) @definition.enum
