use azul_simplecss::{Token, Tokenizer};

#[test]
#[ignore] // Debug-only test that deliberately panics to print tokens
fn debug_lang_pseudo_class() {
    let css = r#"div:lang(de) { color: red; }"#;
    let mut tokenizer = Tokenizer::new(css);

    let mut tokens = Vec::new();
    loop {
        match tokenizer.parse_next() {
            Ok(token) => {
                let is_end = matches!(token, Token::EndOfStream);
                tokens.push(format!("{:?}", token));
                if is_end {
                    break;
                }
            }
            Err(e) => {
                tokens.push(format!("Error: {:?}", e));
                break;
            }
        }
    }

    for t in &tokens {
        eprintln!("{}", t);
    }

    panic!("Tokens: {:?}", tokens);
}
