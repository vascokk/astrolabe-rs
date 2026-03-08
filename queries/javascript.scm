; Function declarations
(function_declaration
  name: (identifier) @name) @definition.function

; Class declarations
(class_declaration
  name: (identifier) @name) @definition.class

; Method definitions
(method_definition
  name: (property_identifier) @name) @definition.method
