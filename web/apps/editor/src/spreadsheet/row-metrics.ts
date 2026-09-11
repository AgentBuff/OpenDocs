import type { CellModel, SheetRowLayout } from "@open-office/schema/artifact";

const DEFAULT_HEIGHT = 28;

/** Sparse view geometry: large fonts grow only their own row, without changing cells. */
export function spreadsheetRowMetrics(total: number, cells: readonly CellModel[], layout: readonly SheetRowLayout[] = []) {
  const heights = new Map<number, number>();
  for (const cell of cells) {
    const points = cell.style?.font?.size;
    if (points == null || !Number.isFinite(points) || cell.row >= total) continue;
    const height = Math.max(DEFAULT_HEIGHT, Math.ceil(points * 4 / 3 * 1.3 + 8));
    if (height > DEFAULT_HEIGHT) heights.set(cell.row, Math.max(heights.get(cell.row) ?? 0, height));
  }
  for (const entry of layout) {
    if (entry.row < total && (entry.hidden || entry.height !== null)) heights.set(entry.row, entry.hidden ? 0 : entry.height! * 4 / 3);
  }
  const rows = [...heights.keys()].sort((a, b) => a - b);
  const extra = [0];
  for (const row of rows) extra.push(extra.at(-1)! + heights.get(row)! - DEFAULT_HEIGHT);
  const top = (row: number) => {
    let low = 0, high = rows.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (rows[mid]! < row) low = mid + 1; else high = mid;
    }
    return row * DEFAULT_HEIGHT + extra[low]!;
  };
  const rowAt = (position: number) => {
    let low = 0, high = total;
    while (low < high) {
      const mid = Math.ceil((low + high) / 2);
      if (top(mid) <= position) low = mid; else high = mid - 1;
    }
    return Math.min(total - 1, Math.max(0, low));
  };
  return { top, rowAt, height: (row: number) => heights.get(row) ?? DEFAULT_HEIGHT };
}
