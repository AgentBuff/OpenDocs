/**
 * Product contract for the contextual table toolbar.
 *
 * Keep this list deliberately smaller than the table command catalog: the
 * floating surface contains frequent formatting actions only. Destructive and
 * clipboard commands belong to the context menu, while mutually-exclusive
 * structure commands share one slot. This prevents future refactors from
 * exposing every engine command as an unexplained icon.
 */
export const TABLE_SELECTION_TOOLBAR_GROUPS = [
  {
    id: "text",
    label: "文字格式",
    actions: [
      "fontIncrease",
      "fontDecrease",
      "bold",
      "textHighlight",
      "textColor",
      "italic",
      "underline",
      "strikethrough",
    ],
  },
  {
    id: "cellAppearance",
    label: "单元格外观",
    actions: ["cellFill", "borders"],
  },
  {
    id: "alignment",
    label: "单元格对齐",
    actions: ["horizontalAlign", "verticalAlign"],
  },
  {
    id: "structure",
    label: "表格结构",
    actions: ["mergeOrSplit", "insertRowColumn"],
  },
] as const;

export type TableToolbarActionId = typeof TABLE_SELECTION_TOOLBAR_GROUPS[number]["actions"][number];
