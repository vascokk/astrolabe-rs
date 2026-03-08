; Function declarations
(function_declaration
  name: (identifier) @name) @definition.function

; Method declarations
(method_declaration
  name: (field_identifier) @name) @definition.method

; Type declarations (structs, interfaces)
(type_declaration
  (type_spec
    name: (type_identifier) @name)) @definition.struct
