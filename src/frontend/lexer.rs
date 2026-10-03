use antlr4_runtime::{CommonTokenStream, InputStream, Token};

use crate::diagnostic::{CompileResult, Diagnostic};
use crate::generated::rx_lexer::{self, RxLexer};

pub fn parse(source: &str) -> CompileResult<CommonTokenStream<RxLexer<InputStream>>> {
    let lexer = RxLexer::new(InputStream::new(source));
    let tokens: CommonTokenStream<RxLexer<InputStream>> = CommonTokenStream::new(lexer);
    for token in tokens.tokens() {
        match token.token_type() {
            rx_lexer::INVALID_CHARACTER_LITERAL => {
                return Err(Diagnostic::new(
                    "Lexer error: character literal is not supported in Rx",
                    token.byte_span().unwrap(),
                ));
            }
            rx_lexer::INVALID_LIFETIME => {
                return Err(Diagnostic::new(
                    format!(
                        "Lexer error: {} is not a valid lifetime",
                        token.text().unwrap()
                    ),
                    token.byte_span().unwrap(),
                ));
            }
            rx_lexer::INVALID_NUMBER => {
                return Err(Diagnostic::new(
                    "Lexer error: invalid number literal",
                    token.byte_span().unwrap(),
                ));
            }
            rx_lexer::ERROR_CHAR => {
                return Err(Diagnostic::new(
                    "Lexer error: unkown character",
                    token.byte_span().unwrap(),
                ));
            }
            rx_lexer::UNTERMINATED_BLOCK_COMMENT => {
                return Err(Diagnostic::new(
                    "Lexer error: unterminated block comment",
                    token.byte_span().unwrap(),
                ));
            }
            _ => {}
        }
    }

    return Ok(tokens);
}
