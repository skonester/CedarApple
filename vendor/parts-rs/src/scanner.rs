use crate::scanner_rules::ScannerRule;

#[derive(Debug)]
pub struct Scanner {
    rules: Vec<ScannerRule>,
    buffer: Vec<Token>,
    index: usize,
    source: String,

    //Meta
    line: usize,
    column: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub enum ScannerError {
    InvalidOperator(String),
    UnterminatedString,
    InvalidEscape,
    UnknownToken(char),
}

impl std::fmt::Display for ScannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScannerError::InvalidOperator(op) => write!(f, "invalid operator '{}'", op),
            ScannerError::UnterminatedString => write!(f, "unterminated string literal"),
            ScannerError::InvalidEscape => write!(f, "invalid escape sequence in string literal"),
            ScannerError::UnknownToken(ch) => write!(f, "unexpected character '{}'", ch),
        }
    }
}

impl std::error::Error for ScannerError {}

impl ScannerError {
    pub fn to_diagnostic(
        &self,
        source: Option<&str>,
        file: Option<&str>,
    ) -> crate::diagnostic::Diagnostic {
        let msg = self.to_string();
        let mut diag = crate::diagnostic::Diagnostic::error(msg.clone());
        if let Some(src) = source {
            diag = diag.with_source(src);
        }
        if let Some(f) = file {
            diag = diag.with_file(f);
        }
        diag
    }
}

impl Scanner {
    pub fn new(rules: Vec<ScannerRule>, source: String) -> Self {
        Scanner {
            rules,
            buffer: vec![],
            index: 0,
            source,
            line: 1,
            column: 0,
        }
    }

    fn create_eof(&self) -> Token {
        Token {
            kind: TokenType::Special,
            lexeme: "EOF".to_string(),
            span: Span {
                line: self.line,
                column: self.column,
            },
        }
    }

    pub fn get_next(&mut self) -> Result<Token, ScannerError> {
        if let Some(elt) = self.buffer.pop() {
            return Ok(elt);
        }

        let ch = self.peek();

        if ch.is_none() {
            return Ok(self.create_eof());
        }

        for rule in &self.rules {
            if rule.base_rule.is_none() {
                if rule.valid_chars.contains(&self.peek().unwrap()) {
                    let res = Self::parse_rule(
                        rule,
                        &mut self.index,
                        &self.source,
                        &mut self.line,
                        &mut self.column,
                    )?;

                    if rule.skip || res.iter().len() == 0 {
                        return self.get_next();
                    }

                    if res.len() == 1 {
                        return Ok(res[0].clone());
                    }

                    let return_token = res[0].clone();

                    self.buffer = res.clone().into_iter().skip(1).rev().collect();
                    return Ok(return_token);
                } else {
                    continue;
                }
            }

            if rule.base_rule.as_ref().unwrap()(&self.peek().unwrap()) {
                let res = Self::parse_rule(
                    rule,
                    &mut self.index,
                    &self.source,
                    &mut self.line,
                    &mut self.column,
                )?;

                if rule.skip || res.iter().len() == 0 {
                    return self.get_next();
                }

                if res.len() == 1 {
                    return Ok(res[0].clone());
                }

                let return_token = res[0].clone();

                self.buffer = res.clone().into_iter().skip(1).rev().collect();
                return Ok(return_token);
            }
        }

        if self.peek().is_none() {
            return Ok(self.create_eof());
        }

        Err(ScannerError::UnknownToken(self.peek().unwrap()))
    }

    pub fn peek(&self) -> Option<char> {
        if self.index >= self.source.len() {
            None
        } else {
            self.source[self.index..].chars().next()
        }
    }

    pub fn parse_rule(
        rule: &ScannerRule,
        index: &mut usize,
        source: &str,
        line: &mut usize,
        column: &mut usize,
    ) -> Result<Vec<Token>, ScannerError> {
        let start = *index;

        let start_span = Span {
            line: *line,
            column: *column,
        };

        loop {
            if *index >= source.len() {
                break;
            }

            let ch = source[*index..].chars().next().unwrap();
            let ch_len = ch.len_utf8();
            *index += ch_len;

            if ch == '\n' {
                *line += 1;
                *column = 0;
            } else {
                *column += 1;
            }

            let next_ch = if *index < source.len() {
                source[*index..].chars().next()
            } else {
                None
            };

            let out_of_bounds = *index >= source.len();
            let no_base_but_valid =
                rule.base_rule.is_none() && rule.valid_chars.contains(&next_ch.unwrap_or('\x00'));
            let matches_base = rule.base_rule.is_some()
                && rule.base_rule.as_ref().unwrap()(&next_ch.unwrap_or('\x00'));
            let matches_whole =
                rule.rule.is_none() || rule.rule.as_ref().unwrap()(&source[start..*index]);

            if out_of_bounds || !(no_base_but_valid || matches_base) || !matches_whole {
                break;
            }
        }

        if let Some(v) = &rule.process {
            return v(&rule.mappings, &source[start..*index], start_span);
        }

        Ok(vec![Token {
            kind: rule.result.clone(),
            lexeme: source[start..*index].to_string(),
            span: start_span,
        }])
    }

    pub fn append_stream(&mut self, mut tokens: Vec<Token>) {
        tokens.reverse();
        self.buffer.splice(0..0, tokens);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenType,
    pub lexeme: String,
    pub span: Span,
}

impl Token {
    pub fn friendly_operator_name(op: &str) -> &'static str {
        match op {
            "PLUS" => "'+'",
            "MINUS" => "'-'",
            "SLASH" => "'/'",
            "STAR" => "'*'",
            "MOD" => "'%'",
            "SEMICOLON" => "';'",
            "COLON" => "':'",
            "DOUBLE_COLON" => "'::'",
            "DOT" => "'.'",
            "COMMA" => "','",
            "LEFT_PAREN" => "'('",
            "RIGHT_PAREN" => "')'",
            "LEFT_BRACE" => "'{'",
            "RIGHT_BRACE" => "'}'",
            "LEFT_BRACKET" => "'['",
            "RIGHT_BRACKET" => "']'",
            "AT" => "'@'",
            "BANG" => "'!'",
            "EQUALS" => "'='",
            "BIT_AND" => "'&'",
            "BIT_OR" => "'|'",
            "BIT_XOR" => "'^'",
            "OBJ_START" => "'|>'",
            "OBJ_END" => "'<|'",
            "EQUALITY" => "'=='",
            "LESS_THAN" => "'<'",
            "MORE_THAN" => "'>'",
            "LESS_EQ" => "'<='",
            "MORE_EQ" => "'>='",
            "ARROW_LEFT" => "'=>'",
            "LEFT_SHIFT" => "'<<'",
            "RIGHT_SHIFT" => "'>>'",
            _ => "operator",
        }
    }

    pub fn user_friendly_name(&self) -> String {
        match self.kind {
            TokenType::Operator => {
                if self.lexeme.is_empty() {
                    "operator".to_string()
                } else {
                    Self::friendly_operator_name(&self.lexeme).to_string()
                }
            }
            TokenType::Keyword => {
                if self.lexeme.is_empty() {
                    "keyword".to_string()
                } else {
                    format!("keyword '{}'", self.lexeme)
                }
            }
            TokenType::Identifier => {
                if self.lexeme.is_empty() {
                    "identifier".to_string()
                } else {
                    format!("identifier '{}'", self.lexeme)
                }
            }
            TokenType::Number => {
                if self.lexeme.is_empty() {
                    "number".to_string()
                } else {
                    format!("number '{}'", self.lexeme)
                }
            }
            TokenType::String => {
                if self.lexeme.is_empty() {
                    "string literal".to_string()
                } else {
                    format!("string \"{}\"", self.lexeme)
                }
            }
            TokenType::Special => {
                if self.lexeme == "EOF" {
                    "end of file".to_string()
                } else if self.lexeme.is_empty() {
                    "special token".to_string()
                } else {
                    format!("'{}'", self.lexeme)
                }
            }
            TokenType::Space => "whitespace".to_string(),
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_friendly_name())
    }
}

impl std::fmt::Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Operator => write!(f, "operator"),
            TokenType::Number => write!(f, "number"),
            TokenType::Keyword => write!(f, "keyword"),
            TokenType::Identifier => write!(f, "identifier"),
            TokenType::String => write!(f, "string"),
            TokenType::Space => write!(f, "whitespace"),
            TokenType::Special => write!(f, "special"),
        }
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.lexeme == other.lexeme
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    Operator,
    Number,
    Keyword,
    Identifier,
    String,
    Space,
    Special,
}
