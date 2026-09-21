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
        const RED: &str = "\x1b[31m";
        const BLUE: &str = "\x1b[34m";
        const BOLD: &str = "\x1b[1m";
        const RESET: &str = "\x1b[0m";

        let source_line = source.lines().nth(self.line - 1).unwrap_or("");

        eprintln!("{BOLD}{RED}error{RESET}: {}", self.message);
        let width = self.line.to_string().len();

        eprintln!(
            "{:>width$}{BLUE}--> {RESET}{}:{}:{}",
            "",
            filename,
            self.line,
            self.column + 1
        );
        eprintln!("{BLUE}{:>width$} |{RESET}", "", width = width);
        eprintln!(
            "{BLUE}{:>width$} |{RESET} {}",
            self.line,
            source_line,
            width = width
        );
        eprintln!(
            "{BLUE}{:>width$} |{RESET} {}{RED}^{RESET}",
            "",
            " ".repeat(self.column),
            width = width
        );
    }
}

pub type CompileResult<T> = Result<T, Diagnostic>;
