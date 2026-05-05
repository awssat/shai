;; =================================================================
;; queries/rust.scm  —  shai Rust semantic captures
;; =================================================================


;; =================================================================
;; 1. IMPL METHODS (Composite Identity)
;; Captures: `impl PlayerPlugin { fn update(...) { ... } }`
;; Identity: @impl_target + @method_name → "PlayerPlugin::update"
;; =================================================================
(impl_item
  type: (type_identifier) @impl_target
  body: (declaration_list
    (function_item
      name: (identifier) @method_name
      parameters: (parameters) @method_params
      body: (block) @method_body
    ) @impl_method
  )
) @impl_block


;; =================================================================
;; 2. FREE FUNCTIONS
;; Captures: `fn player_movement(mut query: Query<...>) { ... }`
;; =================================================================
(function_item
  name: (identifier) @func_name
  parameters: (parameters) @func_params
  body: (block) @func_body
) @free_function


;; =================================================================
;; 3. STRUCTS
;; Captures: `struct Health { max: f32 }`
;; =================================================================
(struct_item
  name: (type_identifier) @struct_name
  body: (field_declaration_list)? @struct_fields
) @struct_def


;; =================================================================
;; 4. ENUMS
;; Captures: `enum AppState { Loading, InGame, Paused }`
;; =================================================================
(enum_item
  name: (type_identifier) @enum_name
  body: (enum_variant_list) @enum_variants
) @enum_def


;; =================================================================
;; 5. TYPE ALIASES
;; Captures: `type Health = f32;`
;; Useful for tracking public type surface changes
;; =================================================================
(type_item
  name: (type_identifier) @type_alias_name
  type: (_) @type_alias_value
) @type_alias
