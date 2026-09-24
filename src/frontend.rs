use antlr4_runtime::{CommonTokenStream, InputStream, Token};

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
