use crate::*;
use crate::common::shared::Shared;

use super::*;
use super::space::grounding::GroundingSpace;
use super::text::{Tokenizer, SExprParser};
use super::types::validate_atom;
use super::interpreter::interpret;

use regex::Regex;
use std::path::PathBuf;
use std::collections::HashMap;

mod stdlib;
use stdlib::*;

const EXEC_SYMBOL : Atom = sym!("!");

pub struct Metta {
    space: Shared<GroundingSpace>,
    tokenizer: Shared<Tokenizer>,
    settings: Shared<HashMap<String, String>>,
}

enum Mode {
    ADD,
    INTERPRET,
}

impl Metta {
    pub fn new(space: Shared<GroundingSpace>) -> Self {
        Metta::from_space_cwd(space, PathBuf::from("."))
    }

    pub fn from_space_cwd(space: Shared<GroundingSpace>, cwd: PathBuf) -> Self {
        let settings = Shared::new(HashMap::new());
        let tokenizer = Shared::new(Tokenizer::new());
        {
            fn regex(regex: &str) -> Regex {
                Regex::new(regex).unwrap()
            }

            let mut tref = tokenizer.borrow_mut();
            let match_op = Atom::gnd(MatchOp{});
            tref.register_token(regex(r"match"), move |_| { match_op.clone() });
            let space_val = Atom::value(space.clone());
            tref.register_token(regex(r"&self"), move |_| { space_val.clone() });
            let import_op = Atom::gnd(ImportOp::new(cwd.clone(), space.clone(), tokenizer.clone()));
            tref.register_token(regex(r"import!"), move |_| { import_op.clone() });
            let bind_op = Atom::gnd(BindOp::new(tokenizer.clone()));
            tref.register_token(regex(r"bind!"), move |_| { bind_op.clone() });
            let new_space_op = Atom::gnd(NewSpaceOp{});
            tref.register_token(regex(r"new-space"), move |_| { new_space_op.clone() });
            let case_op = Atom::gnd(CaseOp::new(space.clone()));
            tref.register_token(regex(r"case"), move |_| { case_op.clone() });
            let assert_equal_op = Atom::gnd(AssertEqualOp::new(space.clone()));
            tref.register_token(regex(r"assertEqual"), move |_| { assert_equal_op.clone() });
            let assert_equal_to_result_op = Atom::gnd(AssertEqualToResultOp::new(space.clone()));
            tref.register_token(regex(r"assertEqualToResult"), move |_| { assert_equal_to_result_op.clone() });
            let collapse_op = Atom::gnd(CollapseOp::new(space.clone()));
            tref.register_token(regex(r"collapse"), move |_| { collapse_op.clone() });
            let pragma_op = Atom::gnd(PragmaOp::new(settings.clone()));
            tref.register_token(regex(r"pragma!"), move |_| { pragma_op.clone() });
        }
        Self{ space, tokenizer, settings }
    }

    pub fn space(&self) -> Shared<GroundingSpace> {
        self.space.clone()
    }

    pub fn tokenizer(&self) -> Shared<Tokenizer> {
        self.tokenizer.clone()
    }

    fn get_setting(&self, key: &str) -> Option<String> {
        self.settings.borrow().get(key.into()).cloned()
    }

    pub fn run(&self, parser: &mut SExprParser) -> Result<Vec<Vec<Atom>>, String> {
        let mut mode = Mode::ADD;
        let mut results: Vec<Vec<Atom>> = Vec::new();

        loop {
            let atom = parser.parse(&self.tokenizer.borrow());
            match atom {
                Some(atom) => {
                    if atom == EXEC_SYMBOL {
                        mode = Mode::INTERPRET;
                        continue;
                    }
                    match self.interp_atom(mode, atom) {
                        Err(msg) => return Err(msg),
                        Ok(Some(result)) => results.push(result),
                        _ => {},
                    }
                    mode = Mode::ADD;
                },
                None => break,
            }
        }
        Ok(results)
    }

    fn interp_atom(&self, mode: Mode, atom: Atom) -> Result<Option<Vec<Atom>>, String> {
        // FIXME: how to make it look better?
        if self.get_setting("type-check").as_ref().map(String::as_str) == Some("auto") {
            if !validate_atom(&self.space.borrow(), &atom) {
                return Ok(Some(vec![Atom::expr([ERROR_SYMBOL, atom, BAD_TYPE_SYMBOL])]))
            }
        }
        match mode {
            Mode::ADD => {
                log::trace!("Metta::run: adding atom: {} into space: {:?}", atom, self.space);
                self.space.borrow_mut().add(atom);
                Ok(None) 
            },
            Mode::INTERPRET => {
                log::trace!("Metta::run: interpreting atom: {}", atom);
                let result = interpret(self.space.clone(), &atom);
                log::trace!("Metta::run: interpretation result {:?}", result);
                match result {
                    Ok(result) => Ok(Some(result)),
                    Err(message) => Err(format!("Error: {}", message)),
                }
            },
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space() {
        let program = "
            (= (And T T) T)
            (= (frog $x)
                (And (croaks $x)
                     (eat_flies $x)))
            (= (croaks Fritz) T)
            (= (eat_flies Fritz) T)
            (= (green $x) (frog $x))
            !(green Fritz)
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![Atom::sym("T")]]));
    }

    #[test]
    fn test_match() {
        let program = "
            (A B)
            !(match &self (A B) (B A))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![expr!("B" "A")]]));
    }

    #[test]
    fn new_space() {
        let program = "
            (A B)
            !(match (new-space) (A B) (B A))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![]]));
    }

    #[test]
    fn bind_new_space() {
        let program = "
            (A B)
            !(bind! &my (new-space))
            !(match &my (A B) (B A))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![], vec![]]));
    }

    #[test]
    fn case() {
        let program = "
            (= (foo) (A B))
            !(case (foo) (
                (($n B) $n)
                (%void% D)
            ))
            !(case (match &self (B C) (B C)) (
                (($n C) $n)
                (%void% D)
            ))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![Atom::sym("A")], vec![Atom::sym("D")]]));
    }

    fn assert_error(atom: Atom, message: &str) -> Atom {
        Atom::expr([ERROR_SYMBOL, atom, Atom::sym(message)])
    }

    #[test]
    fn assert_equal() {
        let program = "
            (= (foo) (A B))
            (= (foo) (B C))
            (= (bar) (B C))
            (= (bar) (A B))
            (= (err) (A B))
            !(assertEqual (foo) (bar))
            !(assertEqual (foo) (err))
            !(assertEqual (err) (foo))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![
            vec![],
            vec![assert_error(expr!("A" "B"), "\nExpected: [(A B)]\nGot: [(A B), (B C)]\nExcessive result: (B C)"),
                 assert_error(expr!("B" "C"), "\nExpected: [(A B)]\nGot: [(A B), (B C)]\nExcessive result: (B C)")],
            vec![assert_error(expr!("A" "B"), "\nExpected: [(A B), (B C)]\nGot: [(A B)]\nMissed result: (B C)")]
        ]));
    }

    #[test]
    fn assert_equal_to_result() {
        let program = "
            (= (foo) (A B))
            (= (foo) (B C))
            !(assertEqualToResult (foo) ((B C) (A B)))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![]]));
    }

    #[test]
    fn collapse() {
        let program = "
            (= (foo) (A B))
            (= (foo) (B C))
            !(collapse (foo))
        ";

        let metta = Metta::new(Shared::new(GroundingSpace::new()));
        let result = metta.run(&mut SExprParser::new(program));
        assert_eq!(result, Ok(vec![vec![expr!(("A" "B") ("B" "C"))]]));
    }
}
