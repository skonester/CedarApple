" Vim syntax file
" Language: Parts (.pts)
" Maintainer: Parts Language Team

if exists("b:current_syntax")
  finish
endif

" Keywords
syn keyword partsKeyword let fun enum import syntax translation part
syn keyword partsConditional if else match
syn keyword partsRepeat for in
syn keyword partsControl return break continue raise

" Booleans & Constants
syn keyword partsBoolean true false
syn keyword partsType Int Double Bool String Bytes Result Option
syn keyword partsConstant Ok Err Some None

" Macro parameter variables
syn match partsMacroVar "@[a-zA-Z_][a-zA-Z0-9_]*"
syn match partsMacroDef "\<part\s\+[a-zA-Z_][a-zA-Z0-9_]*" contains=partsKeyword
syn match partsMacroCall "\<[a-zA-Z_][a-zA-Z0-9_]*!\ze[{(]"

" Numbers
syn match partsNumber "\v<\d+>"
syn match partsFloat "\v<\d+\.\d+([eE][+-]?\d+)?>"
syn match partsHex "\v<0[xX][0-9a-fA-F]+>"
syn match partsBin "\v<0[bB][01]+>"

" Strings
syn region partsStringDouble start='"' end='"' contains=partsEscape
syn region partsStringBacktick start='`' end='`' contains=partsEscape
syn match partsEscape contained "\\\([nrt0bf\"\\]\|x[0-9a-fA-F]\{2}\)"
syn match partsStdImport "@std/[a-zA-Z0-9_./-]\+"

" Comments
syn match partsCommentLine "//.*$"
syn region partsCommentBlock start="/\*" end="\*/"
syn match partsShebang "^#!.*$"

" Operators
syn match partsOperator "\v(\+|\-|\*|\/|\%|\=|\=\=|\<|\>|\<\=|\>\=|\&|\||\^|\!|\:\:|\=\>|\<\<|\>\>)"
syn match partsObjectDelimiter "\v(\|\>|\<\|)"

" Function declarations and calls
syn match partsFunction "\v<let\s+([a-zA-Z_][a-zA-Z0-9_]*)\ze\s*\(" contains=partsKeyword
syn match partsFunctionCall "\v<[a-zA-Z_][a-zA-Z0-9_]*\ze\s*\("

" Built-in functions
syn keyword partsBuiltin println print input exec env len join to_uppercase to_lowercase to_hex push is_ok is_err is_some is_none unwrap unwrap_or unwrap_err expect

" Highlight links
hi def link partsKeyword Keyword
hi def link partsConditional Conditional
hi def link partsRepeat Repeat
hi def link partsControl Statement
hi def link partsBoolean Boolean
hi def link partsType Type
hi def link partsConstant Constant
hi def link partsMacroVar Identifier
hi def link partsMacroDef Function
hi def link partsMacroCall Macro
hi def link partsNumber Number
hi def link partsFloat Float
hi def link partsHex Number
hi def link partsBin Number
hi def link partsStringDouble String
hi def link partsStringBacktick String
hi def link partsEscape Special
hi def link partsStdImport Include
hi def link partsCommentLine Comment
hi def link partsCommentBlock Comment
hi def link partsShebang Comment
hi def link partsOperator Operator
hi def link partsObjectDelimiter Delimiter
hi def link partsFunction Function
hi def link partsFunctionCall Function
hi def link partsBuiltin Special

let b:current_syntax = "parts"
