//! Pure spreadsheet formula calculation as a derived projection.
//!
//! This module reads a validated [`SpreadsheetModel`] and never writes a calculated value back
//! to a canonical cell.  The first evaluator deliberately has a small, explicit grammar: numeric
//! constants, cell references, parentheses and `+ - * /`.  Functions, ranges, external workbook
//! references, cycles and unknown references are rejected instead of being guessed.

use std::collections::{BTreeMap, HashMap};

use oo_schema::{ArtifactEnvelope, ArtifactPayload, CellModel, DateSystem, SpreadsheetModel};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SpreadsheetFunctionCategory {
    Aggregate,
    Math,
    Logic,
    Text,
    Statistics,
    DateTime,
    Lookup,
    Financial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetFunctionDescriptor {
    pub name: &'static str,
    pub category: SpreadsheetFunctionCategory,
    pub volatile: bool,
}

/// Explicit formula support matrix. `TODAY`/`NOW` are the only volatile
/// functions; unknown names remain typed `unsupportedFunction` errors.
pub fn spreadsheet_function_catalog() -> &'static [SpreadsheetFunctionDescriptor] {
    use SpreadsheetFunctionCategory as C;
    const FUNCTIONS: &[SpreadsheetFunctionDescriptor] = &[
        SpreadsheetFunctionDescriptor {
            name: "SUM",
            category: C::Aggregate,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "AVERAGE",
            category: C::Aggregate,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MIN",
            category: C::Aggregate,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MAX",
            category: C::Aggregate,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "COUNT",
            category: C::Aggregate,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "ROUND",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "ROUNDUP",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "ROUNDDOWN",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "ABS",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "INT",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MOD",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "POWER",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "SQRT",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "CEILING",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "FLOOR",
            category: C::Math,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "AND",
            category: C::Logic,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "OR",
            category: C::Logic,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "NOT",
            category: C::Logic,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "IF",
            category: C::Logic,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "IFERROR",
            category: C::Logic,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "CONCAT",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "LEFT",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "RIGHT",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MID",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "LEN",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "UPPER",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "LOWER",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "TRIM",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "SUBSTITUTE",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "REPLACE",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "FIND",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "SEARCH",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "EXACT",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "TEXTJOIN",
            category: C::Text,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "COUNTA",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "COUNTIF",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "SUMIF",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "AVERAGEIF",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MEDIAN",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "STDEV.S",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "LARGE",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "SMALL",
            category: C::Statistics,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "TODAY",
            category: C::DateTime,
            volatile: true,
        },
        SpreadsheetFunctionDescriptor {
            name: "NOW",
            category: C::DateTime,
            volatile: true,
        },
        SpreadsheetFunctionDescriptor {
            name: "DATE",
            category: C::DateTime,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "YEAR",
            category: C::DateTime,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MONTH",
            category: C::DateTime,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "DAY",
            category: C::DateTime,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "VLOOKUP",
            category: C::Lookup,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "HLOOKUP",
            category: C::Lookup,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "MATCH",
            category: C::Lookup,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "INDEX",
            category: C::Lookup,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "XLOOKUP",
            category: C::Lookup,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "PV",
            category: C::Financial,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "FV",
            category: C::Financial,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "PMT",
            category: C::Financial,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "NPV",
            category: C::Financial,
            volatile: false,
        },
        SpreadsheetFunctionDescriptor {
            name: "IRR",
            category: C::Financial,
            volatile: false,
        },
    ];
    FUNCTIONS
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
    for address in materialized_addresses(model) {
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
    for address in materialized_addresses(model) {
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

/// Enumerates every materialized cell address without allocating an index.
fn materialized_addresses(model: &SpreadsheetModel) -> Vec<CellAddress> {
    model
        .sheets
        .iter()
        .flat_map(|sheet| {
            sheet.cells.iter().map(move |cell| CellAddress {
                sheet_id: sheet.id.clone(),
                row: cell.row,
                column: cell.column,
            })
        })
        .collect()
}

/// Computes the derived values for exactly the requested addresses. The
/// evaluator is pull-based and memoized, so each target transitively evaluates
/// only the cells it reads — the evaluation set is graph-bounded, not
/// model-bounded. A viewport request on a 100k-cell sheet therefore no longer
/// evaluates every materialized cell; it evaluates the window's formulas plus
/// their transitive inputs.
///
/// The model must already be schema-validated (server artifacts and engine
/// output always are); unlike [`calculate_with_errors`] this entry point does
/// not re-clone the whole payload for validation. Failures are projected as
/// typed error values, exactly like [`calculate_with_errors`].
pub fn calculate_targets_with_errors(
    model: &SpreadsheetModel,
    targets: &[CellAddress],
) -> Result<CalculationResult, FormulaCalculationError> {
    let mut calculator = Calculator::new(model)?;
    let mut values = BTreeMap::new();
    for address in targets {
        if values.contains_key(address) {
            continue;
        }
        let value = match calculator.evaluate_cell(address) {
            Ok(value) => value,
            Err(error) => CalculatedValue::from_error(&error),
        };
        values.insert(address.clone(), value);
    }
    Ok(CalculationResult { values })
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
    model: &'a SpreadsheetModel,
    sheets: HashMap<String, String>,
    /// sheet id -> position in `model.sheets`; built on demand per sheet id.
    sheet_positions: std::cell::RefCell<HashMap<String, usize>>,
    /// sheet position -> cells verified sorted by (row, column); the engine
    /// keeps the sparse Vec sorted, so lookups can binary-search instead of
    /// cloning every cell into a BTreeMap per request.
    sorted_sheets: std::cell::RefCell<HashMap<usize, bool>>,
    states: HashMap<CellAddress, VisitState>,
    stack: Vec<CellAddress>,
    values: BTreeMap<CellAddress, CalculatedValue>,
}

impl<'a> Calculator<'a> {
    fn new(model: &'a SpreadsheetModel) -> Result<Self, FormulaCalculationError> {
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
        }
        Ok(Self {
            model,
            sheets,
            sheet_positions: std::cell::RefCell::new(HashMap::new()),
            sorted_sheets: std::cell::RefCell::new(HashMap::new()),
            states: HashMap::new(),
            stack: Vec::new(),
            values: BTreeMap::new(),
        })
    }

    /// Resolves one materialized cell by address. Sheets are looked up through
    /// a lazily built id -> position map and cells through binary search, so a
    /// bounded viewport evaluation never allocates a per-cell index.
    fn cell_at(&self, address: &CellAddress) -> Option<&'a CellModel> {
        let position = {
            let mut positions = self.sheet_positions.borrow_mut();
            *positions
                .entry(address.sheet_id.clone())
                .or_insert_with_key(|sheet_id| {
                    self.model
                        .sheets
                        .iter()
                        .position(|sheet| sheet.id == *sheet_id)
                        .unwrap_or(usize::MAX)
                })
        };
        if position == usize::MAX {
            return None;
        }
        let sheet = &self.model.sheets[position];
        let sorted = {
            let mut sorted = self.sorted_sheets.borrow_mut();
            *sorted.entry(position).or_insert_with(|| {
                sheet
                    .cells
                    .windows(2)
                    .all(|pair| (pair[0].row, pair[0].column) < (pair[1].row, pair[1].column))
            })
        };
        if sorted {
            sheet
                .cells
                .binary_search_by_key(&(address.row, address.column), |cell| {
                    (cell.row, cell.column)
                })
                .ok()
                .map(|index| &sheet.cells[index])
        } else {
            sheet
                .cells
                .iter()
                .find(|cell| cell.row == address.row && cell.column == address.column)
        }
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
            self.cell_at(address)
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
            Expr::Range { .. } => Err(FormulaCalculationError::UnsupportedRange {
                cell: current.clone(),
            }),
            Expr::Function { name, args } => self.evaluate_function(current, name, args),
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
            } => match operator {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
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
                        BinOp::Add => left + right,
                        BinOp::Sub => left - right,
                        BinOp::Mul => left * right,
                        BinOp::Div if right == 0.0 => {
                            return Err(FormulaCalculationError::DivisionByZero {
                                cell: current.clone(),
                            })
                        }
                        BinOp::Div => left / right,
                        _ => unreachable!("arithmetic operator"),
                    };
                    finite_number(current, result)
                }
                BinOp::Eq | BinOp::Ne | BinOp::Gt | BinOp::Lt | BinOp::Ge | BinOp::Le => {
                    let left = self.evaluate_expression(current, left)?;
                    let right = self.evaluate_expression(current, right)?;
                    Ok(CalculatedValue::Bool(compare_values(
                        current, left, right, *operator,
                    )?))
                }
            },
        }
    }

    /// Evaluates a cell expression into a value list. A range yields every cell
    /// in the box (empty cells become `Blank`), whereas any other expression
    /// yields a single value. This is how `SUM(A1:A5)` / `AVG` consume ranges
    /// without a second aggregation model in the browser.
    fn evaluate_values(
        &mut self,
        current: &CellAddress,
        expression: &Expr,
    ) -> Result<Vec<CalculatedValue>, FormulaCalculationError> {
        match expression {
            Expr::Range { start, end } => {
                let start = self.resolve_coordinate(current, start)?;
                let end = self.resolve_coordinate(current, end)?;
                if start.sheet_id != end.sheet_id {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "跨工作表范围".into(),
                    });
                }
                let sheet_id = start.sheet_id.clone();
                let mut values = Vec::new();
                for row in start.row.min(end.row)..=start.row.max(end.row) {
                    for column in start.column.min(end.column)..=start.column.max(end.column) {
                        let address = CellAddress {
                            sheet_id: sheet_id.clone(),
                            row,
                            column,
                        };
                        values.push(self.value_at(&address)?);
                    }
                }
                Ok(values)
            }
            other => Ok(vec![self.evaluate_expression(current, other)?]),
        }
    }

    fn value_at(
        &mut self,
        address: &CellAddress,
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        if self.cell_at(address).is_some() {
            self.evaluate_cell(address)
        } else {
            Ok(CalculatedValue::Blank)
        }
    }

    fn evaluate_function(
        &mut self,
        current: &CellAddress,
        name: &str,
        args: &[Expr],
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        let function = name.trim().to_ascii_uppercase();
        match function.as_str() {
            // ---- 数学（10）----
            "ROUND" => {
                let number = self.single_number(current, args, 0)?;
                let digits = self.optional_number(current, args, 1, 0.0)?;
                let factor = 10f64.powf(digits);
                Ok(CalculatedValue::Number((number * factor).round() / factor))
            }
            "ROUNDUP" => {
                let number = self.single_number(current, args, 0)?;
                let digits = self.optional_number(current, args, 1, 0.0)?;
                let factor = 10f64.powf(digits);
                let scaled = number * factor;
                let rounded = if scaled > 0.0 {
                    scaled.ceil()
                } else {
                    scaled.floor()
                };
                Ok(CalculatedValue::Number(rounded / factor))
            }
            "ROUNDDOWN" => {
                let number = self.single_number(current, args, 0)?;
                let digits = self.optional_number(current, args, 1, 0.0)?;
                let factor = 10f64.powf(digits);
                let scaled = number * factor;
                let rounded = if scaled > 0.0 {
                    scaled.floor()
                } else {
                    scaled.ceil()
                };
                Ok(CalculatedValue::Number(rounded / factor))
            }
            "ABS" => {
                let number = self.single_number(current, args, 0)?;
                Ok(CalculatedValue::Number(number.abs()))
            }
            "INT" => {
                let number = self.single_number(current, args, 0)?;
                Ok(CalculatedValue::Number(number.floor()))
            }
            "MOD" => {
                let number = self.single_number(current, args, 0)?;
                let divisor = self.single_number(current, args, 1)?;
                if divisor == 0.0 {
                    return Err(FormulaCalculationError::DivisionByZero {
                        cell: current.clone(),
                    });
                }
                Ok(CalculatedValue::Number(
                    number - divisor * (number / divisor).floor(),
                ))
            }
            "POWER" => {
                let base = self.single_number(current, args, 0)?;
                let exponent = self.single_number(current, args, 1)?;
                finite_number(current, base.powf(exponent))
            }
            "SQRT" => {
                let number = self.single_number(current, args, 0)?;
                if number < 0.0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "SQRT 负数".into(),
                    });
                }
                Ok(CalculatedValue::Number(number.sqrt()))
            }
            "CEILING" => {
                let number = self.single_number(current, args, 0)?;
                let factor = self.optional_number(current, args, 1, 1.0)?;
                if factor == 0.0 {
                    return Ok(CalculatedValue::Number(0.0));
                }
                Ok(CalculatedValue::Number((number / factor).ceil() * factor))
            }
            "FLOOR" => {
                let number = self.single_number(current, args, 0)?;
                let factor = self.optional_number(current, args, 1, 1.0)?;
                if factor == 0.0 {
                    return Err(FormulaCalculationError::DivisionByZero {
                        cell: current.clone(),
                    });
                }
                Ok(CalculatedValue::Number((number / factor).floor() * factor))
            }
            // ---- 逻辑（3）----
            "AND" => {
                let values = self.collect_all(current, args)?;
                if values.is_empty() {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "AND 无参数".into(),
                    });
                }
                Ok(CalculatedValue::Bool(values.iter().all(is_truthy_value)))
            }
            "OR" => {
                let values = self.collect_all(current, args)?;
                Ok(CalculatedValue::Bool(values.iter().any(is_truthy_value)))
            }
            "NOT" => {
                let value = self.evaluate_expression(current, first_arg(args)?)?;
                Ok(CalculatedValue::Bool(!is_truthy(current, value)?))
            }
            // ---- 文本（12）----
            "CONCAT" | "CONCATENATE" => {
                let values = self.collect_all(current, args)?;
                Ok(CalculatedValue::Text(
                    values.iter().map(as_text).collect::<String>(),
                ))
            }
            "LEFT" => {
                let text = self.text_arg(current, args, 0)?;
                let count = self.optional_number(current, args, 1, 1.0)? as usize;
                Ok(CalculatedValue::Text(text.chars().take(count).collect()))
            }
            "RIGHT" => {
                let text = self.text_arg(current, args, 0)?;
                let count = self.optional_number(current, args, 1, 1.0)? as usize;
                let chars: Vec<char> = text.chars().collect();
                let start = chars.len().saturating_sub(count);
                Ok(CalculatedValue::Text(chars[start..].iter().collect()))
            }
            "MID" => {
                let text = self.text_arg(current, args, 0)?;
                let start = self.single_number(current, args, 1)?;
                let count = self.single_number(current, args, 2)?;
                if start < 1.0 || count < 0.0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "MID 参数越界".into(),
                    });
                }
                let chars: Vec<char> = text.chars().collect();
                let begin = (start as usize).saturating_sub(1);
                Ok(CalculatedValue::Text(
                    chars.iter().skip(begin).take(count as usize).collect(),
                ))
            }
            "LEN" => {
                let text = self.text_arg(current, args, 0)?;
                Ok(CalculatedValue::Number(text.chars().count() as f64))
            }
            "UPPER" => {
                let text = self.text_arg(current, args, 0)?;
                Ok(CalculatedValue::Text(text.to_uppercase()))
            }
            "LOWER" => {
                let text = self.text_arg(current, args, 0)?;
                Ok(CalculatedValue::Text(text.to_lowercase()))
            }
            "TRIM" => {
                let text = self.text_arg(current, args, 0)?;
                Ok(CalculatedValue::Text(
                    text.split_whitespace().collect::<Vec<_>>().join(" "),
                ))
            }
            "SUBSTITUTE" => {
                let text = self.text_arg(current, args, 0)?;
                let search = self.text_arg(current, args, 1)?;
                let replace = self.text_arg(current, args, 2)?;
                if search.is_empty() {
                    return Ok(CalculatedValue::Text(text));
                }
                Ok(CalculatedValue::Text(text.replace(&search, &replace)))
            }
            "REPLACE" => {
                let text = self.text_arg(current, args, 0)?;
                let start = self.single_number(current, args, 1)?;
                let count = self.single_number(current, args, 2)?;
                let replacement = self.text_arg(current, args, 3)?;
                if start < 1.0 || count < 0.0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "REPLACE 参数越界".into(),
                    });
                }
                let chars: Vec<char> = text.chars().collect();
                let begin = (start as usize).saturating_sub(1).min(chars.len());
                let end = (begin + count as usize).min(chars.len());
                let mut out = String::new();
                out.extend(&chars[..begin]);
                out.push_str(&replacement);
                out.extend(&chars[end..]);
                Ok(CalculatedValue::Text(out))
            }
            "FIND" => {
                let needle = self.text_arg(current, args, 0)?;
                let haystack = self.text_arg(current, args, 1)?;
                let start = self.optional_number(current, args, 2, 1.0)?;
                if start < 1.0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "FIND 起始位越界".into(),
                    });
                }
                let haystack_chars: Vec<char> = haystack.chars().collect();
                let from = (start as usize).saturating_sub(1).min(haystack_chars.len());
                let needle_chars: Vec<char> = needle.chars().collect();
                let found = haystack_chars[from..]
                    .windows(needle_chars.len().max(1))
                    .position(|window| window == needle_chars.as_slice())
                    .map(|position| position + from + 1);
                Ok(CalculatedValue::Number(found.ok_or_else(|| {
                    FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "FIND 未找到子串".into(),
                    }
                })? as f64))
            }
            "SEARCH" => {
                let needle = self.text_arg(current, args, 0)?.to_lowercase();
                let haystack = self.text_arg(current, args, 1)?.to_lowercase();
                let start = self.optional_number(current, args, 2, 1.0)?;
                if start < 1.0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "SEARCH 起始位越界".into(),
                    });
                }
                let from = (start as usize)
                    .saturating_sub(1)
                    .min(haystack.chars().count());
                let suffix: String = haystack.chars().skip(from).collect();
                let byte =
                    suffix
                        .find(&needle)
                        .ok_or_else(|| FormulaCalculationError::TypeMismatch {
                            cell: current.clone(),
                            operation: "SEARCH 未找到子串".into(),
                        })?;
                let scalar_offset = suffix[..byte].chars().count();
                Ok(CalculatedValue::Number((from + scalar_offset + 1) as f64))
            }
            "EXACT" => Ok(CalculatedValue::Bool(
                self.text_arg(current, args, 0)? == self.text_arg(current, args, 1)?,
            )),
            "TEXTJOIN" => {
                let delimiter = self.text_arg(current, args, 0)?;
                let ignore_empty = is_truthy(
                    current,
                    self.evaluate_expression(current, second_arg(args)?)?,
                )?;
                let mut values = Vec::new();
                for argument in args.iter().skip(2) {
                    values.extend(self.evaluate_values(current, argument)?);
                }
                Ok(CalculatedValue::Text(
                    values
                        .into_iter()
                        .filter_map(|value| {
                            let text = as_text(&value);
                            (!ignore_empty || !text.is_empty()).then_some(text)
                        })
                        .collect::<Vec<_>>()
                        .join(&delimiter),
                ))
            }
            // ---- 统计（4）----
            "COUNTA" => {
                let values = self.collect_all(current, args)?;
                Ok(CalculatedValue::Number(
                    values
                        .iter()
                        .filter(|value| !matches!(value, CalculatedValue::Blank))
                        .count() as f64,
                ))
            }
            "COUNTIF" => {
                let criteria_values = self.evaluate_values(current, first_arg(args)?)?;
                let criteria = self.evaluate_expression(current, second_arg(args)?)?;
                Ok(CalculatedValue::Number(
                    criteria_values
                        .iter()
                        .filter(|value| matches_criteria(value, &criteria))
                        .count() as f64,
                ))
            }
            "SUMIF" => {
                let range_values = self.evaluate_values(current, first_arg(args)?)?;
                let criteria = self.evaluate_expression(current, second_arg(args)?)?;
                let sum_range_values = if args.len() >= 3 {
                    self.evaluate_values(current, &args[2])?
                } else {
                    range_values.clone()
                };
                let total: f64 = range_values
                    .iter()
                    .zip(sum_range_values.iter())
                    .filter(|(value, _)| matches_criteria(value, &criteria))
                    .filter_map(|(_, sum)| as_number(sum))
                    .sum();
                Ok(CalculatedValue::Number(total))
            }
            "MEDIAN" => {
                let mut numbers = self.collect_numeric(current, args)?;
                numbers.sort_by(f64::total_cmp);
                let value = match numbers.len() {
                    0 => {
                        return Err(FormulaCalculationError::TypeMismatch {
                            cell: current.clone(),
                            operation: "MEDIAN 空集".into(),
                        })
                    }
                    length if length % 2 == 1 => numbers[length / 2],
                    length => (numbers[length / 2 - 1] + numbers[length / 2]) / 2.0,
                };
                Ok(CalculatedValue::Number(value))
            }
            "AVERAGEIF" => {
                let range_values = self.evaluate_values(current, first_arg(args)?)?;
                let criteria = self.evaluate_expression(current, second_arg(args)?)?;
                let average_values = if args.len() >= 3 {
                    self.evaluate_values(current, &args[2])?
                } else {
                    range_values.clone()
                };
                let numbers: Vec<f64> = range_values
                    .iter()
                    .zip(average_values.iter())
                    .filter(|(value, _)| matches_criteria(value, &criteria))
                    .filter_map(|(_, value)| as_number(value))
                    .collect();
                if numbers.is_empty() {
                    return Err(FormulaCalculationError::DivisionByZero {
                        cell: current.clone(),
                    });
                }
                Ok(CalculatedValue::Number(
                    numbers.iter().sum::<f64>() / numbers.len() as f64,
                ))
            }
            "STDEV" | "STDEV.S" => {
                let values = self.collect_numeric(current, args)?;
                if values.len() < 2 {
                    return Err(FormulaCalculationError::DivisionByZero {
                        cell: current.clone(),
                    });
                }
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                let variance = values
                    .iter()
                    .map(|value| (value - mean).powi(2))
                    .sum::<f64>()
                    / (values.len() - 1) as f64;
                Ok(CalculatedValue::Number(variance.sqrt()))
            }
            "LARGE" | "SMALL" => {
                let mut values = self
                    .evaluate_values(current, first_arg(args)?)?
                    .iter()
                    .filter_map(as_number)
                    .collect::<Vec<_>>();
                values.sort_by(f64::total_cmp);
                let rank = self.single_number(current, args, 1)? as usize;
                if rank == 0 || rank > values.len() {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: format!("{function} 排名越界"),
                    });
                }
                let index = if function == "LARGE" {
                    values.len() - rank
                } else {
                    rank - 1
                };
                Ok(CalculatedValue::Number(values[index]))
            }
            // ---- 日期（6）----
            "TODAY" => Ok(CalculatedValue::Number(
                serial_now(self.model.metadata.date_system).floor(),
            )),
            "NOW" => Ok(CalculatedValue::Number(serial_now(
                self.model.metadata.date_system,
            ))),
            "DATE" => {
                let year = self.single_number(current, args, 0)? as i64;
                let month = self.single_number(current, args, 1)? as i64;
                let day = self.single_number(current, args, 2)? as i64;
                if !(1..=9999).contains(&year)
                    || !(1..=12).contains(&month)
                    || !(1..=31).contains(&day)
                {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "DATE 年月日越界".into(),
                    });
                }
                Ok(CalculatedValue::Number(serial_from_ymd_system(
                    year,
                    month,
                    day,
                    self.model.metadata.date_system,
                )))
            }
            "YEAR" => {
                let (year, _, _) = ymd_from_serial_system(
                    self.single_number(current, args, 0)?,
                    self.model.metadata.date_system,
                );
                Ok(CalculatedValue::Number(year as f64))
            }
            "MONTH" => {
                let (_, month, _) = ymd_from_serial_system(
                    self.single_number(current, args, 0)?,
                    self.model.metadata.date_system,
                );
                Ok(CalculatedValue::Number(month as f64))
            }
            "DAY" => {
                let (_, _, day) = ymd_from_serial_system(
                    self.single_number(current, args, 0)?,
                    self.model.metadata.date_system,
                );
                Ok(CalculatedValue::Number(day as f64))
            }
            // ---- 查找（1）----
            "VLOOKUP" => {
                let needle = self.evaluate_expression(current, first_arg(args)?)?;
                let Expr::Range { start, end } = second_arg(args)? else {
                    return Err(FormulaCalculationError::UnsupportedRange {
                        cell: current.clone(),
                    });
                };
                let start = self.resolve_coordinate(current, start)?;
                let end = self.resolve_coordinate(current, end)?;
                if start.sheet_id != end.sheet_id {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "跨工作表 VLOOKUP".into(),
                    });
                }
                let column_index = self.single_number(current, args, 2)? as usize;
                if column_index == 0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "VLOOKUP 列序号从 1 开始".into(),
                    });
                }
                let top = start.row.min(end.row);
                let left = start.column.min(end.column);
                let bottom = start.row.max(end.row);
                let width = (start.column.abs_diff(end.column) + 1) as usize;
                if column_index > width {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "VLOOKUP 列序号超出表宽".into(),
                    });
                }
                for row in top..=bottom {
                    let candidate = self.value_at(&CellAddress {
                        sheet_id: start.sheet_id.clone(),
                        row,
                        column: left,
                    })?;
                    if values_equal_loose(&candidate, &needle) {
                        let result = self.value_at(&CellAddress {
                            sheet_id: start.sheet_id.clone(),
                            row,
                            column: left + (column_index - 1) as u32,
                        })?;
                        return Ok(result);
                    }
                }
                Err(FormulaCalculationError::TypeMismatch {
                    cell: current.clone(),
                    operation: "VLOOKUP 未找到匹配值".into(),
                })
            }
            "HLOOKUP" => self.lookup_horizontal(current, args),
            "MATCH" => {
                let needle = self.evaluate_expression(current, first_arg(args)?)?;
                let values = self.evaluate_values(current, second_arg(args)?)?;
                let mode = self.optional_number(current, args, 2, 0.0)?;
                if mode != 0.0 {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "MATCH 当前只支持精确匹配 0".into(),
                    });
                }
                let index = values
                    .iter()
                    .position(|value| values_equal_loose(value, &needle))
                    .ok_or_else(|| FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "MATCH 未找到匹配值".into(),
                    })?;
                Ok(CalculatedValue::Number((index + 1) as f64))
            }
            "INDEX" => self.index_range(current, args),
            "XLOOKUP" => {
                let needle = self.evaluate_expression(current, first_arg(args)?)?;
                let lookup = self.evaluate_values(current, second_arg(args)?)?;
                let returns = self.evaluate_values(
                    current,
                    args.get(2)
                        .ok_or_else(|| FormulaCalculationError::TypeMismatch {
                            cell: current.clone(),
                            operation: "XLOOKUP 缺少返回区域".into(),
                        })?,
                )?;
                if lookup.len() != returns.len() {
                    return Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "XLOOKUP 查找与返回区域大小不一致".into(),
                    });
                }
                if let Some(index) = lookup
                    .iter()
                    .position(|value| values_equal_loose(value, &needle))
                {
                    return Ok(returns[index].clone());
                }
                if let Some(fallback) = args.get(3) {
                    self.evaluate_expression(current, fallback)
                } else {
                    Err(FormulaCalculationError::TypeMismatch {
                        cell: current.clone(),
                        operation: "XLOOKUP 未找到匹配值".into(),
                    })
                }
            }
            // ---- 财务 ----
            "PV" | "FV" | "PMT" => self.evaluate_financial_annuity(current, &function, args),
            "NPV" => {
                let rate = self.single_number(current, args, 0)?;
                let mut values = Vec::new();
                for argument in args.iter().skip(1) {
                    values.extend(self.evaluate_values(current, argument)?);
                }
                let result = values
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| {
                        as_number(value).map(|number| number / (1.0 + rate).powi(index as i32 + 1))
                    })
                    .sum();
                finite_number(current, result)
            }
            "IRR" => {
                let values = self
                    .evaluate_values(current, first_arg(args)?)?
                    .iter()
                    .filter_map(as_number)
                    .collect::<Vec<_>>();
                let mut rate = self.optional_number(current, args, 1, 0.1)?;
                for _ in 0..64 {
                    let value: f64 = values
                        .iter()
                        .enumerate()
                        .map(|(period, cash)| cash / (1.0 + rate).powi(period as i32))
                        .sum();
                    let derivative: f64 = values
                        .iter()
                        .enumerate()
                        .skip(1)
                        .map(|(period, cash)| {
                            -(period as f64) * cash / (1.0 + rate).powi(period as i32 + 1)
                        })
                        .sum();
                    if derivative.abs() < 1e-12 {
                        break;
                    }
                    let next = rate - value / derivative;
                    if (next - rate).abs() < 1e-10 {
                        return finite_number(current, next);
                    }
                    rate = next;
                }
                Err(FormulaCalculationError::TypeMismatch {
                    cell: current.clone(),
                    operation: "IRR 未收敛".into(),
                })
            }
            "SUM" => Ok(CalculatedValue::Number(
                self.collect_numeric(current, args)?.iter().sum(),
            )),
            "AVG" | "AVERAGE" => {
                let values = self.collect_numeric(current, args)?;
                if values.is_empty() {
                    Ok(CalculatedValue::Number(0.0))
                } else {
                    Ok(CalculatedValue::Number(
                        values.iter().sum::<f64>() / values.len() as f64,
                    ))
                }
            }
            "MIN" => {
                let values = self.collect_numeric(current, args)?;
                Ok(CalculatedValue::Number(
                    values.into_iter().min_by(f64::total_cmp).unwrap_or(0.0),
                ))
            }
            "MAX" => {
                let values = self.collect_numeric(current, args)?;
                Ok(CalculatedValue::Number(
                    values.into_iter().max_by(f64::total_cmp).unwrap_or(0.0),
                ))
            }
            "COUNT" => Ok(CalculatedValue::Number(
                self.collect_numeric(current, args)?.len() as f64,
            )),
            "IF" => {
                if args.len() != 3 {
                    return Err(FormulaCalculationError::Syntax {
                        cell: current.clone(),
                        message: "IF 需要三个参数".into(),
                    });
                }
                let condition = self.evaluate_expression(current, &args[0])?;
                let truthy = is_truthy(current, condition)?;
                let branch = if truthy { &args[1] } else { &args[2] };
                self.evaluate_expression(current, branch)
            }
            "IFERROR" => match self.evaluate_expression(current, first_arg(args)?) {
                Ok(value) => Ok(value),
                Err(_) => self.evaluate_expression(current, second_arg(args)?),
            },
            other => Err(FormulaCalculationError::UnsupportedFunction {
                cell: current.clone(),
                function: other.into(),
            }),
        }
    }

    fn lookup_horizontal(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        let needle = self.evaluate_expression(current, first_arg(args)?)?;
        let Expr::Range { start, end } = second_arg(args)? else {
            return Err(FormulaCalculationError::UnsupportedRange {
                cell: current.clone(),
            });
        };
        let start = self.resolve_coordinate(current, start)?;
        let end = self.resolve_coordinate(current, end)?;
        if start.sheet_id != end.sheet_id {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: "跨工作表 HLOOKUP".into(),
            });
        }
        let row_index = self.single_number(current, args, 2)? as u32;
        let height = start.row.abs_diff(end.row) + 1;
        if row_index == 0 || row_index > height {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: "HLOOKUP 行序号超出表高".into(),
            });
        }
        let top = start.row.min(end.row);
        for column in start.column.min(end.column)..=start.column.max(end.column) {
            let candidate = self.value_at(&CellAddress {
                sheet_id: start.sheet_id.clone(),
                row: top,
                column,
            })?;
            if values_equal_loose(&candidate, &needle) {
                return self.value_at(&CellAddress {
                    sheet_id: start.sheet_id,
                    row: top + row_index - 1,
                    column,
                });
            }
        }
        Err(FormulaCalculationError::TypeMismatch {
            cell: current.clone(),
            operation: "HLOOKUP 未找到匹配值".into(),
        })
    }

    fn index_range(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        let Expr::Range { start, end } = first_arg(args)? else {
            return Err(FormulaCalculationError::UnsupportedRange {
                cell: current.clone(),
            });
        };
        let start = self.resolve_coordinate(current, start)?;
        let end = self.resolve_coordinate(current, end)?;
        if start.sheet_id != end.sheet_id {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: "跨工作表 INDEX".into(),
            });
        }
        let row_index = self.single_number(current, args, 1)? as u32;
        let column_index = self.optional_number(current, args, 2, 1.0)? as u32;
        let top = start.row.min(end.row);
        let left = start.column.min(end.column);
        if row_index == 0
            || column_index == 0
            || row_index > start.row.abs_diff(end.row) + 1
            || column_index > start.column.abs_diff(end.column) + 1
        {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: "INDEX 行列序号越界".into(),
            });
        }
        self.value_at(&CellAddress {
            sheet_id: start.sheet_id,
            row: top + row_index - 1,
            column: left + column_index - 1,
        })
    }

    fn evaluate_financial_annuity(
        &mut self,
        current: &CellAddress,
        function: &str,
        args: &[Expr],
    ) -> Result<CalculatedValue, FormulaCalculationError> {
        let rate = self.single_number(current, args, 0)?;
        let periods = self.single_number(current, args, 1)?;
        if periods <= 0.0 || rate <= -1.0 {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: format!("{function} 参数越界"),
            });
        }
        let third = self.single_number(current, args, 2)?;
        let fourth = self.optional_number(current, args, 3, 0.0)?;
        let timing = self.optional_number(current, args, 4, 0.0)?;
        if timing != 0.0 && timing != 1.0 {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: format!("{function} type 只能为 0 或 1"),
            });
        }
        let growth = (1.0 + rate).powf(periods);
        let factor = if rate.abs() < 1e-12 {
            periods
        } else {
            (growth - 1.0) / rate
        };
        let result = match function {
            "PV" => -(fourth + third * (1.0 + rate * timing) * factor) / growth,
            "FV" => -(fourth * growth + third * (1.0 + rate * timing) * factor),
            "PMT" => -(fourth + third * growth) / ((1.0 + rate * timing) * factor),
            _ => unreachable!("financial dispatcher uses a closed function set"),
        };
        finite_number(current, result)
    }

    /// 求第 `index` 个参数的数值（Excel 数字语义）。
    fn single_number(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
        index: usize,
    ) -> Result<f64, FormulaCalculationError> {
        let Some(expression) = args.get(index) else {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: format!("缺少第 {} 个参数", index + 1),
            });
        };
        let value = self.evaluate_expression(current, expression)?;
        number_value(current, value, "数值参数")
    }

    fn optional_number(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
        index: usize,
        default: f64,
    ) -> Result<f64, FormulaCalculationError> {
        match args.get(index) {
            Some(expression) => number_value(
                current,
                self.evaluate_expression(current, expression)?,
                "数值参数",
            ),
            None => Ok(default),
        }
    }

    /// 求第 `index` 个参数的文本。
    fn text_arg(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
        index: usize,
    ) -> Result<String, FormulaCalculationError> {
        let Some(expression) = args.get(index) else {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: current.clone(),
                operation: format!("缺少第 {} 个参数", index + 1),
            });
        };
        let value = self.evaluate_expression(current, expression)?;
        Ok(as_text(&value))
    }

    /// 平铺收集全部参数的值（范围展开成单值序列）。
    fn collect_all(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
    ) -> Result<Vec<CalculatedValue>, FormulaCalculationError> {
        let mut out = Vec::new();
        for arg in args {
            out.extend(self.evaluate_values(current, arg)?);
        }
        Ok(out)
    }

    /// Collects the numeric values of an argument list, ignoring blanks and text
    /// the same way an Excel range aggregate does.
    fn collect_numeric(
        &mut self,
        current: &CellAddress,
        args: &[Expr],
    ) -> Result<Vec<f64>, FormulaCalculationError> {
        let mut out = Vec::new();
        for arg in args {
            for value in self.evaluate_values(current, arg)? {
                if let Some(number) = range_number(value) {
                    out.push(number);
                }
            }
        }
        Ok(out)
    }

    /// Resolves a reference to coordinates without requiring the cell to exist,
    /// which is required for range endpoints that may be empty.
    fn resolve_coordinate(
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
        Ok(CellAddress {
            sheet_id,
            row: reference.row,
            column: reference.column,
        })
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
        if self.cell_at(&address).is_none() {
            return Err(FormulaCalculationError::UnknownReference {
                cell: current.clone(),
                reference: address,
            });
        }
        Ok(address)
    }
}

/// Excel 1900 日期系统在 serial 60 保留虚构的 1900-02-29。
const EXCEL_EPOCH_DAYS: i64 = -25_569; // days_from_civil(1899, 12, 30) 相对 1970-01-01

/// Howard Hinnant 的 civil_from_days / days_from_civil 算法（公历日数互转）。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Excel serial（含小数天 = 时间）。
fn serial_now(date_system: DateSystem) -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs();
    let days = (seconds as i64 / 86_400 - EXCEL_EPOCH_DAYS) as f64;
    let fraction =
        ((seconds % 86_400) as f64 + f64::from(now.subsec_nanos()) / 1_000_000_000.0) / 86_400.0;
    days + fraction
        - if date_system == DateSystem::Excel1904 {
            1_462.0
        } else {
            0.0
        }
}

fn serial_from_ymd_system(year: i64, month: i64, day: i64, date_system: DateSystem) -> f64 {
    serial_from_ymd(year, month, day)
        - if date_system == DateSystem::Excel1904 {
            1_462.0
        } else {
            0.0
        }
}

fn ymd_from_serial_system(serial: f64, date_system: DateSystem) -> (i64, i64, i64) {
    ymd_from_serial(
        serial
            + if date_system == DateSystem::Excel1904 {
                1_462.0
            } else {
                0.0
            },
    )
}

fn serial_from_ymd(year: i64, month: i64, day: i64) -> f64 {
    if (year, month, day) == (1900, 2, 29) {
        return 60.0;
    }
    let days = days_from_civil(year, month, day) - EXCEL_EPOCH_DAYS;
    (if days <= 60 { days - 1 } else { days }) as f64
}

fn ymd_from_serial(serial: f64) -> (i64, i64, i64) {
    let days = serial.floor() as i64;
    if days == 60 {
        return (1900, 2, 29);
    }
    civil_from_days(EXCEL_EPOCH_DAYS + days + i64::from(days < 60))
}

/// 数值文本互认：Excel 的 "5" 参与数值比较。
fn as_number(value: &CalculatedValue) -> Option<f64> {
    match value {
        CalculatedValue::Number(number) => Some(*number),
        CalculatedValue::Blank => Some(0.0),
        CalculatedValue::Text(text) => text.trim().parse::<f64>().ok(),
        CalculatedValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        CalculatedValue::Error { .. } => None,
    }
}

fn as_text(value: &CalculatedValue) -> String {
    match value {
        CalculatedValue::Blank => String::new(),
        CalculatedValue::Number(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                format!("{}", *number as i64)
            } else {
                format!("{number}")
            }
        }
        CalculatedValue::Text(text) => text.clone(),
        CalculatedValue::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.into(),
        CalculatedValue::Error { code, .. } => format!("#{code:?}"),
    }
}

/// COUNTIF/SUMIF 条件匹配器：支持 "">=5""/""<>x""/""文本"" 与裸值。
fn matches_criteria(value: &CalculatedValue, criteria: &CalculatedValue) -> bool {
    let text = as_text(criteria);
    let (operator, operand) = if let Some(stripped) = text.strip_prefix(">=") {
        (">=", stripped)
    } else if let Some(stripped) = text.strip_prefix("<=") {
        ("<=", stripped)
    } else if let Some(stripped) = text.strip_prefix("<>") {
        ("<>", stripped)
    } else if let Some(stripped) = text.strip_prefix('>') {
        (">", stripped)
    } else if let Some(stripped) = text.strip_prefix('<') {
        ("<", stripped)
    } else if let Some(stripped) = text.strip_prefix('=') {
        ("=", stripped)
    } else {
        ("=", text.as_str())
    };
    let operand_number = operand.trim().parse::<f64>().ok();
    let value_number = as_number(value);
    match operator {
        "=" => match (operand_number, value_number) {
            (Some(expected), Some(actual)) => (actual - expected).abs() < 1e-9,
            _ => as_text(value).eq_ignore_ascii_case(operand),
        },
        "<>" => match (operand_number, value_number) {
            (Some(expected), Some(actual)) => (actual - expected).abs() >= 1e-9,
            _ => !as_text(value).eq_ignore_ascii_case(operand),
        },
        _ => {
            let (Some(expected), Some(actual)) = (operand_number, value_number) else {
                return false;
            };
            match operator {
                ">" => actual > expected,
                ">=" => actual >= expected,
                "<" => actual < expected,
                "<=" => actual <= expected,
                _ => false,
            }
        }
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

/// Coerces a value into a numeric cell for range functions; `None` for a blank
/// or text cell so range aggregates skip them like Excel does.
fn range_number(value: CalculatedValue) -> Option<f64> {
    match value {
        CalculatedValue::Number(value) => Some(value),
        CalculatedValue::Bool(value) => Some(if value { 1.0 } else { 0.0 }),
        CalculatedValue::Blank | CalculatedValue::Text(_) | CalculatedValue::Error { .. } => None,
    }
}

fn first_arg(args: &[Expr]) -> Result<&Expr, FormulaCalculationError> {
    args.first().ok_or(FormulaCalculationError::TypeMismatch {
        cell: CellAddress {
            sheet_id: String::new(),
            row: 0,
            column: 0,
        },
        operation: "缺少参数".into(),
    })
}

fn second_arg(args: &[Expr]) -> Result<&Expr, FormulaCalculationError> {
    args.get(1).ok_or(FormulaCalculationError::TypeMismatch {
        cell: CellAddress {
            sheet_id: String::new(),
            row: 0,
            column: 0,
        },
        operation: "缺少第二个参数".into(),
    })
}

/// 宽松布尔判定（AND/OR 用）：Blank=false、Number!=0、Text 非空、Bool 本身。
fn is_truthy_value(value: &CalculatedValue) -> bool {
    match value {
        CalculatedValue::Blank => false,
        CalculatedValue::Number(number) => *number != 0.0,
        CalculatedValue::Bool(flag) => *flag,
        CalculatedValue::Text(text) => !text.is_empty(),
        CalculatedValue::Error { .. } => false,
    }
}

/// VLOOKUP 匹配：数字按值、文本按大小写不敏感、Bool 相等。
fn values_equal_loose(left: &CalculatedValue, right: &CalculatedValue) -> bool {
    match (as_number(left), as_number(right)) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-9,
        _ => as_text(left).eq_ignore_ascii_case(&as_text(right)),
    }
}

fn is_truthy(cell: &CellAddress, value: CalculatedValue) -> Result<bool, FormulaCalculationError> {
    match value {
        CalculatedValue::Bool(value) => Ok(value),
        CalculatedValue::Number(value) => Ok(value != 0.0),
        CalculatedValue::Blank => Ok(false),
        CalculatedValue::Text(_) | CalculatedValue::Error { .. } => {
            Err(FormulaCalculationError::TypeMismatch {
                cell: cell.clone(),
                operation: "IF 条件".into(),
            })
        }
    }
}

fn compare_values(
    cell: &CellAddress,
    left: CalculatedValue,
    right: CalculatedValue,
    operator: BinOp,
) -> Result<bool, FormulaCalculationError> {
    let ordering = match (left, right) {
        (CalculatedValue::Number(a), CalculatedValue::Number(b)) => a.partial_cmp(&b),
        (CalculatedValue::Text(a), CalculatedValue::Text(b)) => Some(a.cmp(&b)),
        (CalculatedValue::Bool(a), CalculatedValue::Bool(b)) => Some(a.cmp(&b)),
        (CalculatedValue::Blank, CalculatedValue::Blank) => Some(std::cmp::Ordering::Equal),
        (CalculatedValue::Blank, CalculatedValue::Number(_))
        | (CalculatedValue::Number(_), CalculatedValue::Blank)
        | (CalculatedValue::Blank, CalculatedValue::Text(_))
        | (CalculatedValue::Text(_), CalculatedValue::Blank)
        | (CalculatedValue::Blank, CalculatedValue::Bool(_))
        | (CalculatedValue::Bool(_), CalculatedValue::Blank) => {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: cell.clone(),
                operation: "比较".into(),
            })
        }
        _ => {
            return Err(FormulaCalculationError::TypeMismatch {
                cell: cell.clone(),
                operation: "比较".into(),
            })
        }
    };
    let ordering =
        ordering.ok_or_else(|| FormulaCalculationError::NonFinite { cell: cell.clone() })?;
    Ok(match operator {
        BinOp::Eq => ordering == std::cmp::Ordering::Equal,
        BinOp::Ne => ordering != std::cmp::Ordering::Equal,
        BinOp::Gt => ordering == std::cmp::Ordering::Greater,
        BinOp::Lt => ordering == std::cmp::Ordering::Less,
        BinOp::Ge => ordering != std::cmp::Ordering::Less,
        BinOp::Le => ordering != std::cmp::Ordering::Greater,
        _ => unreachable!("comparison operator"),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Text(String),
    Reference(Reference),
    Range {
        start: Reference,
        end: Reference,
    },
    Function {
        name: String,
        args: Vec<Expr>,
    },
    Unary {
        operator: char,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: BinOp,
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
    Comma,
    Colon,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
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
        let expression = self.parse_comparison(cell)?;
        match self.peek() {
            Token::End => Ok(expression),
            Token::Colon => Err(FormulaCalculationError::UnsupportedRange { cell: cell.clone() }),
            Token::ExternalWorkbook => {
                Err(FormulaCalculationError::UnsupportedExternalWorkbook { cell: cell.clone() })
            }
            token => Err(self.syntax(cell, format!("多余的 token {token:?}"))),
        }
    }

    fn parse_comparison(&mut self, cell: &CellAddress) -> Result<Expr, FormulaCalculationError> {
        let mut expression = self.parse_expression(cell)?;
        loop {
            let operator = match self.peek() {
                Token::Equal => BinOp::Eq,
                Token::NotEqual => BinOp::Ne,
                Token::Greater => BinOp::Gt,
                Token::Less => BinOp::Lt,
                Token::GreaterEqual => BinOp::Ge,
                Token::LessEqual => BinOp::Le,
                _ => break,
            };
            self.next();
            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(self.parse_expression(cell)?),
            };
        }
        Ok(expression)
    }

    fn parse_expression(&mut self, cell: &CellAddress) -> Result<Expr, FormulaCalculationError> {
        let mut expression = self.parse_term(cell)?;
        loop {
            let operator = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
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
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
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
                    self.next();
                    let end = self.parse_reference(cell)?;
                    return Ok(Expr::Range {
                        start: reference,
                        end,
                    });
                }
                Ok(Expr::Reference(reference))
            }
            Token::LeftParen => {
                let expression = self.parse_comparison(cell)?;
                if !matches!(self.next(), Token::RightParen) {
                    return Err(self.syntax(cell, "缺少右括号"));
                }
                Ok(expression)
            }
            Token::Identifier(function) if matches!(self.peek(), Token::LeftParen) => {
                self.next();
                let mut args = Vec::new();
                if !matches!(self.peek(), Token::RightParen) {
                    loop {
                        args.push(self.parse_comparison(cell)?);
                        match self.next() {
                            Token::Comma => continue,
                            Token::RightParen => break,
                            _ => return Err(self.syntax(cell, "函数参数缺少逗号或右括号")),
                        }
                    }
                } else {
                    self.next();
                }
                Ok(Expr::Function {
                    name: function,
                    args,
                })
            }
            Token::Identifier(identifier) => {
                // 布尔字面量（函数参数与比较两侧均可用）。
                match identifier.to_ascii_uppercase().as_str() {
                    "TRUE" => Ok(Expr::Number(1.0)),
                    "FALSE" => Ok(Expr::Number(0.0)),
                    _ => Err(self.syntax(cell, format!("未知标识符 {identifier}"))),
                }
            }
            Token::ExternalWorkbook => {
                Err(FormulaCalculationError::UnsupportedExternalWorkbook { cell: cell.clone() })
            }
            token => Err(self.syntax(cell, format!("意外的 token {token:?}"))),
        }
    }

    fn parse_reference(
        &mut self,
        cell: &CellAddress,
    ) -> Result<Reference, FormulaCalculationError> {
        match self.next() {
            Token::Reference(reference) => Ok(reference),
            token => Err(self.syntax(cell, format!("期望单元格引用，得到 {token:?}"))),
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
            ',' => {
                self.position += 1;
                Token::Comma
            }
            '=' => {
                self.position += 1;
                Token::Equal
            }
            '!' if self.chars.get(self.position + 1) == Some(&'=') => {
                self.position += 2;
                Token::NotEqual
            }
            '>' => {
                self.position += 1;
                if self.chars.get(self.position) == Some(&'=') {
                    self.position += 1;
                    Token::GreaterEqual
                } else {
                    Token::Greater
                }
            }
            '<' => {
                self.position += 1;
                if self.chars.get(self.position) == Some(&'=') {
                    self.position += 1;
                    Token::LessEqual
                } else {
                    Token::Less
                }
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

    fn numeric_model(formulas: &[(u32, u32, &str)]) -> SpreadsheetModel {
        SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: formulas
                    .iter()
                    .map(|(row, column, formula)| CellModel {
                        row: *row,
                        column: *column,
                        formula: Some((*formula).into()),
                        ..CellModel::default()
                    })
                    .collect(),
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        }
    }

    #[test]
    fn sum_avg_min_max_count_evaluate_ranges() {
        // A1 = 10, A2 = 20, A3 = 30 -> aggregates over A1:A3.
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![
                    CellModel {
                        row: 0,
                        column: 0,
                        value: Some(10.into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 1,
                        column: 0,
                        value: Some(20.into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 2,
                        column: 0,
                        value: Some(30.into()),
                        ..CellModel::default()
                    },
                    // A5 holds a formula that aggregates the range.
                    CellModel {
                        row: 4,
                        column: 0,
                        formula: Some("=SUM(A1:A3)".into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 4,
                        column: 1,
                        formula: Some("=AVG(A1:A3)".into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 4,
                        column: 2,
                        formula: Some("=MIN(A1:A3)+MAX(A1:A3)".into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 4,
                        column: 3,
                        formula: Some("=COUNT(A1:A4)".into()),
                        ..CellModel::default()
                    },
                ],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let result = calculate_with_errors(&model).unwrap();
        let sheet: String = "sheet-1".into();
        let value = |column: u32| {
            result
                .value(&CellAddress {
                    sheet_id: sheet.clone(),
                    row: 4,
                    column,
                })
                .cloned()
        };
        assert_eq!(value(0), Some(CalculatedValue::Number(60.0)), "SUM");
        assert_eq!(value(1), Some(CalculatedValue::Number(20.0)), "AVG");
        assert_eq!(value(2), Some(CalculatedValue::Number(40.0)), "MIN+MAX");
        // COUNT(A1:A4) counts 3 numeric cells; A4 is blank so it is skipped.
        assert_eq!(value(3), Some(CalculatedValue::Number(3.0)), "COUNT");
    }

    #[test]
    fn if_with_comparison_returns_the_correct_branch() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![
                    CellModel {
                        row: 0,
                        column: 0,
                        value: Some(42.into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 0,
                        column: 1,
                        formula: Some("=IF(A1>10, \"big\", \"small\")".into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 0,
                        column: 2,
                        formula: Some("=IF(A1<5, 1, 2)".into()),
                        ..CellModel::default()
                    },
                ],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let result = calculate_with_errors(&model).unwrap();
        let sheet: String = "sheet-1".into();
        assert_eq!(
            result.value(&CellAddress {
                sheet_id: sheet.clone(),
                row: 0,
                column: 1
            }),
            Some(&CalculatedValue::Text("big".into()))
        );
        assert_eq!(
            result.value(&CellAddress {
                sheet_id: sheet.clone(),
                row: 0,
                column: 2
            }),
            Some(&CalculatedValue::Number(2.0))
        );
    }

    #[test]
    fn unsupported_function_is_a_typed_error_not_a_crash() {
        let model = numeric_model(&[(0, 0, "=UNKNOWNFN(1,2)")]);
        let result = calculate_with_errors(&model).unwrap();
        assert!(matches!(
            result.values.values().next(),
            Some(CalculatedValue::Error {
                code: FormulaErrorCode::UnsupportedFunction,
                ..
            })
        ));
    }

    fn eval_one(formula: &str, cells: &[(u32, u32, &str)]) -> CalculatedValue {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: cells
                    .iter()
                    .map(|(row, column, text)| {
                        if let Ok(number) = text.parse::<f64>() {
                            CellModel {
                                row: *row,
                                column: *column,
                                value: Some(number.into()),
                                ..CellModel::default()
                            }
                        } else {
                            CellModel {
                                row: *row,
                                column: *column,
                                value: Some(text.to_string().into()),
                                ..CellModel::default()
                            }
                        }
                    })
                    .collect(),
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let with_formula = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: {
                    let mut all = model.sheets[0].cells.clone();
                    all.push(CellModel {
                        row: 99,
                        column: 99,
                        formula: Some(formula.to_string()),
                        ..CellModel::default()
                    });
                    all
                },
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let result = calculate_with_errors(&with_formula).unwrap();
        result
            .value(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 99,
                column: 99,
            })
            .cloned()
            .expect("公式格必须被求值")
    }

    #[test]
    fn math_functions_round_abs_int_mod_power_sqrt_ceiling_floor() {
        assert_eq!(
            eval_one("=ROUND(2.345, 2)", &[]),
            CalculatedValue::Number(2.35)
        );
        assert_eq!(
            eval_one("=ROUNDUP(2.1, 0)", &[]),
            CalculatedValue::Number(3.0)
        );
        assert_eq!(
            eval_one("=ROUNDDOWN(2.9, 0)", &[]),
            CalculatedValue::Number(2.0)
        );
        assert_eq!(eval_one("=ABS(-7)", &[]), CalculatedValue::Number(7.0));
        assert_eq!(eval_one("=INT(2.9)", &[]), CalculatedValue::Number(2.0));
        assert_eq!(eval_one("=INT(-2.1)", &[]), CalculatedValue::Number(-3.0));
        assert_eq!(eval_one("=MOD(7, 3)", &[]), CalculatedValue::Number(1.0));
        assert_eq!(
            eval_one("=POWER(2, 10)", &[]),
            CalculatedValue::Number(1024.0)
        );
        assert_eq!(eval_one("=SQRT(81)", &[]), CalculatedValue::Number(9.0));
        assert_eq!(eval_one("=CEILING(2.1)", &[]), CalculatedValue::Number(3.0));
        assert_eq!(eval_one("=FLOOR(2.9)", &[]), CalculatedValue::Number(2.0));
    }

    #[test]
    fn logic_and_text_functions() {
        assert_eq!(
            eval_one("=AND(TRUE, 1, \"x\")", &[]),
            CalculatedValue::Bool(true)
        );
        assert_eq!(eval_one("=AND(TRUE, 0)", &[]), CalculatedValue::Bool(false));
        assert_eq!(eval_one("=OR(FALSE, 0)", &[]), CalculatedValue::Bool(false));
        assert_eq!(eval_one("=OR(FALSE, 3)", &[]), CalculatedValue::Bool(true));
        assert_eq!(eval_one("=NOT(FALSE)", &[]), CalculatedValue::Bool(true));

        assert_eq!(
            eval_one("=CONCAT(\"a\", 1, \"b\")", &[]),
            CalculatedValue::Text("a1b".into())
        );
        assert_eq!(
            eval_one("=LEFT(\"hello\", 2)", &[]),
            CalculatedValue::Text("he".into())
        );
        assert_eq!(
            eval_one("=RIGHT(\"hello\", 2)", &[]),
            CalculatedValue::Text("lo".into())
        );
        assert_eq!(
            eval_one("=MID(\"hello\", 2, 3)", &[]),
            CalculatedValue::Text("ell".into())
        );
        assert_eq!(
            eval_one("=LEN(\"你好\")", &[]),
            CalculatedValue::Number(2.0)
        );
        assert_eq!(
            eval_one("=UPPER(\"abc\")", &[]),
            CalculatedValue::Text("ABC".into())
        );
        assert_eq!(
            eval_one("=LOWER(\"ABC\")", &[]),
            CalculatedValue::Text("abc".into())
        );
        assert_eq!(
            eval_one("=TRIM(\"  a   b  \")", &[]),
            CalculatedValue::Text("a b".into())
        );
        assert_eq!(
            eval_one("=SUBSTITUTE(\"aaa\", \"a\", \"b\")", &[]),
            CalculatedValue::Text("bbb".into())
        );
        assert_eq!(
            eval_one("=REPLACE(\"abcdef\", 2, 3, \"XY\")", &[]),
            CalculatedValue::Text("aXYef".into())
        );
        assert_eq!(
            eval_one("=FIND(\"b\", \"abcb\")", &[]),
            CalculatedValue::Number(2.0)
        );
    }

    #[test]
    fn statistics_functions_counta_countif_sumif_median() {
        let cells = vec![
            (0u32, 0u32, "10"),
            (1, 0, "20"),
            (2, 0, "30"),
            (3, 0, "text"),
        ];
        assert_eq!(
            eval_one("=COUNTA(A1:A4)", &cells),
            CalculatedValue::Number(4.0)
        );
        assert_eq!(
            eval_one("=COUNTIF(A1:A4, \">=20\")", &cells),
            CalculatedValue::Number(2.0)
        );
        assert_eq!(
            eval_one("=COUNTIF(A1:A4, \"text\")", &cells),
            CalculatedValue::Number(1.0)
        );
        assert_eq!(
            eval_one("=SUMIF(A1:A4, \">10\")", &cells),
            CalculatedValue::Number(50.0)
        );
        assert_eq!(
            eval_one("=MEDIAN(A1:A3)", &cells),
            CalculatedValue::Number(20.0)
        );
        assert_eq!(
            eval_one("=MEDIAN(A1:A2)", &cells),
            CalculatedValue::Number(15.0)
        );
    }

    #[test]
    fn excel_serial_dates_preserve_the_1900_leap_day_boundary() {
        for (serial, date) in [
            (1.0, (1900, 1, 1)),
            (59.0, (1900, 2, 28)),
            (60.0, (1900, 2, 29)),
            (61.0, (1900, 3, 1)),
            (25569.0, (1970, 1, 1)),
            (46267.0, (2026, 9, 2)),
        ] {
            assert_eq!(ymd_from_serial(serial), date);
            assert_eq!(serial_from_ymd(date.0, date.1, date.2), serial);
        }
        assert_eq!(eval_one("=DAY(60)", &[]), CalculatedValue::Number(29.0));
        assert_eq!(eval_one("=MONTH(60)", &[]), CalculatedValue::Number(2.0));
    }

    #[test]
    fn date_functions_use_excel_1900_serial() {
        assert_eq!(
            eval_one("=DATE(1900, 1, 1)", &[]),
            CalculatedValue::Number(1.0)
        );
        assert_eq!(
            eval_one("=YEAR(DATE(2026, 9, 2))", &[]),
            CalculatedValue::Number(2026.0)
        );
        assert_eq!(
            eval_one("=MONTH(DATE(2026, 9, 2))", &[]),
            CalculatedValue::Number(9.0)
        );
        assert_eq!(
            eval_one("=DAY(DATE(2026, 9, 2))", &[]),
            CalculatedValue::Number(2.0)
        );
        match eval_one("=TODAY()", &[]) {
            CalculatedValue::Number(value) => assert!(value > 45_000.0 && value == value.floor()),
            other => panic!("TODAY 应返回数字：{other:?}"),
        }
    }

    #[test]
    fn vlookup_finds_exact_match_by_row() {
        let cells = vec![
            (0u32, 0u32, "apple"),
            (0, 1, "10"),
            (1, 0, "banana"),
            (1, 1, "20"),
            (2, 0, "cherry"),
            (2, 1, "30"),
        ];
        assert_eq!(
            eval_one("=VLOOKUP(\"banana\", A1:B3, 2)", &cells),
            CalculatedValue::Number(20.0)
        );
        assert_eq!(
            eval_one("=VLOOKUP(\"BANANA\", A1:B3, 2)", &cells),
            CalculatedValue::Number(20.0)
        );
        let numeric = vec![(0u32, 0u32, "1"), (0, 1, "one"), (1, 0, "2"), (1, 1, "two")];
        assert_eq!(
            eval_one("=VLOOKUP(2, A1:B2, 2)", &numeric),
            CalculatedValue::Text("two".into())
        );
        assert!(matches!(
            eval_one("=VLOOKUP(\"missing\", A1:B3, 2)", &cells),
            CalculatedValue::Error { .. }
        ));
        assert!(matches!(
            eval_one("=VLOOKUP(\"apple\", A1:B3, 3)", &cells),
            CalculatedValue::Error { .. }
        ));
    }

    #[test]
    fn lookup_text_statistics_and_financial_functions_cover_the_matrix() {
        let cells = vec![
            (0u32, 0u32, "name"),
            (0, 1, "apple"),
            (0, 2, "banana"),
            (1, 0, "value"),
            (1, 1, "10"),
            (1, 2, "20"),
            (2, 0, "-100"),
            (3, 0, "60"),
            (4, 0, "60"),
        ];
        assert_eq!(
            eval_one("=HLOOKUP(\"banana\", A1:C2, 2)", &cells),
            CalculatedValue::Number(20.0)
        );
        assert_eq!(
            eval_one("=MATCH(\"apple\", B1:C1, 0)", &cells),
            CalculatedValue::Number(1.0)
        );
        assert_eq!(
            eval_one("=INDEX(B1:C2, 2, 2)", &cells),
            CalculatedValue::Number(20.0)
        );
        assert_eq!(
            eval_one("=XLOOKUP(\"banana\", B1:C1, B2:C2)", &cells),
            CalculatedValue::Number(20.0)
        );
        assert_eq!(
            eval_one("=SEARCH(\"好\", \"你好吗\")", &[]),
            CalculatedValue::Number(2.0)
        );
        assert_eq!(
            eval_one("=EXACT(\"A\", \"a\")", &[]),
            CalculatedValue::Bool(false)
        );
        assert_eq!(
            eval_one("=TEXTJOIN(\"-\", TRUE, \"a\", \"\", \"b\")", &[]),
            CalculatedValue::Text("a-b".into())
        );
        assert_eq!(
            eval_one("=AVERAGEIF(B2:C2, \">10\")", &cells),
            CalculatedValue::Number(20.0)
        );
        assert_eq!(
            eval_one("=SMALL(B2:C2, 1)", &cells),
            CalculatedValue::Number(10.0)
        );
        assert_eq!(
            eval_one("=LARGE(B2:C2, 1)", &cells),
            CalculatedValue::Number(20.0)
        );
        match eval_one("=IRR(A3:A5)", &cells) {
            CalculatedValue::Number(value) => assert!((value - 0.13066).abs() < 0.001),
            other => panic!("IRR 应返回数字：{other:?}"),
        }
        match eval_one("=PMT(0.01, 12, 1000)", &[]) {
            CalculatedValue::Number(value) => assert!((value + 88.8488).abs() < 0.001),
            other => panic!("PMT 应返回数字：{other:?}"),
        }
    }

    #[test]
    fn function_catalog_date_system_and_error_propagation_are_explicit() {
        let catalog = spreadsheet_function_catalog();
        let names: std::collections::HashSet<_> =
            catalog.iter().map(|function| function.name).collect();
        assert_eq!(names.len(), catalog.len());
        assert!(catalog
            .iter()
            .find(|function| function.name == "NOW")
            .is_some_and(|function| function.volatile));
        assert!(catalog
            .iter()
            .find(|function| function.name == "PMT")
            .is_some_and(|function| function.category == SpreadsheetFunctionCategory::Financial));
        assert_eq!(
            eval_one("=IFERROR(UNKNOWNFN(), 42)", &[]),
            CalculatedValue::Number(42.0)
        );
        assert!(matches!(
            eval_one("=ROUND(1, UNKNOWNFN())", &[]),
            CalculatedValue::Error {
                code: FormulaErrorCode::UnsupportedFunction,
                ..
            }
        ));

        let model = SpreadsheetModel {
            metadata: oo_schema::SpreadsheetMetadata {
                date_system: DateSystem::Excel1904,
                ..Default::default()
            },
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![CellModel {
                    row: 0,
                    column: 0,
                    formula: Some("=DATE(1904,1,1)".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let result = calculate_with_errors(&model).unwrap();
        assert_eq!(
            result.values.values().next(),
            Some(&CalculatedValue::Number(0.0))
        );
        assert_eq!(
            ymd_from_serial_system(0.0, DateSystem::Excel1904),
            (1904, 1, 1)
        );
    }

    #[test]
    fn target_evaluation_pulls_only_transitive_inputs() {
        // A1=1 (constant), B1 reads A1, C1 is an unrelated formula far away.
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
                        formula: Some("=A1 + 1".into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 999,
                        column: 999,
                        formula: Some("=1 + 1".into()),
                        ..CellModel::default()
                    },
                ],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let sheet: String = "sheet-1".into();
        let targets = vec![CellAddress {
            sheet_id: sheet.clone(),
            row: 0,
            column: 1,
        }];
        let result = calculate_targets_with_errors(&model, &targets).unwrap();
        // The result is exactly the requested targets; transitive inputs are
        // consumed by the DFS and memoized internally, not surfaced.
        assert_eq!(result.values.len(), 1);
        assert_eq!(
            result.value(&CellAddress {
                sheet_id: sheet.clone(),
                row: 0,
                column: 1
            }),
            Some(&CalculatedValue::Number(2.0))
        );
        // The unrelated formula was never evaluated.
        assert_eq!(
            result.value(&CellAddress {
                sheet_id: sheet.clone(),
                row: 999,
                column: 999
            }),
            None
        );
    }

    #[test]
    fn target_evaluation_reports_cycles_as_error_values() {
        let model = numeric_model(&[(0, 0, "=B1"), (0, 1, "=A1")]);
        let sheet: String = "sheet-1".into();
        let targets = vec![CellAddress {
            sheet_id: sheet.clone(),
            row: 0,
            column: 0,
        }];
        let result = calculate_targets_with_errors(&model, &targets).unwrap();
        assert!(matches!(
            result.value(&CellAddress {
                sheet_id: sheet.clone(),
                row: 0,
                column: 0
            }),
            Some(CalculatedValue::Error {
                code: FormulaErrorCode::Cycle,
                ..
            })
        ));
        // Only the requested root is reported; the cycle partner stays unevaluated.
        assert_eq!(result.values.len(), 1);
    }
}
