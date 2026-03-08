; Function definitions
(function_definition
  name: (identifier) @name) @definition.function

; Class definitions
(class_definition
  name: (identifier) @name) @definition.class

; Decorated function definitions
(decorated_definition
  definition: (function_definition
    name: (identifier) @name)) @definition.function
