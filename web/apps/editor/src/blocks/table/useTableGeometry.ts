import { useCallback, useLayoutEffect, useState, type RefObject } from "react";

import type { TableBlock } from "@open-office/schema/artifact";

import { emptyTableGeometry, measureTableGeometry, type TableGeometry } from "./model.js";

/**
 * Measures rendered table geometry for non-persistent affordances. The DOM is
 * observed only as a projection of the canonical grid; no measured value is
 * written into the document model here.
 */
export function useTableGeometry(
  wrapRef: RefObject<HTMLDivElement | null>,
  table: TableBlock | null,
  shouldRemeasure = false,
): { geometry: TableGeometry; measure: () => void } {
  const [geometry, setGeometry] = useState<TableGeometry>(emptyTableGeometry);

  const measure = useCallback(() => {
    const wrap = wrapRef.current;
    const element = wrap?.querySelector<HTMLTableElement>(".block-table");
    if (!wrap || !element || !table) return;
    const wrapRect = wrap.getBoundingClientRect();
    const tableRect = element.getBoundingClientRect();
    const rows = Array.from(element.tBodies[0]?.rows ?? []);
    // A colgroup stays one-to-one with stable column IDs even when a body row
    // contains merged anchors and therefore fewer visible cells.
    const columns = Array.from(element.querySelectorAll<HTMLTableColElement>("col"));
    setGeometry(measureTableGeometry(
      wrapRect,
      tableRect,
      rows.map((row) => row.getBoundingClientRect()),
      columns.map((column) => column.getBoundingClientRect()),
      table.rows.map((row) => row.id),
      table.columns.map((column) => column.id),
    ));
  }, [table, wrapRef]);

  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap || !table) return undefined;
    let frame = 0;
    const run = () => { frame = 0; measure(); };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(run); };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    observer?.observe(wrap);
    window.addEventListener("resize", schedule);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      observer?.disconnect();
      window.removeEventListener("resize", schedule);
    };
  }, [measure, table, wrapRef]);

  useLayoutEffect(() => {
    if (!shouldRemeasure) return undefined;
    const frame = requestAnimationFrame(measure);
    return () => cancelAnimationFrame(frame);
  }, [measure, shouldRemeasure]);

  return { geometry, measure };
}
