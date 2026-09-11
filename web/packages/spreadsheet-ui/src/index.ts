/**
 * @open-office/spreadsheet-ui
 *
 * Renderer-neutral Spreadsheet contracts shared by the grid renderer, the
 * Studio shell and future toolbar adapters: typed semantic commands, the
 * structured capability matrix and the pure selection/address models.
 *
 * This package never holds a SpreadsheetModel, an engine handle or snapshot
 * state — those stay with the app shell and the canonical Rust engine. It is
 * the spreadsheet counterpart of `@open-office/presentation-ui`.
 */

export * from "./address";
export * from "./selection";
export * from "./commands";
export * from "./types";
export * from "./capabilities";
export * from "./toolbar";
