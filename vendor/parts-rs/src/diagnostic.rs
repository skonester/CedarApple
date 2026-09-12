use std::fmt::Display;

/// Severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
    Info,
}

impl DiagnosticLevel {
    pub fn name(&self) -> &'static str {
        match self {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Note => "note",
            DiagnosticLevel::Help => "help",
            DiagnosticLevel::Info => "info",
        }
    }

    pub fn ansi_color(&self) -> &'static str {
        match self {
            DiagnosticLevel::Error => "\x1b[1;31m",   // Bold Red
            DiagnosticLevel::Warning => "\x1b[1;33m", // Bold Yellow
            DiagnosticLevel::Note => "\x1b[1;36m",    // Bold Cyan
            DiagnosticLevel::Help => "\x1b[1;32m",    // Bold Green
            DiagnosticLevel::Info => "\x1b[1;34m",    // Bold Blue
        }
    }
}

impl Display for DiagnosticLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A structured diagnostic representing a compiler/parser/runtime message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub length: usize,
    pub label: Option<String>,
    pub notes: Vec<String>,
    pub help: Option<String>,
    pub source_code: Option<String>,
}

impl Diagnostic {
    pub fn new(level: DiagnosticLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            file: None,
            line: None,
            column: None,
            length: 1,
            label: None,
            notes: Vec::new(),
            help: None,
            source_code: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Warning, message)
    }

    pub fn note(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Note, message)
    }

    pub fn help(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Help, message)
    }

    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source_code = Some(source.into());
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_length(mut self, length: usize) -> Self {
        self.length = if length == 0 { 1 } else { length };
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Render this diagnostic as a plain string without ANSI color codes.
    pub fn render_plain(
        &self,
        fallback_source: Option<&str>,
        fallback_file: Option<&str>,
    ) -> String {
        self.render_internal(fallback_source, fallback_file, false)
    }

    /// Render this diagnostic with ANSI color codes.
    pub fn render_colored(
        &self,
        fallback_source: Option<&str>,
        fallback_file: Option<&str>,
    ) -> String {
        self.render_internal(fallback_source, fallback_file, true)
    }

    /// Render automatically detecting color preference (checks NO_COLOR environment variable).
    pub fn render(&self, fallback_source: Option<&str>, fallback_file: Option<&str>) -> String {
        let use_color = std::env::var_os("NO_COLOR").is_none();
        self.render_internal(fallback_source, fallback_file, use_color)
    }

    fn render_internal(
        &self,
        fallback_source: Option<&str>,
        fallback_file: Option<&str>,
        use_color: bool,
    ) -> String {
        let mut out = String::new();

        let bold = if use_color { "\x1b[1m" } else { "" };
        let reset = if use_color { "\x1b[0m" } else { "" };
        let color = if use_color {
            self.level.ansi_color()
        } else {
            ""
        };
        let blue = if use_color { "\x1b[1;34m" } else { "" };

        // 1. Header: error: message
        out.push_str(&format!(
            "{}{}{}{}: {}{}{}\n",
            bold,
            color,
            self.level.name(),
            reset,
            bold,
            self.message,
            reset
        ));

        let file_name = self.file.as_deref().or(fallback_file);
        let source = self.source_code.as_deref().or(fallback_source);

        // 2. Location pointer: --> file:line:col
        if let (Some(file), Some(line), Some(col)) = (file_name, self.line, self.column) {
            if line > 0 {
                out.push_str(&format!(
                    "  {}-->{} {}:{}:{}\n",
                    blue,
                    reset,
                    file,
                    line,
                    if col == 0 { 1 } else { col }
                ));
            } else {
                out.push_str(&format!("  {}-->{} {}\n", blue, reset, file));
            }
        } else if let Some(file) = file_name {
            out.push_str(&format!("  {}-->{} {}\n", blue, reset, file));
        } else if let (Some(line), Some(col)) = (self.line, self.column)
            && line > 0
        {
            out.push_str(&format!(
                "  {}-->{} line {}, col {}\n",
                blue,
                reset,
                line,
                if col == 0 { 1 } else { col }
            ));
        }

        // 3. Source code snippet with caret
        if let (Some(src), Some(target_line)) = (source, self.line)
            && target_line > 0
        {
            let lines: Vec<&str> = src.lines().collect();
            if target_line <= lines.len() {
                let line_str = lines[target_line - 1];
                let line_num_str = format!("{}", target_line);
                let gutter_width = line_num_str.len().max(2);

                // Empty gutter line: "   |"
                out.push_str(&format!(
                    "{:width$} {}|{}\n",
                    "",
                    blue,
                    reset,
                    width = gutter_width
                ));

                // Source line: " 2 | let x = 5"
                out.push_str(&format!(
                    "{}{:>width$}{} {}|{} {}\n",
                    bold,
                    line_num_str,
                    reset,
                    blue,
                    reset,
                    line_str,
                    width = gutter_width
                ));

                // Caret line: "   |     ^^^^ unexpected token"
                let col = self.column.unwrap_or(1);
                let start_col = if col == 0 { 1 } else { col };
                let indent = start_col.saturating_sub(1);
                let carets_count = self.length.max(1);
                let carets = "^".repeat(carets_count);

                let label_str = if let Some(l) = &self.label {
                    format!(" {}", l)
                } else {
                    String::new()
                };

                out.push_str(&format!(
                    "{:width$} {}|{} {:indent$}{}{}{}{}{}\n",
                    "",
                    blue,
                    reset,
                    "",
                    bold,
                    color,
                    carets,
                    label_str,
                    reset,
                    width = gutter_width,
                    indent = indent
                ));
            }
        }

        // 4. Notes
        for note in &self.notes {
            out.push_str(&format!(
                "   {}={} {}{}:{} {}\n",
                blue,
                reset,
                bold,
                DiagnosticLevel::Note.name(),
                reset,
                note
            ));
        }

        // 5. Help
        if let Some(help_msg) = &self.help {
            let green = if use_color { "\x1b[1;32m" } else { "" };
            out.push_str(&format!(
                "   {}={} {}{}:{} {}\n",
                blue,
                reset,
                green,
                DiagnosticLevel::Help.name(),
                reset,
                help_msg
            ));
        }

        out
    }

    pub fn print(&self, source: Option<&str>, file: Option<&str>) {
        print!("{}", self.render(source, file));
    }

    pub fn eprint(&self, source: Option<&str>, file: Option<&str>) {
        eprint!("{}", self.render(source, file));
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render_plain(None, None))
    }
}

impl std::error::Error for Diagnostic {}

/// A collection of diagnostics produced during scanning, parsing, or compilation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn render(&self, source: Option<&str>, file: Option<&str>) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render(source, file))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn render_plain(&self, source: Option<&str>, file: Option<&str>) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render_plain(source, file))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn render_colored(&self, source: Option<&str>, file: Option<&str>) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render_colored(source, file))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn print(&self, source: Option<&str>, file: Option<&str>) {
        print!("{}", self.render(source, file));
    }

    pub fn eprint(&self, source: Option<&str>, file: Option<&str>) {
        eprint!("{}", self.render(source, file));
    }
}
