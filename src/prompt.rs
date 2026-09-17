use std::collections::VecDeque;
use std::io::{self, Write};

use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
    Quit,
}

/// 交互抽象：终端实现读标准输入；测试用脚本实现
pub trait Prompt {
    fn confirm(&mut self, question: &str, default_yes: bool) -> Result<Answer>;
    fn number(&mut self, question: &str, default: usize) -> Result<usize>;
}

pub struct TerminalPrompt;

/// EOF 返回 None
fn read_line(prompt: &str) -> Result<Option<String>> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut s = String::new();
    if io::stdin().read_line(&mut s)? == 0 {
        println!();
        return Ok(None);
    }
    Ok(Some(s.trim().to_string()))
}

impl Prompt for TerminalPrompt {
    fn confirm(&mut self, question: &str, default_yes: bool) -> Result<Answer> {
        let hint = if default_yes { "[Y/n/q]" } else { "[y/N/q]" };
        let default = if default_yes { Answer::Yes } else { Answer::No };
        loop {
            let Some(line) = read_line(&format!("{question} {hint}: "))? else {
                return Ok(default);
            };
            match line.to_ascii_lowercase().as_str() {
                "" => return Ok(default),
                "y" | "yes" => return Ok(Answer::Yes),
                "n" | "no" => return Ok(Answer::No),
                "q" | "quit" => return Ok(Answer::Quit),
                _ => println!("  请输入 y / n / q"),
            }
        }
    }

    fn number(&mut self, question: &str, default: usize) -> Result<usize> {
        loop {
            let Some(line) = read_line(&format!("{question} [默认 {default}]: "))? else {
                return Ok(default);
            };
            if line.is_empty() {
                return Ok(default);
            }
            match line.parse::<usize>() {
                Ok(n) if n > 0 => return Ok(n),
                _ => println!("  请输入正整数"),
            }
        }
    }
}

/// 预先写好答案的 prompt；答案用尽后返回默认值
#[derive(Default)]
pub struct ScriptedPrompt {
    pub answers: VecDeque<Answer>,
    pub numbers: VecDeque<usize>,
}

impl ScriptedPrompt {
    pub fn new(answers: Vec<Answer>, numbers: Vec<usize>) -> Self {
        ScriptedPrompt {
            answers: answers.into(),
            numbers: numbers.into(),
        }
    }
}

impl Prompt for ScriptedPrompt {
    fn confirm(&mut self, _q: &str, default_yes: bool) -> Result<Answer> {
        Ok(self
            .answers
            .pop_front()
            .unwrap_or(if default_yes { Answer::Yes } else { Answer::No }))
    }
    fn number(&mut self, _q: &str, default: usize) -> Result<usize> {
        Ok(self.numbers.pop_front().unwrap_or(default))
    }
}
