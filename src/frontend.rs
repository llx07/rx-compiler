use std::sync::{Arc, Mutex};

use antlr4_runtime::{
    CommonTokenStream, ErrorListener, InputStream, Recognizer, SyntaxErrorEvent, Token,
};

use crate::diagnostic::{CompileResult, Diagnostic};
use crate::generated::rx_lexer::{self, RxLexer};
use crate::generated::rx_parser::{self, RxParser};

pub fn lexer_parse(source: &str) -> CompileResult<CommonTokenStream<RxLexer<InputStream>>> {
    let lexer = RxLexer::new(InputStream::new(source));
    let tokens: CommonTokenStream<RxLexer<InputStream>> = CommonTokenStream::new(lexer);
    for token in tokens.tokens() {
        match token.token_type() {
            rx_lexer::INVALID_CHARACTER_LITERAL => {
                return Err(Diagnostic::new(
                    "Lexer error: character literal is not supported in Rx",
                    token.line(),
                    token.column(),
                ));
            }
            rx_lexer::INVALID_LIFETIME => {
                return Err(Diagnostic::new(
                    format!(
                        "Lexer error: {} is not a valid lifetime",
                        token.text().unwrap()
                    ),
                    token.line(),
                    token.column(),
                ));
            }
            rx_lexer::INVALID_NUMBER => {
                return Err(Diagnostic::new(
                    "Lexer error: invalid number literal",
                    token.line(),
                    token.column(),
                ));
            }
            rx_lexer::ERROR_CHAR => {
                return Err(Diagnostic::new(
                    "Lexer error: unkown character",
                    token.line(),
                    token.column(),
                ));
            }
            rx_lexer::UNTERMINATED_BLOCK_COMMENT => {
                return Err(Diagnostic::new(
                    "Lexer error: unterminated block comment",
                    token.line(),
                    token.column(),
                ));
            }
            _ => {}
        }
    }

    return Ok(tokens);
}

struct DiagnosticListener {
    diagnostic: Arc<Mutex<Option<Diagnostic>>>,
}

impl<R> ErrorListener<R> for DiagnosticListener
where
    R: Recognizer + ?Sized,
{
    fn syntax_error(&mut self, _recognizer: &R, event: &SyntaxErrorEvent<'_>) {
        if self.diagnostic.lock().unwrap().is_some() {
            return;
        }
        *(self.diagnostic.lock().unwrap()) =
            Some(Diagnostic::new(event.message, event.line, event.column));
    }
}

pub fn parser_parse(tokens: CommonTokenStream<RxLexer<InputStream>>) -> CompileResult<()> {
    let mut parser = RxParser::new(tokens);
    parser.remove_error_listeners();

    let diagnostic: Arc<Mutex<Option<Diagnostic>>> = Arc::new(Mutex::new(None));
    parser.add_error_listener(DiagnosticListener {
        diagnostic: Arc::clone(&diagnostic),
    });

    let result = parser.crate_();

    let diagnostic = diagnostic.lock().unwrap().take();
    if let Some(diagnostic) = diagnostic {
        return Err(diagnostic);
    }

    let root = match result {
        Ok(root) => root,
        Err(_) => {
            return Err(Diagnostic::new("unkown error at parser", 1, 0));
        }
    };

    return Ok(());
}
