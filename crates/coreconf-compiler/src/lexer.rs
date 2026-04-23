#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Identifier(String),
    String(String),
    LBrace,
    RBrace,
    Semicolon,
}

pub fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' => tokens.push(Token::LBrace),
            '}' => tokens.push(Token::RBrace),
            ';' => tokens.push(Token::Semicolon),
            '"' => {
                let mut value = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '"' {
                        break;
                    }
                    value.push(next);
                }
                tokens.push(Token::String(value));
            }
            c if c.is_whitespace() => {}
            c => {
                let mut ident = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || matches!(next, '{' | '}' | ';' | '"') {
                        break;
                    }
                    ident.push(next);
                    chars.next();
                }
                tokens.push(Token::Identifier(ident));
            }
        }
    }

    tokens
}
