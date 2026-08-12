//! Pure spreadsheet formula calculation as a derived projection.
//!
//! This module reads a validated [`SpreadsheetModel`] and never writes a calculated value back
//! to a canonical cell.  The first evaluator deliberately has a small, explicit grammar: numeric
//! constants, cell references, parentheses and `+ - * /`.  Functions, ranges, external workbook
//! references, cycles and unknown references are rejected instead of being guessed.

use std::collections::{BTreeMap, HashMap};

use oo_schema::{ArtifactEnvelope, ArtifactPayload, CellModel, SpreadsheetModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CellAddress;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculationResult {
    /// Derived values keyed by canonical cell address. The source model is not mutated.
    pub values: BTreeMap<CellAddress, CalculatedValue>,
}

impl CalculationResult {
    pub fn value(&self, address: &CellAddress) -> Option<&CalculatedValue> {
        self.values.get(address)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum CalculatedValue {
    Blank,
    Number(f64),
    Text(String),
    Bool(bool),
    /// Spreadsheet errors are values in the read projection. A strict caller can continue using
    /// [`calculate`] while a grid renderer can use [`calculate_with_errors`] to display all errors
    /// in one pass without aborting the viewport.
    Error {
        code: FormulaErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormulaErrorCode {
    Cycle,
    UnknownReference,
    UnknownSheet,
    Syntax,
    UnsupportedFunction,
    UnsupportedRange,
    UnsupportedExternalWorkbook,
    TypeMismatch,
    DivisionByZero,
    NonFinite,
    UnsupportedValue,
    EmptyFormula,
}

/// Evaluate every materialized cell in a validated snapshot into a detached projection.
pub fn calculate(model: &SpreadsheetModel) -> Result<CalculationResult, FormulaCalculationError> {
    ArtifactEnvelope::new(
        "spreadsheet-formula-calculation",
        ArtifactPayload::Spreadsheet(model.clone()),
    )
    .validate()
    .map_err(FormulaCalculationError::Schema)?;

    let mut calculator = Calculator::new(model)?;
    let addresses = calculator.cells.keys().cloned().collect::<Vec<_>>();
    for address in addresses {
        calculator.evaluate_cell(&address)?;
    }
    Ok(CalculationResult {
        values: calculator.values,
    })
}

/// Calculates every materialized cell while representing per-cell failures as typed error
/// values. This is the spreadsheet-friendly projection: a bad formula does not prevent unrelated
/// cells from rendering, and cyclic references are visible instead of silently becoming blank.
pub fn calculate_with_errors(
    model: &SpreadsheetModel,
) -> Result<CalculationResult, FormulaCalculationError> {
    ArtifactEnvelope::new(
        "spreadsheet-formula-calculation",
        ArtifactPayload::Spreadsheet(model.clone()),
    )
    .validate()
    .map_err(FormulaCalculationError::Schema)?;
    let mut calculator = Calculator::new(model)?;
    let addresses = calculator.cells.keys().cloned().collect::<Vec<_>>();
    for address in addresses {
        if let Err(error) = calculator.evaluate_cell(&address) {
            calculator
                .values
                .insert(address, CalculatedValue::from_error(&error));
        }
    }
    Ok(CalculationResult {
        values: calculator.values,
    })
}

impl CalculatedValue {
    fn from_error(error: &FormulaCalculationError) -> Self {
        let code = match error {
            FormulaCalculationError::Cycle { .. } => FormulaErrorCode::Cycle,
            FormulaCalculationError::UnknownReference { .. } => FormulaErrorCode::UnknownReference,
            FormulaCalculationError::UnknownSheet { .. } => FormulaErrorCode::UnknownSheet,
            FormulaCalculationError::Syntax { .. } => FormulaErrorCode::Syntax,
            FormulaCalculationError::UnsupportedFunction { .. } => {
                FormulaErrorCode::UnsupportedFunction
            }
            FormulaCalculationError::UnsupportedRange { .. } => FormulaErrorCode::UnsupportedRange,
            FormulaCalculationError::UnsupportedExternalWorkbook { .. } => {
                FormulaErrorCode::UnsupportedExternalWorkbook
            }
            FormulaCalculationError::TypeMismatch { .. } => FormulaErrorCode::TypeMismatch,
            FormulaCalculationError::DivisionByZero { .. } => FormulaErrorCode::DivisionByZero,
            FormulaCalculationError::NonFinite { .. } => FormulaErrorCode::NonFinite,
            FormulaCalculationError::UnsupportedValue { .. } => FormulaErrorCode::UnsupportedValue,
            FormulaCalculationError::EmptyFormula { .. } => FormulaErrorCode::EmptyFormula,
            FormulaCalculationError::Schema(_) => FormulaErrorCode::UnsupportedValue,
        };
        Self::Error {
            code,
            message: error.to_string(),
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum FormulaCalculationError {
    #[error("Spreadsheet schema 校验失败：{0}")]
    Schema(#[source] oo_schema::SchemaValidationError),
    #[error("公式单元格 {cell:?} 为空")]
    EmptyFormula { cell: CellAddress },
    #[error("公式 {cell:?} 语法无效：{message}")]
    Syntax { cell: CellAddress, message: String },
    #[error("公式 {cell:?} 使用了不支持的函数：{function}")]
    UnsupportedFunction { cell: CellAddress, function: String },
    #[error("公式 {cell:?} 使用了不支持的范围")]
    UnsupportedRange { cell: CellAddress },
    #[error("公式 {cell:?} 使用了外部工作簿引用")]
    UnsupportedExternalWorkbook { cell: CellAddress },
    #[error("公式 {cell:?} 引用了未知工作表：{sheet}")]
    UnknownSheet { cell: CellAddress, sheet: String },
    #[error("公式 {cell:?} 引用了未知单元格：{reference:?}")]
    UnknownReference {
        cell: CellAddress,
        reference: CellAddress,
    },
    #[error("公式循环引用：{cells:?}")]
    Cycle { cells: Vec<CellAddress> },
    #[error("公式 {cell:?} 的值类型不支持 {operation}")]
    TypeMismatch {
        cell: CellAddress,
        operation: String,
    },
    #[error("公式 {cell:?} 除数不能为零")]
    DivisionByZero { cell: CellAddress },
    #[error("公式 {cell:?} 产生了非有限数字")]
    NonFinite { cell: CellAddress },
    #[error("单元格 {cell:?} 的常量值类型不支持")]
    UnsupportedValue { cell: CellAddress },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

struct Calculator<'a> {
    cells: BTreeMap<CellAddress, &'a CellModel>,
    sheets: HashMap<String, String>,
    states: HashMap<CellAddress, VisitState>,
    stack: Vec<CellAddress>,
    values: BTreeMap<CellAddress, CalculatedValue>,
}

impl<'a> Calculator<'a> {
    fn new(model: &'a SpreadsheetModel) -> Result<Self, FormulaCalculationError> {
        let mut cells = BTreeMap::new();
        let mut sheets = HashMap::new();
        for sheet in &model.sheets {
            for key in [&sheet.id, &sheet.name] {
                let normalized = normalize_sheet(key);
                if let Some(previous) = sheets.insert(normalized.clone(), sheet.id.clone()) {
                    if previous != sheet.id {
                        return Err(FormulaCalculationError::UnknownSheet {
                            cell: CellAddress {
                                sheet_id: sheet.id.clone(),
                                row: 0,
                                column: 0,
                            },
                            sheet: normalized,
                        });
                    }
                }
            }
            for cell in &sheet.cells {
                cells.insert(
                    CellAddress {
                        sheet_id: sheet.id.clone(),
                        row: cell.row,
                        column: cell.column,
                    },
                    cell,
                );
            }
        }
        Ok(Self {
            cells,
            sheets,
            states: HashMap::new(),
            stack: Vec::new(),
            values: BTreeMap::new(),
        })
    }

    fn evaluate_cell(
        &mut self,
        address: &CellAddress,
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        if let Some(value) = self.values.get(address) {
            return Ok(value.clone());
        }
        if matches!(self.states.get(address), Some(VisitState::Visiting)) {
            let start = self
                .stack
                .iter()
                .position(|item| item == address)
                .unwrap_or(0);
            return Err(FormulaCalculationError::Cycle {
                cells: self.stack[start..].to_vec(),
            });
        }
        let cell =
            self.cells
                .get(address)
                .ok_or_else(|| FormulaCalculationError::UnknownReference {
                    cell: self
                        .stack
                        .last()
                        .cloned()
                        .unwrap_or_else(|| address.clone()),
                    reference: address.clone(),
                })?;
        self.states.insert(address.clone(), VisitState::Visiting);
        self.stack.push(address.clone());
        let result = if let Some(formula) = cell.formula.as_deref() {
            if formula.trim().is_empty() {
                Err(FormulaCalculationError::EmptyFormula {
                    cell: address.clone(),
                })
            } else {
                let expression = Parser::new(formula).parse(address)?;
                self.evaluate_expression(address, &expression)
            }
        } else {
            constant_value(address, cell.value.as_ref())
        };
        self.stack.pop();
        match result {
            Ok(value) => {
                self.states.insert(address.clone(), VisitState::Done);
                self.values.insert(address.clone(), value.clone());
                Ok(value)
            }
            Err(error) => {
                self.states.remove(address);
                Err(error)
            }
        }
    }

    fn evaluate_expression(
        &mut self,
        current: &CellAddress,
        expression: &Expr,
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        match expression {
            Expr::Number(value) => Ok(CalculatedValue::Number(*value)),
            Expr::Text(value) => Ok(CalculatedValue::Text(value.clone())),
            Expr::Reference(reference) => {
                let address = self.resolve_reference(current, reference)?;
                self.evaluate_cell(&address)
            }
            Expr::Unary { operator, value } => {
                let value = self.evaluate_expression(current, value)?;
                let number = number_value(current, value, "一元运算")?;
                let result = if *operator == '-' { -number } else { number };
                finite_number(current, result)
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left = number_value(
                    current,
                    self.evaluate_expression(current, left)?,
                    "四则运算",
                )?;
                let right = number_value(
                    current,
                    self.evaluate_expression(current, right)?,
                    "四则运算",
                )?;
                let result = match operator {
                    '+' => left + right,
                    '-' => left - right,
                    '*' => left * right,
                    '/' if right == 0.0 => {
                        return Err(FormulaCalculationError::DivisionByZero {
                            cell: current.clone(),
                        })
                    }
                    '/' => left / right,
                    _ => unreachable!("parser only creates supported operators"),
                };
                finite_number(current, result)
            }
        }
    }

    fn resolve_reference(
        &self,
        current: &CellAddress,
        reference: &Reference,
    ) -> Result<CellAddress, FormulaCalculationError> {
        let sheet_id = match &reference.sheet {
            Some(sheet) => self
                .sheets
                .get(&normalize_sheet(sheet))
                .cloned()
                .ok_or_else(|| FormulaCalculationError::UnknownSheet {
                    cell: current.clone(),
                    sheet: sheet.clone(),
                })?,
            None => current.sheet_id.clone(),
        };
        let address = CellAddress {
            sheet_id,
            row: reference.row,
            column: reference.column,
        };
        if !self.cells.contains_key(&address) {
            return Err(FormulaCalculationError::UnknownReference {
                cell: current.clone(),
                reference: address,
            });
        }
        Ok(address)
    }
}

fn normalize_sheet(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn constant_value(
    cell: &CellAddress,
    value: Option<&Value>,
) -> Result<CalculatedValue, FormulaCalculationError> {
    match value {
        None | Some(Value::Null) => Ok(CalculatedValue::Blank),
        Some(Value::Bool(value)) => Ok(CalculatedValue::Bool(*value)),
        Some(Value::String(value)) => Ok(CalculatedValue::Text(value.clone())),
        Some(Value::Number(value)) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(CalculatedValue::Number)
            .ok_or_else(|| FormulaCalculationError::UnsupportedValue { cell: cell.clone() }),
        Some(Value::Array(_) | Value::Object(_)) => {
            Err(FormulaCalculationError::UnsupportedValue { cell: cell.clone() })
        }
    }
}

fn number_value(
    cell: &CellAddress,
    value: CalculatedValue,
    operation: &str,
) -> Result<f64, FormulaCalculationError> {
    match value {
        CalculatedValue::Number(value) => Ok(value),
        _ => Err(FormulaCalculationError::TypeMismatch {
            cell: cell.clone(),
            operation: operation.into(),
        }),
    }
}

fn finite_number(
    cell: &CellAddress,
    value: f64,
) -> Result<CalculatedValue, FormulaCalculationError> {
    value
        .is_finite()
        .then_some(CalculatedValue::Number(value))
        .ok_or_else(|| FormulaCalculationError::NonFinite { cell: cell.clone() })
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Text(String),
    Reference(Reference),
    Unary {
        operator: char,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: char,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Reference {
    sheet: Option<String>,
    row: u32,
    column: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Reference(Reference),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
    Colon,
    ExternalWorkbook,
    End,
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(formula: &str) -> Self {
        let source = formula.trim().strip_prefix('=').unwrap_or(formula.trim());
        Self {
            tokens: Lexer::new(source).tokenize(),
            position: 0,
        }
    }

    fn parse(mut self, cell: &CellAddress) -> Result<Expr, FormulaCalculationError> {
        let expression = self.parse_expression(cell)?;
        match self.peek() {
            Token::End => Ok(expression),
            Token::Colon => Err(FormulaCalculationError::UnsupportedRange { cell: cell.clone() }),
            Token::ExternalWorkbook => {
                Err(FormulaCalculationError::UnsupportedExternalWorkbook { cell: cell.clone() })
            }
            token => Err(self.syntax(cell, format!("多余的 token {token:?}"))),
        }
    }

    fn parse_expression(&mut self, cell: &CellAddress) -> Result<Expr, FormulaCalculationError> {
        let mut expression = self.parse_term(cell)?;
        loop {
            let operator = match self.peek() {
                Token::Plus => '+',
                Token::Minus => '-',
                _ => break,
            };
            self.next();
            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(self.parse_term(cell)?),
            };
        }
        Ok(expression)
    }

    fn parse_term(&mut self, cell: &CellAddress) -> Result<Expr, FormulaCalculationError> {
        let mut expression = self.parse_factor(cell)?;
        loop {
            let operator = match self.peek() {
                Token::Star => '*',
                Token::Slash => '/',
                _ => break,
            };
            self.next();
            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(self.parse_factor(cell)?),
            };
        }
        Ok(expression)
    }

    fn parse_factor(&mut self, cell: &CellAddress) -> Result<Expr, FormulaCalculationError> {
        match self.next() {
            Token::Plus => Ok(Expr::Unary {
                operator: '+',
                value: Box::new(self.parse_factor(cell)?),
            }),
            Token::Minus => Ok(Expr::Unary {
                operator: '-',
                value: Box::new(self.parse_factor(cell)?),
            }),
            Token::Number(value) => Ok(Expr::Number(value)),
            Token::Text(value) => Ok(Expr::Text(value)),
            Token::Reference(reference) => {
                if matches!(self.peek(), Token::Colon) {
                    return Err(FormulaCalculationError::UnsupportedRange { cell: cell.clone() });
                }
                Ok(Expr::Reference(reference))
            }
            Token::LeftParen => {
                let expression = self.parse_expression(cell)?;
                if !matches!(self.next(), Token::RightParen) {
                    return Err(self.syntax(cell, "缺少右括号"));
                }
                Ok(expression)
            }
            Token::Identifier(function) if matches!(self.peek(), Token::LeftParen) => {
                Err(FormulaCalculationError::UnsupportedFunction {
                    cell: cell.clone(),
                    function,
                })
            }
            Token::Identifier(identifier) => {
                Err(self.syntax(cell, format!("未知标识符 {identifier}")))
            }
            Token::ExternalWorkbook => {
                Err(FormulaCalculationError::UnsupportedExternalWorkbook { cell: cell.clone() })
            }
            token => Err(self.syntax(cell, format!("意外的 token {token:?}"))),
        }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::End)
    }

    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        self.position += 1;
        token
    }

    fn syntax(&self, cell: &CellAddress, message: impl Into<String>) -> FormulaCalculationError {
        FormulaCalculationError::Syntax {
            cell: cell.clone(),
            message: message.into(),
        }
    }
}

struct Lexer {
    chars: Vec<char>,
    position: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            position: 0,
        }
    }

    fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let done = matches!(token, Token::End);
            tokens.push(token);
            if done {
                return tokens;
            }
        }
    }

    fn next_token(&mut self) -> Token {
        while self.position < self.chars.len() && self.chars[self.position].is_whitespace() {
            self.position += 1;
        }
        let Some(character) = self.chars.get(self.position).copied() else {
            return Token::End;
        };
        match character {
            '+' => {
                self.position += 1;
                Token::Plus
            }
            '-' => {
                self.position += 1;
                Token::Minus
            }
            '*' => {
                self.position += 1;
                Token::Star
            }
            '/' => {
                self.position += 1;
                Token::Slash
            }
            '(' => {
                self.position += 1;
                Token::LeftParen
            }
            ')' => {
                self.position += 1;
                Token::RightParen
            }
            ':' => {
                self.position += 1;
                Token::Colon
            }
            '[' => {
                self.position += 1;
                Token::ExternalWorkbook
            }
            '"' => self.read_text(),
            '\'' => self.read_quoted_sheet_reference(),
            '$' | 'A'..='Z' | 'a'..='z' => self.read_word_or_reference(),
            '0'..='9' | '.' => self.read_number(),
            _ => {
                self.position += 1;
                Token::Identifier(character.to_string())
            }
        }
    }

    fn read_text(&mut self) -> Token {
        self.position += 1;
        let mut value = String::new();
        while self.position < self.chars.len() {
            let character = self.chars[self.position];
            self.position += 1;
            if character == '"' {
                if self.chars.get(self.position) == Some(&'"') {
                    value.push('"');
                    self.position += 1;
                } else {
                    return Token::Text(value);
                }
            } else {
                value.push(character);
            }
        }
        Token::Identifier("unterminated string".into())
    }

    fn read_quoted_sheet_reference(&mut self) -> Token {
        self.position += 1;
        let mut sheet = String::new();
        while self.position < self.chars.len() {
            let character = self.chars[self.position];
            self.position += 1;
            if character == '\'' {
                if self.chars.get(self.position) == Some(&'\'') {
                    sheet.push('\'');
                    self.position += 1;
                    continue;
                }
                if self.chars.get(self.position) == Some(&'!') {
                    self.position += 1;
                    return self.read_reference(Some(sheet));
                }
                return Token::Identifier("未闭合的工作表引用".into());
            }
            sheet.push(character);
        }
        Token::Identifier("未闭合的工作表引用".into())
    }

    fn read_word_or_reference(&mut self) -> Token {
        let start = self.position;
        if self.chars[self.position] == '$' {
            return self.read_reference(None);
        }
        while self.chars.get(self.position).is_some_and(|character| {
            character.is_ascii_alphanumeric() || *character == '_' || *character == '.'
        }) {
            self.position += 1;
        }
        if self.chars.get(self.position) == Some(&'!') {
            let sheet = self.chars[start..self.position].iter().collect::<String>();
            self.position += 1;
            return self.read_reference(Some(sheet));
        }
        let word = self.chars[start..self.position].iter().collect::<String>();
        if looks_like_reference(&word) {
            if let Some(reference) = parse_reference(&word, None) {
                return Token::Reference(reference);
            }
        }
        Token::Identifier(word)
    }

    fn read_reference(&mut self, sheet: Option<String>) -> Token {
        let start = self.position;
        if self.chars.get(self.position) == Some(&'$') {
            self.position += 1;
        }
        while self
            .chars
            .get(self.position)
            .is_some_and(|character| character.is_ascii_alphabetic())
        {
            self.position += 1;
        }
        if self.chars.get(self.position) == Some(&'$') {
            self.position += 1;
        }
        while self
            .chars
            .get(self.position)
            .is_some_and(|character| character.is_ascii_digit())
        {
            self.position += 1;
        }
        let value = self.chars[start..self.position].iter().collect::<String>();
        parse_reference(&value, sheet).map_or_else(
            || Token::Identifier("无效单元格引用".into()),
            Token::Reference,
        )
    }

    fn read_number(&mut self) -> Token {
        let start = self.position;
        while self.chars.get(self.position).is_some_and(|character| {
            character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E' | '+' | '-')
        }) {
            self.position += 1;
        }
        let text = self.chars[start..self.position].iter().collect::<String>();
        text.parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map_or_else(
                || Token::Identifier(format!("无效数字 {text}")),
                Token::Number,
            )
    }
}

fn looks_like_reference(value: &str) -> bool {
    let trimmed = value.trim_start_matches('$');
    let letters = trimmed
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .count();
    letters > 0
        && letters <= 4
        && trimmed[letters..]
            .chars()
            .all(|character| character.is_ascii_digit())
        && !trimmed[letters..].is_empty()
}

fn parse_reference(value: &str, sheet: Option<String>) -> Option<Reference> {
    let mut value = value;
    if let Some(stripped) = value.strip_prefix('$') {
        value = stripped;
    }
    let column_end = value
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .count();
    if column_end == 0 || column_end > 4 {
        return None;
    }
    let row_start = if value.as_bytes().get(column_end) == Some(&b'$') {
        column_end + 1
    } else {
        column_end
    };
    let row_text = value.get(row_start..)?;
    if row_text.is_empty() || !row_text.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let row = row_text.parse::<u32>().ok()?.checked_sub(1)?;
    let mut column = 0u32;
    for character in value[..column_end].chars() {
        column = column
            .checked_mul(26)?
            .checked_add((character.to_ascii_uppercase() as u8).checked_sub(b'A')? as u32 + 1)?;
    }
    Some(Reference {
        sheet,
        row,
        column: column.checked_sub(1)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{CellModel, SheetModel};

    #[test]
    fn error_projection_keeps_unrelated_values_and_marks_cycles() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![
                    CellModel {
                        row: 0,
                        column: 0,
                        value: Some(1.into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 0,
                        column: 1,
                        formula: Some("=C1".into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 0,
                        column: 2,
                        formula: Some("=B1".into()),
                        ..CellModel::default()
                    },
                ],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let result = calculate_with_errors(&model).unwrap();
        assert_eq!(
            result.value(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 0
            }),
            Some(&CalculatedValue::Number(1.0))
        );
        assert!(matches!(
            result.value(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 1
            }),
            Some(CalculatedValue::Error {
                code: FormulaErrorCode::Cycle,
                ..
            })
        ));
        assert!(matches!(
            result.value(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 2
            }),
            Some(CalculatedValue::Error {
                code: FormulaErrorCode::Cycle,
                ..
            })
        ));
    }

    #[test]
    fn division_by_zero_is_a_typed_error_value() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![CellModel {
                    row: 0,
                    column: 0,
                    formula: Some("=1/0".into()),
                    ..CellModel::default()
                }],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let result = calculate_with_errors(&model).unwrap();
        assert!(matches!(
            result.values.values().next(),
            Some(CalculatedValue::Error {
                code: FormulaErrorCode::DivisionByZero,
                ..
            })
        ));
    }
}
