# Parts Editor Support, Syntax Highlighting & LSP

This directory provides syntax highlighting, editor configuration, and Language Server Protocol (LSP) setup for the **Parts** (`.pts`) programming language.

---

## 1. Parts Language Server (`parts --lsp`)

The `parts` compiler includes a built-in Language Server Protocol (LSP) server providing:
- **Real-time Diagnostics**: Syntax, scanner, and compiler error detection on-the-fly.
- **Hover Documentation**: Signatures, docstrings, and type information for keywords, primitives (`Int`, `Double`, `Bool`, `String`, `Bytes`), built-ins (`println`, `exec`, `unwrap`, etc.), and user-defined functions/enums.
- **Auto-Completion**: Keywords, snippets (`fn`, `match`, `enum`, `for in`), built-in types, standard library modules (`@std/std.pts`, `@std/net.pts`, etc.), and local symbols.
- **Document Symbols / Outline**: Hierarchy of functions, enums, variants, variables, and macros.
- **Go to Definition**: Jump directly to function, enum, variable, and macro definitions.
- **Document Formatting**: Clean code formatting and trailing whitespace normalization.

To launch the server:
```bash
parts --lsp
```

---

## 2. Editor Setup Guides

### A. Neovim (Built-in LSP / `nvim-lspconfig`)

Add to your `~/.config/nvim/init.lua`:

```lua
-- Filetype detection and syntax
vim.filetype.add({
  extension = {
    pts = 'parts',
  },
})

-- Start Parts LSP automatically for .pts files
vim.api.nvim_create_autocmd('FileType', {
  pattern = 'parts',
  callback = function(args)
    vim.lsp.start({
      name = 'parts-lsp',
      cmd = { 'parts', '--lsp' },
      root_dir = vim.fs.root(args.buf, { 'Cargo.toml', '.git' }),
    })
  end,
})
```

Also install the syntax file:
```bash
mkdir -p ~/.config/nvim/syntax
cp editors/vim/syntax/parts.vim ~/.config/nvim/syntax/
```

---

### B. Helix Editor

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "parts"
scope = "source.pts"
file-types = ["pts"]
comment-token = "//"
roots = ["Cargo.toml", ".git"]
indent = { tab-width = 4, unit = "    " }
language-servers = ["parts-lsp"]

[language-server.parts-lsp]
command = "parts"
args = ["--lsp"]
```

---

### C. VS Code / VSCodium / Cursor

1. Copy the extension to your extensions directory:
   ```bash
   mkdir -p ~/.vscode/extensions/parts-language
   cp -r editors/vscode/* ~/.vscode/extensions/parts-language/
   ```
2. Reload VS Code (`Developer: Reload Window`).

---

### D. Sublime Text (LSP Package)

1. Copy `Parts.sublime-syntax` to Packages/User:
   ```bash
   cp editors/sublime/Parts.sublime-syntax ~/.config/sublime-text/Packages/User/
   ```
2. Open `Preferences > Package Settings > LSP > Settings` and add:
   ```json
   {
     "clients": {
       "parts-lsp": {
         "enabled": true,
         "command": ["parts", "--lsp"],
         "selector": "source.pts"
       }
     }
   }
   ```

---

### E. Emacs (`eglot`)

Add to your `~/.emacs.d/init.el`:

```elisp
(add-to-list 'auto-mode-alist '("\\.pts\\'" . prog-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs '(prog-mode . ("parts" "--lsp"))))
```

---

### F. GNU Nano

Append to `~/.nanorc`:
```bash
cat editors/nano/parts.nanorc >> ~/.nanorc
```
