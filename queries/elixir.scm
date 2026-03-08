; Module definitions: defmodule MyApp.Foo do ... end
(call
  target: (identifier) @_target
  (arguments
    (alias) @name)
  (#eq? @_target "defmodule")) @definition.module

; Public function definitions: def foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "def")) @definition.function

; Private function definitions: defp foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "defp")) @definition.function

; Public macro definitions: defmacro foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "defmacro")) @definition.function

; Private macro definitions: defmacrop foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "defmacrop")) @definition.function

; Struct definitions: defstruct [:field1, :field2]
(call
  target: (identifier) @_target
  (#eq? @_target "defstruct")) @definition.struct

; Protocol definitions: defprotocol Foo do ... end
(call
  target: (identifier) @_target
  (arguments
    (alias) @name)
  (#eq? @_target "defprotocol")) @definition.interface

; Protocol implementations: defimpl Proto, for: Type do ... end
(call
  target: (identifier) @_target
  (arguments
    (alias) @name)
  (#eq? @_target "defimpl")) @definition.impl
