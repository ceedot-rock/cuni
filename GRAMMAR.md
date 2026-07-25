# CuNi Grammar (EBNF)

A formal grammar for CuNi v0.1, derived directly from the implementation
(`src/lexer.rs`, `src/parser.rs`) rather than written ahead of it — every rule
below corresponds to an actual lexer/parser function, cited in a comment.
SPEC.md is the prose/rationale reference; this file is the syntax reference.

Notation: `::=` defines a rule, `|` alternation, `[ ]` optional, `{ }` zero or
more repetitions, `"..."` a literal token, `<...>` a lexical/prose primitive
defined in §1.

## 1. Lexical Grammar

```ebnf
program        ::= { trivia } item { trivia }               (* lexer::run *)

trivia         ::= whitespace | comment
comment        ::= "#" { <any char except newline> }        (* skip_trivia *)

ident          ::= ident_start { ident_cont }                (* lex_ident *)
ident_start    ::= "a".."z" | "A".."Z" | "_"
ident_cont     ::= ident_start | "0".."9"

int_lit        ::= digit { digit }                            (* lex_number *)
float_lit      ::= digit { digit } "." digit { digit }
digit          ::= "0".."9"

plain_string   ::= '"' { string_char | escape } '"'           (* lex_plain_string *)
interp_string  ::= "`" { string_char | escape | interp_hole } "`"  (* lex_interp_string *)
interp_hole    ::= "${" expr "}"
string_char    ::= <any char except the string's own delimiter, "\", or (in interp_string) "$" followed by "{">
escape         ::= "\" ( "n" | "t" | '"' | "`" | "\" | "$" )  (* read_escape *)

bool_lit       ::= "true" | "false"
none_lit       ::= "none"
```

Reserved words (returned by `token::keyword`, never lexed as `ident`):

```
do  end  let  mut  def  link  ret  fail  if  els  for  in  whl
typ  iface  is  enum  use  ext  and  or  not  true  false  none
```

`ext` blocks are the one lexical exception: once the lexer sees `ext ... do`,
every line up to the matching `end` is read as `target_name ":" <raw text to
end of line>` (an `ExtTarget` token) rather than tokenized as CuNi at all —
see `lexer::scan_ext_body_token`. `ext_target` in §2 reflects this.

## 2. Items

```ebnf
item        ::= use_decl | ext_decl | typ_decl | iface_decl | enum_decl
              | fn_decl | link_decl | stmt                    (* parser::parse_item *)

use_decl    ::= "use" ident                                   (* parse_use *)

ext_decl    ::= "ext" ident "(" params ")" "->" type
                "do" { ext_target } "end"                     (* parse_ext *)
ext_target  ::= ident ":" <raw source text to end of line>

typ_decl    ::= "typ" ident [ "is" ident ]
                "do" { field } "end"                           (* parse_typ *)
field       ::= ident ":" type
(* Construction is not a separate production: `TypName(args…)` is an ordinary
   `call` expression whose callee is the type name. Positional args match
   field declaration order; typeck enforces arity; backends rewrite as needed
   (Go composite literal, JS `new`). See SPEC.md §10. *)

iface_decl  ::= "iface" ident "do" { method_sig } "end"        (* parse_iface *)
method_sig  ::= ident "(" params ")" "->" type

enum_decl   ::= "enum" ident "do" { ident } "end"              (* parse_enum *)

fn_decl     ::= "def" ident [ generics ] "(" params ")" "->" type
                [ "?" ] "do" { stmt } "end"                    (* parse_fn(is_link=false) *)
generics    ::= "<" ident { "," ident } ">"

(* SPEC.md §16. Identical shape to fn_decl but no generics — `parse_fn`
   handles both `def` and `link` with an `is_link` flag, since a wire
   contract needs a concrete, enumerable type shape a type parameter
   doesn't give it. Params/return additionally restricted to scalar types
   (int/float/str/bool) by a separate check, not the grammar itself — see
   checks.rs::find_bad_link_type. *)
link_decl   ::= "link" ident "(" params ")" "->" type
                [ "?" ] "do" { stmt } "end"                    (* parse_fn(is_link=true) *)

params      ::= [ param { "," param } ]                        (* parse_params *)
param       ::= ident ":" type

type        ::= ident [ "<" type { "," type } ">" ]            (* parse_type *)
```

## 3. Statements

```ebnf
stmt        ::= let_stmt | mut_stmt | assign_stmt | ret_stmt | fail_stmt
              | if_stmt | for_stmt | whl_stmt | todo_stmt | expr_stmt
                                                                (* parse_stmt *)

let_stmt    ::= "let" ident [ ":" type ] "=" expr
mut_stmt    ::= "mut" ident [ ":" type ] "=" expr
assign_stmt ::= expr "=" expr
ret_stmt    ::= "ret" [ expr ]
fail_stmt   ::= "fail" expr
if_stmt     ::= "if" expr "do" { stmt } [ "els" { stmt } ] "end"  (* parse_if *)
for_stmt    ::= "for" ident [ "," ident ] "in" expr "do" { stmt } "end"
whl_stmt    ::= "whl" expr "do" { stmt } "end"
todo_stmt   ::= "..."
expr_stmt   ::= expr
```

`assign_stmt` and `expr_stmt` share a parse path: any statement not starting
with a statement keyword parses an expression first, then checks for a
following `=` (`parse_stmt`'s fallback arm).

## 4. Expressions

Precedence climbs low-to-high through this chain (each rule calls the next):

```ebnf
expr        ::= or_expr                                        (* parse_expr *)
or_expr     ::= and_expr { "or" and_expr }
and_expr    ::= equality_expr { "and" equality_expr }
equality_expr
            ::= comparison_expr { ( "==" | "!=" ) comparison_expr }
comparison_expr
            ::= additive_expr { ( "<" | ">" | "<=" | ">=" ) additive_expr }
additive_expr
            ::= multiplicative_expr { ( "+" | "-" ) multiplicative_expr }
multiplicative_expr
            ::= unary_expr { ( "*" | "/" | "%" ) unary_expr }
unary_expr  ::= ( "not" | "-" ) unary_expr | postfix_expr
postfix_expr
            ::= primary { call_suffix | index_suffix | field_suffix | unwrap_suffix }
call_suffix ::= "(" [ expr { "," expr } ] ")"
index_suffix
            ::= "[" expr "]"
field_suffix
            ::= "." ident
unwrap_suffix
            ::= "??" "do" { stmt } "end"                       (* the `??` operator, §12/§13 *)

primary     ::= int_lit | float_lit | bool_lit | none_lit
              | plain_string | interp_string
              | ident
              | "(" expr ")"
              | "[" [ expr { "," expr } ] "]"                   (* list literal *)
              | "{" [ expr ":" expr { "," expr ":" expr } ] "}"  (* map literal *)
```

`field_suffix` also covers `EnumName.Variant` access (§14) and stdlib method
calls (`xs.push(v)`, `xs.len()`, §15) — these aren't separate grammar
productions, they're ordinary `field_suffix`/`call_suffix` combinations that
codegen recognizes by name.

## 5. What this grammar does not cover

This is the *syntactic* grammar the current lexer/parser accept — it says
nothing about which programs are valid CuNi (e.g. it happily parses `fail`
outside a fallible function, or `.push` on a `let`-bound list; see SPEC.md
§18 on the still-undesigned type/effect checker). It also doesn't specify a
tokenization priority table (e.g. why `??` lexes before `?`) since the lexer
is hand-written and unambiguous by construction rather than driven by a
formal precedence declaration — `src/lexer.rs` is the source of truth for
exact lexical disambiguation.
