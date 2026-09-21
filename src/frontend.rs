use antlr4_runtime::{CommonTokenStream, InputStream, Token};

use crate::diagnostic::{CompileResult, Diagnostic};
use crate::generated::rx_lexer::{self, RxLexer};
use crate::generated::rx_parser::RxParser;

// TODO: we have not implemented AST for now
// frontend does not return any value and only check
// for grammars.
pub fn parse(source: &str) -> CompileResult<()> {
    let lexer = RxLexer::new(InputStream::new(source));
    let tokens: CommonTokenStream<RxLexer<InputStream>> = CommonTokenStream::new(lexer);
    // let vocabulary = rx_lexer::metadata().vocabulary();
    for token in tokens.tokens() {
        // println!(
        //     "type={} channel={} text={:?}",
        //     vocabulary.display_name(token.token_type()),
        //     token.channel(),
        //     token.text(),
        // );

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

    return Ok(());
}
