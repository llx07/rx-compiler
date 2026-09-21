#[derive(Debug)]
pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }

    pub fn show(&self, filename: &str, source: &str) {
        let source_line = source.lines().nth(self.line - 1).unwrap_or("");

        eprintln!("error: {}", self.message);
        let width = self.line.to_string().len();

        eprintln!(
            "{:>width$}--> {}:{}:{}",
            "", filename, self.line, self.column
        );
        eprintln!("{:>width$} |", "", width = width);
        eprintln!("{:>width$} | {}", self.line, source_line, width = width);
        eprintln!(
            "{:>width$} | {}^",
            "",
            " ".repeat(self.column.saturating_sub(1)),
            width = width
        );
    }
}

pub type CompileResult<T> = Result<T, Diagnostic>;
