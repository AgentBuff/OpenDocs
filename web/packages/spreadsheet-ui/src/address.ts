/**
 * Spreadsheet A1 reference helpers.
 *
 * These are pure, renderer-free conversions between 0-based (row, column)
 * coordinates and the A1 references used in formulas and in the cell editor.
 * They are intentionally small and unit-tested so the grid, formula bar and
 * command builders share one canonical coordinate convention.
 */

/** 0-based column index -> column letters (0 -> "A", 25 -> "Z", 26 -> "AA"). */
export function columnToLetters(column: number): string {
  if (!Number.isSafeInteger(column) || column < 0) {
    throw new Error(`列索引无效：${column}`);
  }
  let value = column;
  let letters = "";
  while (value >= 0) {
    letters = String.fromCharCode((value % 26) + 65) + letters;
    value = Math.floor(value / 26) - 1;
  }
  return letters;
}

/** Column letters -> 0-based column index ("A" -> 0, "AA" -> 26). */
export function lettersToColumn(letters: string): number {
  const upper = letters.trim().toUpperCase();
  if (!/^[A-Z]+$/.test(upper)) {
    throw new Error(`列名无效：${letters}`);
  }
  let column = 0;
  for (const character of upper) {
    column = column * 26 + (character.charCodeAt(0) - 64);
  }
  return column - 1;
}

/** A1 cell label -> 0-based `{ row, column }`. */
export function parseCellRef(reference: string): { row: number; column: number } {
  const match = /^([A-Za-z]+)([0-9]+)$/.exec(reference.trim());
  if (!match) {
    throw new Error(`单元格引用无效：${reference}`);
  }
  return { row: Number(match[2]) - 1, column: lettersToColumn(match[1]) };
}

/** 0-based `{ row, column }` -> A1 cell label (1-based row). */
export function cellRef(row: number, column: number): string {
  return `${columnToLetters(column)}${row + 1}`;
}

/** Half-open inclusive range `{startRow,startColumn,endRow,endColumn}` -> A1 range label. */
export function rangeRef(
  range: { startRow: number; startColumn: number; endRow: number; endColumn: number },
): string {
  const start = cellRef(range.startRow, range.startColumn);
  const end = cellRef(range.endRow, range.endColumn);
  return start === end ? start : `${start}:${end}`;
}

export function parseRangeRef(reference: string): {
  startRow: number;
  startColumn: number;
  endRow: number;
  endColumn: number;
} {
  const [start, end = start] = reference.split(":");
  const startRef = parseCellRef(start);
  const endRef = parseCellRef(end);
  return {
    startRow: Math.min(startRef.row, endRef.row),
    startColumn: Math.min(startRef.column, endRef.column),
    endRow: Math.max(startRef.row, endRef.row),
    endColumn: Math.max(startRef.column, endRef.column),
  };
}

/** Clamp a cell coordinate into an inclusive range. */
export function clampCell(row: number, column: number, maxRow: number, maxColumn: number): { row: number; column: number } {
  return {
    row: Math.max(0, Math.min(maxRow, row)),
    column: Math.max(0, Math.min(maxColumn, column)),
  };
}
