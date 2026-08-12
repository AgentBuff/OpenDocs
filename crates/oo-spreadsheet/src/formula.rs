//! Spreadsheet 公式依赖图的纯查询派生索引。
//!
//! 该模块只读取 `SpreadsheetModel` 中的公式，解析常见 A1 引用并建立反向依赖关系；它
//! 不把计算结果写回 cell，也不属于 SpreadsheetEngine 的持久化状态。未来公式求值器、
//! 虚拟 viewport 和 XLSX adapter 都可以消费这个索引，而不需要在 UI 层重新扫描公式。

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use oo_schema::{ArtifactEnvelope, ArtifactPayload, SchemaValidationError, SpreadsheetModel};

use super::CellAddress;

const MAX_RANGE_CELLS: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaDependencyIndex {
    /// formula cell -> cells read by that formula.
    dependencies: BTreeMap<CellAddress, BTreeSet<CellAddress>>,
    /// source cell -> formula cells that read the source.
    dependents: BTreeMap<CellAddress, BTreeSet<CellAddress>>,
}

impl FormulaDependencyIndex {
    /// Builds a derived index from an already persisted Spreadsheet snapshot.
    pub fn from_model(model: &SpreadsheetModel) -> Result<Self, FormulaDependencyError> {
        ArtifactEnvelope::new(
            "spreadsheet-formula-index",
            ArtifactPayload::Spreadsheet(model.clone()),
        )
        .validate()?;

        let sheet_lookup = build_sheet_lookup(model)?;
        let mut dependencies = BTreeMap::new();
        let mut dependents: BTreeMap<CellAddress, BTreeSet<CellAddress>> = BTreeMap::new();

        for sheet in &model.sheets {
            for cell in &sheet.cells {
                let Some(formula) = cell.formula.as_deref() else {
                    continue;
                };
                let formula_address = CellAddress {
                    sheet_id: sheet.id.clone(),
                    row: cell.row,
                    column: cell.column,
                };
                let references = parse_references(formula, &sheet.id, &sheet_lookup)?;
                let references: BTreeSet<_> = references.into_iter().collect();
                for reference in &references {
                    dependents
                        .entry(reference.clone())
                        .or_default()
                        .insert(formula_address.clone());
                }
                // Keep formula cells with no references visible to queries and cycle traversal.
                dependencies.insert(formula_address, references);
            }
        }

        Ok(Self {
            dependencies,
            dependents,
        })
    }

    pub fn formula_cells(&self) -> Vec<CellAddress> {
        self.dependencies.keys().cloned().collect()
    }

    pub fn dependencies_of(&self, formula: &CellAddress) -> Vec<CellAddress> {
        self.dependencies
            .get(formula)
            .into_iter()
            .flat_map(|references| references.iter().cloned())
            .collect()
    }

    pub fn dependents_of(&self, source: &CellAddress) -> Vec<CellAddress> {
        self.dependents
            .get(source)
            .into_iter()
            .flat_map(|dependents| dependents.iter().cloned())
            .collect()
    }

    /// Returns changed cells and the complete reverse-dependent closure as a bounded topology.
    /// `topological_order` is `None` when the affected subgraph contains a cycle.
    pub fn affected_topology(&self, changed: &[CellAddress]) -> FormulaDependencySubgraph {
        let mut nodes: BTreeSet<CellAddress> = changed.iter().cloned().collect();
        let mut queue: VecDeque<_> = nodes.iter().cloned().collect();
        while let Some(source) = queue.pop_front() {
            for dependent in self.dependents_of(&source) {
                if nodes.insert(dependent.clone()) {
                    queue.push_back(dependent);
                }
            }
        }

        let mut edges = Vec::new();
        for source in &nodes {
            for dependent in self.dependents_of(source) {
                if nodes.contains(&dependent) {
                    edges.push(FormulaDependencyEdge {
                        source: source.clone(),
                        dependent,
                    });
                }
            }
        }
        edges.sort();
        edges.dedup();

        let topological_order = topological_order(&nodes, &edges);
        FormulaDependencySubgraph {
            nodes: nodes.into_iter().collect(),
            edges,
            cyclic: topological_order.is_none(),
            topological_order,
        }
    }

    pub fn cycles(&self) -> Vec<Vec<CellAddress>> {
        let mut graph: BTreeMap<CellAddress, BTreeSet<CellAddress>> = BTreeMap::new();
        for (formula, references) in &self.dependencies {
            graph.entry(formula.clone()).or_default();
            for reference in references {
                graph
                    .entry(formula.clone())
                    .or_default()
                    .insert(reference.clone());
                graph.entry(reference.clone()).or_default();
            }
        }

        let mut index = 0;
        let mut indices = BTreeMap::new();
        let mut lowlinks = BTreeMap::new();
        let mut stack = Vec::new();
        let mut on_stack = BTreeSet::new();
        let mut components = Vec::new();
        for node in graph.keys().cloned().collect::<Vec<_>>() {
            if !indices.contains_key(&node) {
                tarjan_visit(
                    node,
                    &graph,
                    &mut index,
                    &mut indices,
                    &mut lowlinks,
                    &mut stack,
                    &mut on_stack,
                    &mut components,
                );
            }
        }

        components
            .into_iter()
            .filter(|component| {
                component.len() > 1
                    || component.first().is_some_and(|node| {
                        graph.get(node).is_some_and(|edges| edges.contains(node))
                    })
            })
            .map(|mut component| {
                component.sort();
                component
            })
            .collect()
    }

    pub fn has_cycle(&self) -> bool {
        !self.cycles().is_empty()
    }

    /// Incrementally updates only changed formula cells and returns the reverse-dependent closure
    /// that needs recalculation. Callers must include all formulas affected by a sheet rename;
    /// ordinary SetCell batches can pass the changed cell addresses directly.
    pub fn update(
        &mut self,
        model: &SpreadsheetModel,
        changed: &[CellAddress],
    ) -> Result<FormulaDependencySubgraph, FormulaDependencyError> {
        let lookup = build_sheet_lookup(model)?;
        let changed_set: BTreeSet<_> = changed.iter().cloned().collect();
        for address in &changed_set {
            if let Some(previous) = self.dependencies.remove(address) {
                for dependency in previous {
                    if let Some(dependents) = self.dependents.get_mut(&dependency) {
                        dependents.remove(address);
                    }
                    if self
                        .dependents
                        .get(&dependency)
                        .is_some_and(BTreeSet::is_empty)
                    {
                        self.dependents.remove(&dependency);
                    }
                }
            }
            self.dependents.remove(address);
        }
        for address in &changed_set {
            let Some(cell) = model
                .sheets
                .iter()
                .find(|sheet| sheet.id == address.sheet_id)
                .and_then(|sheet| {
                    sheet
                        .cells
                        .iter()
                        .find(|cell| cell.row == address.row && cell.column == address.column)
                })
            else {
                continue;
            };
            let Some(formula) = cell.formula.as_deref() else {
                continue;
            };
            let references: BTreeSet<_> = parse_references(formula, &address.sheet_id, &lookup)?
                .into_iter()
                .collect();
            for reference in &references {
                self.dependents
                    .entry(reference.clone())
                    .or_default()
                    .insert(address.clone());
            }
            self.dependencies.insert(address.clone(), references);
        }
        Ok(self.affected_topology(changed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormulaDependencyEdge {
    pub source: CellAddress,
    pub dependent: CellAddress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaDependencySubgraph {
    pub nodes: Vec<CellAddress>,
    pub edges: Vec<FormulaDependencyEdge>,
    pub cyclic: bool,
    pub topological_order: Option<Vec<CellAddress>>,
}

#[derive(Debug, thiserror::Error)]
pub enum FormulaDependencyError {
    #[error("Spreadsheet schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
    #[error("工作表名称存在歧义：{0}")]
    AmbiguousSheetName(String),
    #[error("公式引用范围过大：{0} 个单元格，限制为 {MAX_RANGE_CELLS}")]
    RangeTooLarge(u64),
}

fn build_sheet_lookup(
    model: &SpreadsheetModel,
) -> Result<HashMap<String, String>, FormulaDependencyError> {
    let mut lookup = HashMap::new();
    for sheet in &model.sheets {
        for name in [&sheet.id, &sheet.name] {
            let key = normalize_sheet_name(name);
            if let Some(previous) = lookup.insert(key.clone(), sheet.id.clone()) {
                if previous != sheet.id {
                    return Err(FormulaDependencyError::AmbiguousSheetName(key));
                }
            }
        }
    }
    Ok(lookup)
}

fn normalize_sheet_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn parse_references(
    formula: &str,
    current_sheet: &str,
    sheet_lookup: &HashMap<String, String>,
) -> Result<Vec<CellAddress>, FormulaDependencyError> {
    let chars: Vec<char> = formula.chars().collect();
    let mut references = BTreeSet::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '"' {
            index = skip_string(&chars, index);
            continue;
        }

        let start = index;
        let (sheet_name, cell_start) = parse_sheet_prefix(&chars, index);
        if sheet_name.is_some() {
            index = cell_start;
        }
        let Some((first, mut end)) = parse_cell_ref(&chars, index) else {
            index = start + 1;
            continue;
        };
        if sheet_name.is_none() && has_identifier_before(&chars, index) {
            index = start + 1;
            continue;
        }
        if has_identifier_after(&chars, end) {
            index = start + 1;
            continue;
        }

        let sheet_id = sheet_name
            .as_deref()
            .and_then(|name| sheet_lookup.get(&normalize_sheet_name(name)))
            .cloned()
            .or_else(|| {
                if sheet_name.is_none() {
                    Some(current_sheet.to_string())
                } else {
                    None
                }
            });
        let Some(sheet_id) = sheet_id else {
            index = end;
            continue;
        };

        let mut cells = vec![CellAddress {
            sheet_id: sheet_id.clone(),
            row: first.0,
            column: first.1,
        }];
        if chars.get(end) == Some(&':') {
            if let Some((second, range_end)) = parse_cell_ref(&chars, end + 1) {
                let rows = first.0.abs_diff(second.0) as u64 + 1;
                let columns = first.1.abs_diff(second.1) as u64 + 1;
                let count = rows.saturating_mul(columns);
                if count > MAX_RANGE_CELLS {
                    return Err(FormulaDependencyError::RangeTooLarge(count));
                }
                cells.clear();
                for row in first.0.min(second.0)..=first.0.max(second.0) {
                    for column in first.1.min(second.1)..=first.1.max(second.1) {
                        cells.push(CellAddress {
                            sheet_id: sheet_id.clone(),
                            row,
                            column,
                        });
                    }
                }
                end = range_end;
            }
        }
        references.extend(cells);
        index = end;
    }
    Ok(references.into_iter().collect())
}

fn skip_string(chars: &[char], mut index: usize) -> usize {
    index += 1;
    while index < chars.len() {
        if chars[index] == '"' {
            if chars.get(index + 1) == Some(&'"') {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    index
}

fn parse_sheet_prefix(chars: &[char], index: usize) -> (Option<String>, usize) {
    if chars.get(index) == Some(&'\'') {
        let mut cursor = index + 1;
        let mut name = String::new();
        while cursor < chars.len() {
            if chars[cursor] == '\'' {
                if chars.get(cursor + 1) == Some(&'\'') {
                    name.push('\'');
                    cursor += 2;
                    continue;
                }
                if chars.get(cursor + 1) == Some(&'!') {
                    return (Some(name), cursor + 2);
                }
                return (None, index);
            }
            name.push(chars[cursor]);
            cursor += 1;
        }
        return (None, index);
    }

    let mut cursor = index;
    while cursor < chars.len() && is_sheet_char(chars[cursor]) {
        cursor += 1;
    }
    if cursor > index && chars.get(cursor) == Some(&'!') {
        return (
            Some(chars[index..cursor].iter().collect::<String>()),
            cursor + 1,
        );
    }
    (None, index)
}

fn parse_cell_ref(chars: &[char], index: usize) -> Option<((u32, u32), usize)> {
    let mut cursor = index;
    if chars.get(cursor) == Some(&'$') {
        cursor += 1;
    }
    let column_start = cursor;
    while chars
        .get(cursor)
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        cursor += 1;
    }
    let column_end = cursor;
    if column_end == column_start || column_end - column_start > 4 {
        return None;
    }
    if chars.get(cursor) == Some(&'$') {
        cursor += 1;
    }
    let row_start = cursor;
    while chars
        .get(cursor)
        .is_some_and(|character| character.is_ascii_digit())
    {
        cursor += 1;
    }
    if row_start == cursor {
        return None;
    }
    let row = chars[row_start..cursor]
        .iter()
        .collect::<String>()
        .parse::<u32>()
        .ok()?
        .checked_sub(1)?;
    let mut column = 0u32;
    for character in &chars[column_start..column_end] {
        let character = character.to_ascii_uppercase();
        column = column
            .checked_mul(26)?
            .checked_add((character as u8 - b'A' + 1) as u32)?;
    }
    Some(((row, column.checked_sub(1)?), cursor))
}

fn is_sheet_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-' | ' ')
}

fn has_identifier_before(chars: &[char], index: usize) -> bool {
    index > 0 && (chars[index - 1].is_ascii_alphanumeric() || chars[index - 1] == '_')
}

fn has_identifier_after(chars: &[char], index: usize) -> bool {
    chars
        .get(index)
        .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
}

fn topological_order(
    nodes: &BTreeSet<CellAddress>,
    edges: &[FormulaDependencyEdge],
) -> Option<Vec<CellAddress>> {
    let mut outgoing: BTreeMap<CellAddress, BTreeSet<CellAddress>> = BTreeMap::new();
    let mut indegree: BTreeMap<CellAddress, usize> =
        nodes.iter().cloned().map(|node| (node, 0)).collect();
    for edge in edges {
        outgoing
            .entry(edge.source.clone())
            .or_default()
            .insert(edge.dependent.clone());
        *indegree.entry(edge.dependent.clone()).or_default() += 1;
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .filter_map(|(node, degree)| (*degree == 0).then_some(node.clone()))
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(node) = ready.pop_first() {
        order.push(node.clone());
        for dependent in outgoing.get(&node).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("edge node must have indegree");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(dependent.clone());
            }
        }
    }
    (order.len() == nodes.len()).then_some(order)
}

#[allow(clippy::too_many_arguments)]
fn tarjan_visit(
    node: CellAddress,
    graph: &BTreeMap<CellAddress, BTreeSet<CellAddress>>,
    index: &mut usize,
    indices: &mut BTreeMap<CellAddress, usize>,
    lowlinks: &mut BTreeMap<CellAddress, usize>,
    stack: &mut Vec<CellAddress>,
    on_stack: &mut BTreeSet<CellAddress>,
    components: &mut Vec<Vec<CellAddress>>,
) {
    indices.insert(node.clone(), *index);
    lowlinks.insert(node.clone(), *index);
    *index += 1;
    stack.push(node.clone());
    on_stack.insert(node.clone());

    for successor in graph
        .get(&node)
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
    {
        if !indices.contains_key(&successor) {
            tarjan_visit(
                successor.clone(),
                graph,
                index,
                indices,
                lowlinks,
                stack,
                on_stack,
                components,
            );
            let successor_lowlink = *lowlinks.get(&successor).expect("visited successor");
            let node_lowlink = lowlinks.get_mut(&node).expect("visited node");
            *node_lowlink = (*node_lowlink).min(successor_lowlink);
        } else if on_stack.contains(&successor) {
            let successor_index = *indices.get(&successor).expect("indexed successor");
            let node_lowlink = lowlinks.get_mut(&node).expect("visited node");
            *node_lowlink = (*node_lowlink).min(successor_index);
        }
    }

    if lowlinks.get(&node) == indices.get(&node) {
        let mut component = Vec::new();
        while let Some(member) = stack.pop() {
            on_stack.remove(&member);
            component.push(member.clone());
            if member == node {
                break;
            }
        }
        components.push(component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{CellModel, SheetModel};

    fn cell(row: u32, column: u32, formula: &str) -> CellModel {
        CellModel {
            row,
            column,
            formula: Some(formula.into()),
            style: None,
            ..CellModel::default()
        }
    }

    fn model() -> SpreadsheetModel {
        SpreadsheetModel {
            sheets: vec![
                SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: vec![cell(0, 0, "=B1 + 'Sheet 2'!$C$3"), cell(0, 1, "=A1")],
                    ..SheetModel::default()
                },
                SheetModel {
                    id: "sheet-2".into(),
                    name: "Sheet 2".into(),
                    cells: vec![cell(2, 2, "=SUM('Sheet 1'!A1:B2)")],
                    ..SheetModel::default()
                },
            ],
            ..SpreadsheetModel::default()
        }
    }

    #[test]
    fn parses_absolute_sheet_and_range_references() {
        let index = FormulaDependencyIndex::from_model(&model()).unwrap();
        let formula = CellAddress {
            sheet_id: "sheet-1".into(),
            row: 0,
            column: 0,
        };
        assert_eq!(
            index.dependencies_of(&formula),
            vec![
                CellAddress {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 1
                },
                CellAddress {
                    sheet_id: "sheet-2".into(),
                    row: 2,
                    column: 2
                },
            ]
        );
        let range_formula = CellAddress {
            sheet_id: "sheet-2".into(),
            row: 2,
            column: 2,
        };
        assert_eq!(index.dependencies_of(&range_formula).len(), 4);
    }

    #[test]
    fn parses_lowercase_a1_references() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![cell(1, 1, "=a1")],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let index = FormulaDependencyIndex::from_model(&model).unwrap();
        assert_eq!(
            index.dependencies_of(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 1,
                column: 1,
            }),
            vec![CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 0,
            }]
        );
    }

    #[test]
    fn reverse_closure_returns_bounded_topology_and_order() {
        let index = FormulaDependencyIndex::from_model(&SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![cell(0, 0, "=B1")],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        })
        .unwrap();
        let source = CellAddress {
            sheet_id: "sheet-1".into(),
            row: 0,
            column: 1,
        };
        let topology = index.affected_topology(std::slice::from_ref(&source));
        assert!(!topology.cyclic);
        assert!(topology.nodes.contains(&source));
        assert_eq!(topology.nodes.len(), 2);
        assert_eq!(topology.edges.len(), 1);
        assert!(topology.topological_order.is_some());
    }

    #[test]
    fn cycles_are_detected_without_mutating_model() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![cell(0, 0, "=B1"), cell(0, 1, "=A1")],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let index = FormulaDependencyIndex::from_model(&model).unwrap();
        assert!(index.has_cycle());
        assert_eq!(index.cycles().len(), 1);
        let topology = index.affected_topology(&[CellAddress {
            sheet_id: "sheet-1".into(),
            row: 0,
            column: 0,
        }]);
        assert!(topology.cyclic);
        assert!(topology.topological_order.is_none());
    }

    #[test]
    fn quoted_literals_and_unknown_sheet_are_not_dependencies() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![cell(0, 0, r#"=\"A99\" + Unknown!A1 + A2"#)],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let index = FormulaDependencyIndex::from_model(&model).unwrap();
        assert_eq!(
            index.dependencies_of(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 0
            }),
            vec![CellAddress {
                sheet_id: "sheet-1".into(),
                row: 1,
                column: 0
            }]
        );
    }

    #[test]
    fn oversized_ranges_fail_at_derived_index_boundary() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![cell(0, 0, "=A1:XFD100000")],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        assert!(matches!(
            FormulaDependencyIndex::from_model(&model),
            Err(FormulaDependencyError::RangeTooLarge(_))
        ));
    }

    #[test]
    fn update_replaces_only_changed_formula_edges() {
        let mut model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![cell(0, 0, "=B1"), cell(0, 1, "=C1"), cell(0, 2, "=1")],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let mut index = FormulaDependencyIndex::from_model(&model).unwrap();
        model.sheets[0].cells[0].formula = Some("=C1".into());
        let changed = CellAddress {
            sheet_id: "sheet-1".into(),
            row: 0,
            column: 0,
        };
        let affected = index
            .update(&model, std::slice::from_ref(&changed))
            .unwrap();
        assert!(affected.nodes.contains(&changed));
        assert!(index
            .dependents_of(&CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 1
            })
            .is_empty());
        assert_eq!(
            index.dependencies_of(&changed),
            vec![CellAddress {
                sheet_id: "sheet-1".into(),
                row: 0,
                column: 2
            }]
        );
    }
}
