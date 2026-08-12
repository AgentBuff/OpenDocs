import { createArtifactToolbarAdapter, type ArtifactToolbarContext, type ArtifactToolbarDescriptor, type ArtifactToolbarAdapter } from "./types.js";

export const SPREADSHEET_TOOLBAR_NAMESPACE = "spreadsheet" as const;

/** Stable semantic ids shared by the spreadsheet renderer and API capability projection. */
export const SPREADSHEET_TOOLBAR_CAPABILITY_IDS = Object.freeze([
  "spreadsheet.formatCells",
  "spreadsheet.freezePane",
  "spreadsheet.filter",
  "spreadsheet.sort",
  "spreadsheet.conditionalFormat",
  "spreadsheet.dataValidation",
  "spreadsheet.mergeCells",
  "spreadsheet.insertRows",
  "spreadsheet.insertColumns",
  "spreadsheet.deleteRows",
  "spreadsheet.deleteColumns",
] as const);
export type SpreadsheetToolbarCapabilityId = typeof SPREADSHEET_TOOLBAR_CAPABILITY_IDS[number];

export type SpreadsheetSelection =
  | { kind: "cell"; sheetId: string; row: number; column: number }
  | { kind: "range"; sheetId: string; startRow: number; startColumn: number; endRow: number; endColumn: number }
  | { kind: "sheet"; sheetId: string };
export type SpreadsheetToolbarContext = ArtifactToolbarContext<SpreadsheetSelection>;
export type SpreadsheetToolbarDescriptor<ActionId extends string = string> = ArtifactToolbarDescriptor<ActionId, SpreadsheetToolbarContext>;
export type SpreadsheetToolbarAdapter<ActionId extends string = string> = ArtifactToolbarAdapter<ActionId, SpreadsheetToolbarContext>;

export function createSpreadsheetToolbarAdapter<ActionId extends string = string>(
  descriptors: readonly SpreadsheetToolbarDescriptor<ActionId>[] = [],
): SpreadsheetToolbarAdapter<ActionId> {
  return createArtifactToolbarAdapter(SPREADSHEET_TOOLBAR_NAMESPACE, descriptors);
}
