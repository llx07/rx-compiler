pub type Span = std::ops::Range<usize>;

#[derive(Debug)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn show(&self, filename: &str, source: &str) {
        const RED: &str = "\x1b[31m";
        const BLUE: &str = "\x1b[34m";
        const BOLD: &str = "\x1b[1m";
        const RESET: &str = "\x1b[0m";

        let start = self.span.start.min(source.len());
        let end = self.span.end.min(source.len()).max(start);

        let line_start = source[..start].rfind('\n').map_or(0, |pos| pos + 1);
        let line_end = source[start..]
            .find('\n')
            .map_or(source.len(), |pos| start + pos);

        let line_end = if line_end < source.len()
            && line_end > line_start
            && source.as_bytes()[line_end - 1] == b'\r'
        {
            line_end - 1
        } else {
            line_end
        };

        let source_line = &source[line_start..line_end];

        let line = source[..line_start].bytes().filter(|&b| b == b'\n').count() + 1;
        let column = source[line_start..start].chars().count();

        let span_end_on_line = end.min(line_end);
        let marker_len = source[start..span_end_on_line].chars().count().max(1);

        let width = line.to_string().len();

        eprintln!("{BOLD}{RED}error{RESET}: {}", self.message);

        eprintln!(
            "{:>width$}{BLUE}--> {RESET}{}:{}:{}",
            "",
            filename,
            line,
            column + 1
        );
        eprintln!("{BLUE}{:>width$} |{RESET}", "", width = width);
        eprintln!(
            "{BLUE}{:>width$} |{RESET} {}",
            line,
            source_line,
            width = width
        );
        eprintln!(
            "{BLUE}{:>width$} |{RESET} {}{RED}{}{RESET}",
            "",
            " ".repeat(column),
            "^".repeat(marker_len),
            width = width,
        );
    }
}

pub type CompileResult<T> = Result<T, Diagnostic>;
