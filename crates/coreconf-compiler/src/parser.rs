use crate::ast::{AstModule, AstStatement};
use crate::lexer::{lex, Token};

pub fn parse_module(input: &str) -> Result<AstModule, String> {
    let tokens = lex(input);
    let mut cursor = 0;

    match tokens.get(cursor) {
        Some(Token::Identifier(keyword)) if keyword == "module" => {}
        _ => return Err("expected module".into()),
    }
    cursor += 1;

    let name = match tokens.get(cursor) {
        Some(Token::Identifier(name)) => name.clone(),
        _ => return Err("expected module name".into()),
    };
    cursor += 1;

    let children = parse_block(&tokens, &mut cursor)?;
    Ok(AstModule { name, children })
}

fn parse_block(tokens: &[Token], cursor: &mut usize) -> Result<Vec<AstStatement>, String> {
    match tokens.get(*cursor) {
        Some(Token::LBrace) => *cursor += 1,
        _ => return Err("expected '{'".into()),
    }

    let mut statements = Vec::new();
    while let Some(token) = tokens.get(*cursor) {
        match token {
            Token::RBrace => {
                *cursor += 1;
                break;
            }
            Token::Identifier(keyword) => {
                let keyword = keyword.clone();
                *cursor += 1;
                let argument = match tokens.get(*cursor) {
                    Some(Token::Identifier(value)) => {
                        *cursor += 1;
                        Some(value.clone())
                    }
                    Some(Token::String(value)) => {
                        *cursor += 1;
                        Some(value.clone())
                    }
                    _ => None,
                };

                let children = match tokens.get(*cursor) {
                    Some(Token::LBrace) => parse_block(tokens, cursor)?,
                    Some(Token::Semicolon) => {
                        *cursor += 1;
                        vec![]
                    }
                    _ => return Err(format!("expected ';' or '{{' after {}", keyword)),
                };

                statements.push(AstStatement {
                    keyword,
                    argument,
                    children,
                });
            }
            _ => return Err("unexpected token in block".into()),
        }
    }

    Ok(statements)
}
