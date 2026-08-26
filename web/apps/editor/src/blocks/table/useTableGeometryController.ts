import type { RefObject } from "react";

import type { TableBlock } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { useTableGeometry } from "./useTableGeometry.js";
import { useTableResize } from "./useTableResize.js";

/**
 * Geometry is an interaction projection, never document state. This behavior
 * boundary joins DOM measurement with transient resize previews while the
 * resize hook remains the only owner of pointer lifecycle and semantic
 * dimension commits.
 */
export function useTableGeometryController({
  blockId,
  table,
  rootRef,
  session,
}: {
  blockId: string;
  table: TableBlock | null;
  rootRef: RefObject<HTMLDivElement | null>;
  session: BlockSessionApi;
}) {
  const resize = useTableResize(blockId, session);
  const { geometry, measure } = useTableGeometry(
    rootRef,
    table,
    Boolean(resize.columnPreview || resize.rowPreview),
  );

  return {
    geometry,
    measure,
    columnPreview: resize.columnPreview,
    rowPreview: resize.rowPreview,
    beginColumnResize: resize.beginColumnResize,
    beginRowResize: resize.beginRowResize,
  };
}
