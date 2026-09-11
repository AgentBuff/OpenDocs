import { FontPicker } from "../typography/FontPicker.js";
import { useId, useState, type ReactNode } from "react";
import type { ComparisonOperator } from "@open-office/schema/artifact";
import { SpreadsheetTableStyles } from "./SpreadsheetTableStyles.js";
import { Popover } from "@open-office/ui";
import {
  NUMBER_FORMAT_PRESETS,
  type SpreadsheetSemanticCommand,
  type TableStyleOptions,
  type TableStylePreset,
  RIBBON_TABS,
  type BorderPreset,
  type RibbonTabId,
  type ToolbarStyleState,
} from "@open-office/spreadsheet-ui";

export type RibbonTab = RibbonTabId | "tableStyle";
export interface SpreadsheetToolbarProps {
  tableStyleContext?: { preset: TableStylePreset; options: TableStyleOptions } | null;
  onContextTableStyle: (preset: TableStylePreset, options: TableStyleOptions) => void;
  onClearTableStyle: () => void;
  onTableStyle: (preset: TableStylePreset, options: TableStyleOptions) => void;
  disabled: boolean; styleState: ToolbarStyleState; selectionActive: boolean; canMergeToggle: boolean; mergeActive: boolean; filterActive: boolean; frozen: boolean; canUndo: boolean; canRedo: boolean; clipboardHasContent: boolean; tab: RibbonTab; onTabChange: (tab: RibbonTab) => void;
  /** 服务端 `/api/capabilities` 发布的 spreadsheet typeId 集合。缺席即不可调度。 */
  availableCapabilities: ReadonlySet<string>;
  onUndo: () => void; onRedo: () => void; onCopy: () => void; onCut: () => void; onPaste: () => void; onClearContents: () => void; onClearFormatting: () => void; onFontFamily: (family: string) => void; onFontSize: (size: number) => void; onBold: () => void; onItalic: () => void; onStrikethrough: () => void; onUnderline: () => void; onFontColor: (color: string) => void; onFillColor: (color: string) => void; onBorderPreset: (preset: BorderPreset) => void; onAlignHorizontal: (value: "left" | "center" | "right") => void; onAlignVertical: (value: "top" | "middle" | "bottom") => void; onWrap: () => void; onMergeToggle: () => void; onMergeCenter: () => void; onNumberFormat: (format: string) => void; onInsertRowAbove: () => void; onInsertRowBelow: () => void; onDeleteRow: () => void; onInsertColumnLeft: () => void; onInsertColumnRight: () => void; onDeleteColumn: () => void; onFindReplace: () => void; onSort: (direction: "ascending" | "descending") => void; onFilterToggle: () => void; onFreezeToggle: () => void;
  /** AutoSum：引擎级 =SUM 插入。 */
  onAutoSum: () => void;
  /** 条件格式：CellIs 规则 upsert（大于给定阈值命中）。 */
  onUpsertConditionalFormat: (rule: { id: string; threshold: number; operator?: ComparisonOperator }) => void;
  /** 删除一条条件格式规则。 */
  onDeleteConditionalFormat: (ruleId: string) => void;
  onClearConditionalFormats: () => void;
  /** 当前 sheet 的条件格式规则投影。 */
  conditionalFormatRules: ReadonlyArray<{ id: string; operator: string; value: number }>;
  filterRule: { column: number; label: string } | null;
  onApplyFilterRule: (type: "contains" | "equals" | "greaterThan" | "lessThan", value: string) => void;
  onClearFilterRule: () => void;
  dataValidationRules: ReadonlyArray<{ id: string; label: string }>;
  onApplyDataValidation: (input: { type: "list" | "wholeNumber" | "decimal" | "date"; first: string; second: string; allowBlank: boolean; errorMessage: string }) => void;
  onDeleteDataValidation: (ruleId: string) => void;
  /** 格式刷：复制聚焦样式，下一次选区应用。 */
  onPaintFormatToggle: () => void;
  paintFormatArmed: boolean;
}

type Icon = "fill" | "undo" | "redo" | "paste" | "copy" | "cut" | "table" | "cell" | "chart" | "spark" | "shape" | "link" | "pin" | "image" | "comment" | "filter" | "sort" | "columns" | "group" | "validate" | "check" | "shield" | "history" | "import" | "sum" | "formula" | "search" | "eye" | "freeze" | "moon" | "tool" | "eraser" | "pdf" | "magic" | "print" | "grid" | "more" | "alignLeft" | "alignCenter" | "alignRight" | "alignTop" | "alignMiddle" | "alignBottom" | "wrap" | "merge" | "currency" | "percent" | "thousands";
type Action = {
  label: string;
  icon: Icon;
  menu?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  pressed?: boolean;
  /** 该动作对应的语义命令；缺席表示纯占位/尚未接入引擎。 */
  capability?: SpreadsheetSemanticCommand["typeId"];
};
type Group = { actions: Action[]; className?: string };

/** 过滤掉服务端未发布对应命令的动作，避免渲染必然 400 的按钮。 */
function gateActions(actions: Action[], available: ReadonlySet<string>): Action[] {
  return actions.filter((action) => !action.capability || available.has(action.capability));
}

function Glyph({ name }: { name: Icon }) {
  const p = { fill: "none", stroke: "currentColor", strokeWidth: 1.45, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  const paths: Record<Icon, ReactNode> = {
    fill: <><path {...p} d="m7 2 7 7-6 6-6-6 6-6M3 9h10M14 11s-2 2-2 3a2 2 0 0 0 4 0c0-1-2-3-2-3Z" /></>,
    undo: <path {...p} d="M8 4 4.5 7.5 8 11M5 7.5h5a3.5 3.5 0 1 1 0 7H8" />, redo: <path {...p} d="m10 4 3.5 3.5L10 11m3-3.5H8a3.5 3.5 0 1 0 0 7h2" />,
    paste: <><rect {...p} x="5" y="4" width="8" height="10" rx="1" /><path {...p} d="M7 4V2.5h4V4M7 8h4M7 10.5h4" /></>, copy: <><rect {...p} x="6" y="5" width="7.5" height="8" rx=".8" /><path {...p} d="M4 11V3.8c0-.5.3-.8.8-.8H11" /></>, cut: <><circle {...p} cx="5" cy="12" r="1.6" /><circle {...p} cx="12" cy="12" r="1.6" /><path {...p} d="m6.3 10.9 5.1-7M11.7 10.9 6.6 3.9" /></>,
    table: <><rect {...p} x="2.7" y="3" width="12.6" height="12" rx=".7" /><path {...p} d="M2.7 7h12.6M2.7 11h12.6M7 3v12M11 3v12" /></>, cell: <><rect {...p} x="3" y="3" width="12" height="12" rx=".7" /><path {...p} d="M6 6h6M6 9h6M6 12h3" /></>, chart: <path {...p} d="M3 15V3M3 15h12M6 12V9M9 12V5M12 12V7" />, spark: <><path {...p} d="m2.5 12 3-3.5 2.6 1.7L11 5l4.5 2" /><path {...p} d="M2.5 15h13" /></>, shape: <><circle {...p} cx="6" cy="6" r="2.6" /><rect {...p} x="9" y="9" width="5.5" height="5.5" rx=".5" /></>, link: <><path {...p} d="M7.2 11.2 6 12.4a2.4 2.4 0 0 1-3.4-3.4l2.8-2.8A2.4 2.4 0 0 1 8.8 6" /><path {...p} d="m10.8 6.8 1.2-1.2A2.4 2.4 0 1 1 15.4 9l-2.8 2.8A2.4 2.4 0 0 1 9.2 12" /><path {...p} d="m6.7 11.3 4.6-4.6" /></>, pin: <><circle {...p} cx="9" cy="8" r="4.8" /><path {...p} d="M9 12.8v2M7.3 8h3.4M9 6.3v3.4" /></>, image: <><rect {...p} x="2.5" y="3" width="13" height="12" rx="1" /><circle {...p} cx="6.2" cy="6.8" r="1.1" /><path {...p} d="m3 13 3.5-3.5 2.4 2 2.1-1.8 4 3.3" /></>, comment: <><path {...p} d="M3 3.5h12v8H8l-3.5 3v-3H3z" /><path {...p} d="M6 6.5h6M6 8.5h4" /></>,
    filter: <path {...p} d="M2.5 3.5h13l-5.1 5.7v4l-2.8 1.3V9.2z" />, sort: <path {...p} d="M5 3v11M2.8 11.8 5 14l2.2-2.2M11 3h4M11 6h3M11 9h2" />, columns: <path {...p} d="M4 3v12M9 3v12M14 3v12M2.5 5.5h3M7.5 8h3M12.5 11h3" />, group: <><rect {...p} x="3" y="3" width="5.5" height="5.5" rx=".8" /><rect {...p} x="9.5" y="9.5" width="5.5" height="5.5" rx=".8" /><path {...p} d="m8.5 6 2 2" /></>, validate: <><rect {...p} x="3" y="3" width="12" height="12" rx=".8" /><path {...p} d="m5.5 9 2 2 5-5" /></>, check: <path {...p} d="m3 9 3.5 3.5L15 4" />, shield: <><path {...p} d="M9 2.5 14 4v4c0 3.3-2 5.7-5 7.5C6 13.7 4 11.3 4 8V4z" /><path {...p} d="M9 6v4M9 12h.01" /></>, history: <path {...p} d="M3 8.5A6 6 0 1 0 5 4M3 3.5V8h4.5" />, import: <path {...p} d="M9 2.5v9M5.5 8 9 11.5 12.5 8M3 14.5h12" />,
    sum: <path {...p} d="M13.5 3.5h-8L10 9l-4.5 5.5h8" />, formula: <path {...p} d="M4 3.5h10M5 14.5l5-11M7 8.5h5M4 14.5h10" />, search: <><circle {...p} cx="7.5" cy="7.5" r="4" /><path {...p} d="m10.5 10.5 4 4" /></>, eye: <><path {...p} d="M2.5 9s2.3-4 6.5-4 6.5 4 6.5 4-2.3 4-6.5 4-6.5-4-6.5-4Z" /><circle {...p} cx="9" cy="9" r="1.6" /></>, freeze: <path {...p} d="M9 2.5v13M3.4 5.8l11.2 6.4M14.6 5.8 3.4 12.2M4.7 3.6l8.6 10.8M13.3 3.6 4.7 14.4" />, moon: <path {...p} d="M14 11.6A5.5 5.5 0 0 1 6.4 4 5.5 5.5 0 1 0 14 11.6Z" />, tool: <path {...p} d="m11.8 3.2 3 3-7.9 7.9-3.2.2.2-3.2zM10.4 4.6l3 3" />, eraser: <><path {...p} d="m4 11 6.8-7 3.3 3.2-6.7 7H4.8L3 12.4z" /><path {...p} d="m8.2 6.7 3.3 3.2M7.5 14.2H15" /></>, pdf: <><path {...p} d="M5 2.5h5l3 3V15H5a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1Z" /><path {...p} d="M10 2.5v3h3M6 11h6M6 8h3" /></>, magic: <path {...p} d="m4 14 7.7-7.7M10.5 3.8l.5-1.3.5 1.3 1.3.5-1.3.5-.5 1.3-.5-1.3-1.3-.5zM4.2 7.4l.7-1.9.7 1.9 1.9.7-1.9.7-.7 1.9-.7-1.9-1.9-.7z" />, print: <path {...p} d="M5 6V2.8h8V6M5 12H3.5v-5h11v5H13M5 10h8v5H5z" />, grid: <><rect {...p} x="3" y="3" width="12" height="12" rx=".6" /><path {...p} d="M3 7h12M3 11h12M7 3v12M11 3v12" /></>, more: <><circle fill="currentColor" cx="4" cy="9" r="1" /><circle fill="currentColor" cx="9" cy="9" r="1" /><circle fill="currentColor" cx="14" cy="9" r="1" /></>,
    alignLeft: <path {...p} d="M3 4h10M3 7.3h7M3 10.7h11.5M3 14h8" />, alignCenter: <path {...p} d="M4 4h10M5.5 7.3h7M3 10.7h12M5 14h8" />, alignRight: <path {...p} d="M5 4h10M8 7.3h7M3.5 10.7H15M7 14h8" />,
    alignTop: <path {...p} d="M3 3.5h12M6 6h6M7 8.8h4M8 11.6h2M9 15V6" />, alignMiddle: <path {...p} d="M3 9h12M6 4.5h6M7 6.8h4M7 11.2h4M6 13.5h6" />, alignBottom: <path {...p} d="M3 14.5h12M6 12h6M7 9.2h4M8 6.4h2M9 3v9" />,
    wrap: <path {...p} d="M3 4.5h10a2.5 2.5 0 0 1 0 5H7M9.5 7 7 9.5 9.5 12M3 13.5h3" />, merge: <><rect {...p} x="2.8" y="4" width="12.4" height="10" rx=".6" /><path {...p} d="M6.2 4v10M11.8 4v10M4.5 9h9M7.8 7.3 6.2 9l1.6 1.7M10.2 7.3 11.8 9l-1.6 1.7" /></>,
    currency: <><path {...p} d="m5 4 4 5 4-5M9 9v6M5.5 10.5h7M5.5 13h7" /></>, percent: <><circle {...p} cx="5.3" cy="5.3" r="1.6" /><circle {...p} cx="12.7" cy="12.7" r="1.6" /><path {...p} d="M13.5 3.5 4.5 14.5" /></>, thousands: <><path {...p} d="M5.8 12.8c1.4-1 2-2.1 2-3.2 0-.9-.5-1.4-1.3-1.4-.7 0-1.3.5-1.3 1.2 0 .8.5 1.3 1.4 1.3M11.5 5v9" /></>,
  };
  return <svg className="ssr__glyph" viewBox="0 0 18 18" aria-hidden="true">{paths[name]}</svg>;
}
function ActionButton({ label, icon, menu, disabled, onClick, pressed }: Action) { return <button type="button" className={`ssr__action${pressed ? " is-pressed" : ""}`} title={label} aria-label={label} aria-pressed={pressed} disabled={disabled} onClick={onClick}><Glyph name={icon} />{menu && <i className="ssr__chevron" />}<span>{label}</span></button>; }
function RibbonGroup({ actions, className }: Group) { return <div className={`ssr__group${className ? ` ${className}` : ""}`}>{actions.map((action) => <ActionButton key={action.label} {...action} />)}</div>; }
const quiet = (label: string, icon: Icon, menu = false): Action => ({ label, icon, menu, disabled: true });
const unit = (items: Array<[string, Icon, boolean?]>) => ({ actions: items.map(([label, icon, menu]) => quiet(label, icon, menu)) });

function HomeMini({ label, icon, disabled, onClick, active, menu, iconOnly }: Action & { active?: boolean; iconOnly?: boolean }) {
  return <button type="button" className={`ssr__home-mini${active ? " is-active" : ""}`} disabled={disabled} onClick={onClick} title={label} aria-label={label} aria-pressed={active}><Glyph name={icon} />{!iconOnly && <span>{label}</span>}{menu && <i className="ssr__mini-chevron" />}</button>;
}

function HomeTile({ label, icon, disabled, onClick, menu, active }: Action & { active?: boolean }) {
  return <button type="button" className={`ssr__home-tile${active ? " is-active" : ""}`} disabled={disabled} onClick={onClick} title={label} aria-pressed={active}><Glyph name={icon} /><span>{label}</span>{menu && <i className="ssr__chevron" />}</button>;
}

function HomeText({ label, children, disabled, onClick, active }: { label: string; children: ReactNode; disabled?: boolean; onClick?: () => void; active?: boolean }) {
  return <button type="button" className={`ssr__home-text${active ? " is-active" : ""}`} title={label} aria-label={label} disabled={disabled} onClick={onClick} aria-pressed={active}>{children}</button>;
}

function rowColumnActions(props: SpreadsheetToolbarProps): Action[] {
  return [
    { label: "在上方插入行", icon: "table", capability: "spreadsheet.insertRows", onClick: props.onInsertRowAbove },
    { label: "在下方插入行", icon: "table", capability: "spreadsheet.insertRows", onClick: props.onInsertRowBelow },
    { label: "在左侧插入列", icon: "columns", capability: "spreadsheet.insertColumns", onClick: props.onInsertColumnLeft },
    { label: "在右侧插入列", icon: "columns", capability: "spreadsheet.insertColumns", onClick: props.onInsertColumnRight },
    { label: "删除所选行", icon: "table", capability: "spreadsheet.deleteRows", onClick: props.onDeleteRow },
    { label: "删除所选列", icon: "columns", capability: "spreadsheet.deleteColumns", onClick: props.onDeleteColumn },
  ];
}

function ActionMenu({ label, icon, disabled, actions, tile = false }: Action & { actions: Action[]; tile?: boolean }) {
  const [open, setOpen] = useState(false);
  return <Popover open={open} onOpenChange={setOpen} placement="bottom-start" role="menu" content={
    <div aria-label={label}>
      {actions.map((action) => <button key={action.label} type="button" role="menuitem" className="ssr__option" disabled={disabled || action.disabled}
        onClick={() => { action.onClick?.(); setOpen(false); }}>{action.label}</button>)}
    </div>
  }><button type="button" className={tile ? "ssr__home-tile" : "ssr__home-mini"} title={label} aria-label={label} disabled={disabled}>
    <span className="ssr__tile-icon"><Glyph name={icon} />{tile && <i className="ssr__chevron" />}</span><span>{label}</span>{!tile && <i className="ssr__chevron" />}
  </button></Popover>;
}

const FONT_SIZES = [10, 11, 12, 14, 16, 18, 20, 24, 28, 36] as const;
const PALETTE = ["#1d2129", "#f53f3f", "#ff7d00", "#fadc19", "#00b42a", "#0fc6c2", "#165dff", "#722ed1", "#86909c", "#ffffff"] as const;

function StyleSelect({ label, current, value = current, options, wide, onPick, disabled }: {
  label: string;
  current: string;
  value?: string;
  options: ReadonlyArray<{ value: string; label: string }>;
  wide?: boolean;
  onPick: (value: string) => void;
  disabled: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      role="listbox"
      content={
        <div className="ssr__optionlist" role="listbox" aria-label={label}>
          {options.map((option) => (
            <button
              key={option.value}
              type="button"
              role="option"
              aria-selected={option.value === value}
              className={`ssr__option${option.value === value ? " is-active" : ""}`}
              onClick={() => { onPick(option.value); setOpen(false); }}
            >
              {option.label}
            </button>
          ))}
        </div>
      }
    >
      <button type="button" className={`ssr__select${wide ? " is-wide" : ""}`} title={label} aria-label={label} disabled={disabled}>
        {current}<i className="ssr__chevron" />
      </button>
    </Popover>
  );
}

function ColorSwatch({ label, glyph, current, onPick, disabled }: {
  label: string;
  glyph: ReactNode;
  current: string | null;
  onPick: (color: string) => void;
  disabled: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}
      placement="bottom-start"
      content={
        <div className="ssr__colorgrid" role="listbox" aria-label={label}>
          {PALETTE.map((color) => (
            <button
              key={color}
              type="button"
              role="option"
              aria-selected={current === color}
              aria-label={color}
              className="ssr__colorcell"
              style={{ background: color }}
              onClick={() => { onPick(color); setOpen(false); }}
            />
          ))}
        </div>
      }
    >
      <button type="button" className="ssr__home-mini ssr__color-button" title={label} aria-label={label} disabled={disabled}>
        <span className="ssr__color-icon">{glyph}<span className="ssr__color-indicator" style={{ background: current ?? (label === "字体颜色" ? "#1d2129" : "#fadc19") }} /></span><i className="ssr__chevron" />
      </button>
    </Popover>
  );
}

function ConditionalFormatPanel({ props, inactive }: { props: SpreadsheetToolbarProps; inactive: boolean }) {
  const thresholdId = useId();
  const [operator, setOperator] = useState<ComparisonOperator>("greaterThan");
  const [view, setView] = useState<"menu" | "new" | "manage">("menu");
  const operators: Array<[ComparisonOperator, string]> = [["greaterThan", "大于"], ["lessThan", "小于"], ["equal", "等于"], ["notEqual", "不等于"], ["greaterThanOrEqual", "大于或等于"], ["lessThanOrEqual", "小于或等于"]];
  const [threshold, setThreshold] = useState("0");
  const parsed = Number(threshold);
  const valid = threshold !== "" && Number.isFinite(parsed);
  return (
    <div className="ssr__cformat">
      {view === "menu" && <div className="ssr__condition-menu">
        <button type="button" aria-label="突出显示单元格" onClick={() => setView("new")}>突出显示单元格 <span>›</span></button>
        {['高亮重复值', '高亮空值', '最前/最后/平均值', '自定义公式', '色阶', '数据条', '图标集'].map(label => <button key={label} type="button" disabled title="暂未支持">{label}</button>)}
        <hr /><button type="button" onClick={() => setView("new")}>新建条件格式</button>
        <button type="button" onClick={() => setView("manage")}>管理条件格式</button>
        <button type="button" disabled={inactive || !props.conditionalFormatRules.length || !props.availableCapabilities.has("spreadsheet.deleteConditionalFormat")} onClick={props.onClearConditionalFormats}>清除本工作表规则</button>
      </div>
      }
      {view !== "menu" && <div className="ssr__condition-editor">
      <button type="button" className="ssr__condition-back" onClick={() => setView("menu")}>‹ 条件格式</button>
      <strong>{view === "new" ? "突出显示单元格" : "当前工作表规则"}</strong>
      {view === "new" && <>
      <div className="ssr__cformat-row">
        <label className="ssr__cformat-label" htmlFor={thresholdId}>{operators.find(entry => entry[0] === operator)?.[1]}</label>
        <select aria-label="条件类型" value={operator} onChange={event => setOperator(event.target.value as ComparisonOperator)}>{operators.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select>
        <input
          id={thresholdId}
          className="ssr__cformat-input"
          type="number"
          value={threshold}
          disabled={inactive}
          onChange={(event) => setThreshold(event.target.value)}
        />
        <button
          type="button"
          className="ssr__home-mini"
          disabled={inactive || !valid || !props.availableCapabilities.has("spreadsheet.upsertConditionalFormat")}
          title="对选区应用条件格式"
          onClick={() => props.onUpsertConditionalFormat({ id: `cf-${Date.now()}`, threshold: parsed, operator })}
        >应用</button>
      </div>
      <p>应用到当前选区，红色文字与浅红填充。</p></>}
      {props.conditionalFormatRules.length === 0 && view === "manage" && <p>当前工作表没有条件格式规则</p>}
      {props.conditionalFormatRules.length > 0 && (
        <div className="ssr__cformat-rules" role="list" aria-label="现有规则">
          {props.conditionalFormatRules.map((rule) => (
            <div key={rule.id} className="ssr__cformat-rule" role="listitem">
              <span>{operators.find(entry => entry[0] === rule.operator)?.[1] ?? rule.operator} {rule.value}</span>
              <button
                type="button"
                className="ssr__cformat-delete"
                title="删除规则"
                disabled={!props.availableCapabilities.has("spreadsheet.deleteConditionalFormat")}
                onClick={() => props.onDeleteConditionalFormat(rule.id)}
              >×</button>
            </div>
          ))}
        </div>
      )}
      </div>}
    </div>
  );
}

function HomeRibbon({ props, inactive }: { props: SpreadsheetToolbarProps; inactive: boolean }) {
  const [borderOpen, setBorderOpen] = useState(false);
  const [tableOpen, setTableOpen] = useState(false);
  // 服务端未发布对应命令时，控件置灰而不是伪装成可用。
  const can = (typeId: string) => props.availableCapabilities.has(typeId);
  const canFormat = can("spreadsheet.formatRange");
  const canMerge = can("spreadsheet.mergeCells") && can("spreadsheet.unmergeCells");
  const sizeLabel = props.styleState.size != null ? String(Math.round(props.styleState.size)) : "10";
  const formatLabel = props.styleState.numberFormat
    ? (NUMBER_FORMAT_PRESETS.find((preset) => preset.key === props.styleState.numberFormat)?.label ?? props.styleState.numberFormat)
    : "常规";
  return <div className="ssr__homebar" role="toolbar" aria-label="开始">
    <div className="ssr__home-stack">
      <HomeMini iconOnly label="粘贴" icon="paste" disabled={props.disabled || !props.clipboardHasContent || !can("spreadsheet.pasteRange")} onClick={props.onPaste} />
      <div><HomeMini iconOnly label="剪切" icon="cut" disabled={inactive || !can("spreadsheet.clearRange")} onClick={props.onCut} /><HomeMini iconOnly label="复制" icon="copy" disabled={inactive} onClick={props.onCopy} /></div>
    </div>
    <div className="ssr__home-stack">
      <div><HomeMini iconOnly label="撤销" icon="undo" disabled={props.disabled || !props.canUndo || !can("spreadsheet.history")} onClick={props.onUndo} /><HomeMini iconOnly label="重做" icon="redo" disabled={props.disabled || !props.canRedo || !can("spreadsheet.history")} onClick={props.onRedo} /></div>
      <div><HomeMini iconOnly label="格式刷" icon="tool" disabled={props.disabled || !props.selectionActive} onClick={props.onPaintFormatToggle} active={props.paintFormatArmed} /><HomeMini iconOnly label="清除格式" icon="eraser" disabled={inactive || !canFormat} onClick={props.onClearFormatting} /></div>
    </div>
    <div className="ssr__home-font">
      <div className="ssr__home-fontrow">
        <FontPicker value={props.styleState.family ?? ""} disabled={inactive || !canFormat} onChange={props.onFontFamily} />
        <StyleSelect
          label="字号"
          current={sizeLabel}
          disabled={inactive || !canFormat}
          options={FONT_SIZES.map((size) => ({ value: String(size), label: String(size) }))}
          onPick={(value) => props.onFontSize(Number(value))}
        />
        <Popover open={borderOpen} onOpenChange={setBorderOpen}
          placement="bottom-start"
          role="menu"
          content={
            <>
              {([
                ["所有框线", { side: "all", style: "thin" }],
                ["外侧框线", { side: "outer", style: "thin" }],
                ["粗外侧框线", { side: "outer", style: "medium" }],
                ["上框线", { side: "top", style: "thin" }],
                ["下框线", { side: "bottom", style: "thin" }],
                ["左框线", { side: "left", style: "thin" }],
                ["右框线", { side: "right", style: "thin" }],
                ["虚线框线", { side: "all", style: "dashed" }],
                ["双线框线", { side: "outer", style: "double" }],
                ["无框线", { side: "none", style: "thin" }],
              ] as Array<[string, BorderPreset]>).map(([label, preset]) => (
                <button key={label} type="button" role="menuitem" className="ssr__option" onClick={() => { props.onBorderPreset(preset); setBorderOpen(false); }}>
                  {label}
                </button>
              ))}
            </>
          }
        >
          <button type="button" className="ssr__home-mini" title="边框" aria-label="边框" disabled={inactive || !canFormat}><Glyph name="grid" /><i className="ssr__chevron" /></button>
        </Popover>
        <Popover popupClassName="ssr__wide-popup" open={tableOpen} onOpenChange={setTableOpen} placement="bottom-start" content={<SpreadsheetTableStyles onPick={(preset, options) => { props.onTableStyle(preset, options); setTableOpen(false); }} onClear={() => { props.onClearFormatting(); setTableOpen(false); }} />}>
          <button type="button" className="ssr__home-mini" title="表格样式" aria-label="表格样式" disabled={inactive || !canFormat}><Glyph name="table" /><span>表格样式</span><i className="ssr__chevron" /></button>
        </Popover>
      </div>
      <div className="ssr__home-fontrow"><HomeText label="加粗" disabled={inactive || !canFormat} onClick={props.onBold} active={props.styleState.bold}><b>B</b></HomeText><HomeText label="斜体" disabled={inactive || !canFormat} onClick={props.onItalic} active={props.styleState.italic}><i>I</i></HomeText><HomeText label="下划线" disabled={inactive || !canFormat} onClick={props.onUnderline} active={props.styleState.underline}><u>U</u></HomeText><HomeText label="删除线" disabled={inactive || !canFormat} onClick={props.onStrikethrough} active={props.styleState.strikethrough}><s>S</s></HomeText>
        <ColorSwatch label="字体颜色" glyph={<span className="ssr__letter-color">A</span>} current={props.styleState.fontColor} onPick={props.onFontColor} disabled={inactive || !canFormat} />
        <ColorSwatch label="填充颜色" glyph={<Glyph name="fill" />} current={props.styleState.fillColor} onPick={props.onFillColor} disabled={inactive || !canFormat} />
        <Popover
          placement="bottom-start"
          popupClassName="ssr__wide-popup"
          content={<ConditionalFormatPanel props={props} inactive={inactive} />}
        >
          <button type="button" className="ssr__home-mini" title="条件格式" disabled={props.disabled || !can("spreadsheet.upsertConditionalFormat")}><Glyph name="table" /><span>条件格式</span><i className="ssr__mini-chevron" /></button>
        </Popover>
      </div>
    </div>
    <div className="ssr__home-alignment">
      <div className="ssr__home-aligngrid">
        <HomeMini iconOnly label="左对齐" icon="alignLeft" disabled={inactive || !canFormat} onClick={() => props.onAlignHorizontal("left")} active={props.styleState.horizontal === "left"} />
        <HomeMini iconOnly label="居中" icon="alignCenter" disabled={inactive || !canFormat} onClick={() => props.onAlignHorizontal("center")} active={props.styleState.horizontal === "center"} />
        <HomeMini iconOnly label="右对齐" icon="alignRight" disabled={inactive || !canFormat} onClick={() => props.onAlignHorizontal("right")} active={props.styleState.horizontal === "right"} />
        <HomeMini iconOnly label="顶端对齐" icon="alignTop" disabled={inactive || !canFormat} onClick={() => props.onAlignVertical("top")} active={props.styleState.vertical === "top"} />
        <HomeMini iconOnly label="垂直居中" icon="alignMiddle" disabled={inactive || !canFormat} onClick={() => props.onAlignVertical("middle")} active={props.styleState.vertical === "middle"} />
        <HomeMini iconOnly label="底端对齐" icon="alignBottom" disabled={inactive || !canFormat} onClick={() => props.onAlignVertical("bottom")} active={props.styleState.vertical === "bottom"} />
      </div>
      <div className="ssr__home-aligncommands">
        <HomeMini label="自动换行" icon="wrap" disabled={inactive || !canFormat} onClick={props.onWrap} active={props.styleState.wrap} />
        <div className="ssr__merge-split"><HomeMini label="合并" icon="merge" disabled={props.disabled || !props.canMergeToggle || !canMerge} onClick={props.onMergeToggle} active={props.mergeActive} /><ActionMenu label="合并选项" icon="more" disabled={props.disabled || !props.canMergeToggle || !canMerge} actions={[
          { label: props.mergeActive ? "取消合并" : "合并单元格", icon: "merge", onClick: props.onMergeToggle },
          { label: "合并相同单元格", icon: "merge", disabled: true },
          { label: "合并并居中", icon: "merge", disabled: props.mergeActive || !canMerge, onClick: props.onMergeCenter },
        ]} /></div>
      </div>
    </div>
    <div className="ssr__home-number">
      <StyleSelect
        label="数字格式"
        current={formatLabel}
        value={props.styleState.numberFormat ?? "General"}
        wide
        disabled={inactive || !canFormat}
        options={NUMBER_FORMAT_PRESETS.map((preset) => ({ value: preset.key, label: `${preset.label}（${preset.sample}）` }))}
        onPick={props.onNumberFormat}
      />
      <div className="ssr__home-numberrow"><HomeMini iconOnly label="货币" icon="currency" disabled={inactive || !canFormat} onClick={() => props.onNumberFormat("¥#,##0.00")} /><HomeMini iconOnly label="百分比" icon="percent" disabled={inactive || !canFormat} onClick={() => props.onNumberFormat("0.00%")} /><HomeMini iconOnly label="千分位" icon="thousands" disabled={inactive || !canFormat} onClick={() => props.onNumberFormat("#,##0")} /></div>
    </div>
    <div className="ssr__home-pairgrid ssr__home-data-grid">
      <HomeMini label="筛选" icon="filter" disabled={props.disabled || !can("spreadsheet.setAutoFilter")} onClick={props.onFilterToggle} active={props.filterActive} />
      <ActionMenu label="排序" icon="sort" disabled={inactive || !can("spreadsheet.sortRange")} actions={[
        { label: "升序", icon: "sort", onClick: () => props.onSort("ascending") },
        { label: "降序", icon: "sort", onClick: () => props.onSort("descending") },
      ]} />
      <HomeMini label="冻结" icon="freeze" disabled={props.disabled || !can("spreadsheet.setFreezePane")} onClick={props.onFreezeToggle} active={props.frozen} />
      <HomeMini label="保护" icon="shield" disabled />
      <HomeMini label="求和" icon="sum" disabled={props.disabled || !can("spreadsheet.setCell")} onClick={props.onAutoSum} />
      <HomeMini label="查找" icon="search" disabled={props.disabled || !can("spreadsheet.replaceRange")} onClick={props.onFindReplace} />
    </div>
    <div className="ssr__home-tiles">
      <ActionMenu label="插入" icon="more" tile disabled={props.disabled} actions={gateActions(rowColumnActions(props), props.availableCapabilities)} /><span className="ssr__wide-tools"><HomeTile label="图片" icon="image" disabled menu /><HomeTile label="图表" icon="chart" disabled menu /><HomeTile label="透视表" icon="table" disabled menu /></span><HomeTile label="快捷工具" icon="magic" disabled menu /><span className="ssr__wide-tools"><HomeTile label="生成图片" icon="image" disabled /><HomeTile label="图片转表格" icon="table" disabled /><HomeTile label="PDF转换" icon="pdf" disabled menu /></span><HomeTile label="打印" icon="print" disabled />
    </div>
  </div>;
}

function FormulaRibbon() {
  const upper: Array<[string, Icon]> = [["求和", "sum"], ["财务", "formula"], ["文本", "formula"], ["统计", "formula"], ["信息", "formula"], ["查找与引用", "search"], ["兼容性", "formula"]];
  const lower: Array<[string, Icon]> = [["逻辑", "formula"], ["日期", "formula"], ["工程", "formula"], ["数据库", "table"], ["数学与三角", "formula"], ["特色函数", "magic"]];
  return <div className="ssr__formula-bar" role="toolbar" aria-label="公式">
    <div className="ssr__formula-matrix">
      <div>{upper.map(([label, icon]) => <HomeMini key={label} label={label} icon={icon} menu />)}</div>
      <div>{lower.map(([label, icon]) => <HomeMini key={label} label={label} icon={icon} menu />)}</div>
    </div>
    <div className="ssr__formula-tiles">
      <HomeTile label="名称管理" icon="formula" menu /><HomeTile label="快速创建" icon="formula" /><HomeTile label="跨文件引用" icon="table" /><HomeTile label="计算选项" icon="cell" />
    </div>
  </div>;
}

function InsertRibbon({ props }: { props: SpreadsheetToolbarProps }) {
  return <div className="ssr__bar" role="toolbar" aria-label="插入">
    <RibbonGroup {...unit([["透视表", "table", true]])} />
    <div className="ssr__group">
      <ActionButton {...quiet("单元格", "cell", true)} />
      <ActionMenu label="行列" icon="columns" tile disabled={props.disabled} actions={gateActions(rowColumnActions(props), props.availableCapabilities)} />
      {unit([["图表", "chart", true], ["迷你图", "spark", true], ["形状", "shape", true], ["链接", "link"], ["位置", "pin"], ["切片器", "table"]]).actions.map(action => <ActionButton key={action.label} {...action} />)}
    </div>
    <RibbonGroup {...unit([["单元格图片", "image", true], ["浮动图片", "image"], ["批量插入图片", "image"]])} />
    <RibbonGroup {...unit([["腾讯文档", "pdf"], ["本地文件", "pdf"], ["微云文件", "pdf"]])} />
    <RibbonGroup {...unit([["批注", "comment"]])} />
  </div>;
}

function DataRibbon({ props, inactive }: { props: SpreadsheetToolbarProps; inactive: boolean }) {
  const can = (typeId: string) => props.availableCapabilities.has(typeId);
  const [filterType, setFilterType] = useState<"contains" | "equals" | "greaterThan" | "lessThan">("contains");
  const [filterValue, setFilterValue] = useState("");
  const [validationType, setValidationType] = useState<"list" | "wholeNumber" | "decimal" | "date">("list");
  const [first, setFirst] = useState("");
  const [second, setSecond] = useState("");
  const [allowBlank, setAllowBlank] = useState(true);
  const [errorMessage, setErrorMessage] = useState("");
  return <div className="ssr__bar ssr__data-tools" role="toolbar" aria-label="数据">
    <div className="ssr__data-card">
      <strong>筛选当前列</strong>
      <select aria-label="筛选条件" value={filterType} onChange={event => setFilterType(event.target.value as typeof filterType)}>
        <option value="contains">包含</option><option value="equals">等于</option><option value="greaterThan">大于</option><option value="lessThan">小于</option>
      </select>
      <input aria-label="筛选值" value={filterValue} onChange={event => setFilterValue(event.target.value)} />
      <button type="button" disabled={inactive || !filterValue || !can("spreadsheet.upsertFilterColumn")} onClick={() => props.onApplyFilterRule(filterType, filterValue)}>应用筛选</button>
      <button type="button" disabled={props.disabled || !props.filterRule || !can("spreadsheet.clearFilter")} onClick={props.onClearFilterRule}>清除{props.filterRule ? `（${props.filterRule.label}）` : ""}</button>
      <button type="button" disabled={inactive || !can("spreadsheet.sortRange")} onClick={() => props.onSort("ascending")}>升序</button>
      <button type="button" disabled={inactive || !can("spreadsheet.sortRange")} onClick={() => props.onSort("descending")}>降序</button>
      <button type="button" disabled={props.disabled || !can("spreadsheet.setAutoFilter")} aria-pressed={props.filterActive} onClick={props.onFilterToggle}>{props.filterActive ? "关闭筛选" : "启用筛选"}</button>
    </div>
    <div className="ssr__data-card">
      <strong>数据验证</strong>
      <select aria-label="验证类型" value={validationType} onChange={event => setValidationType(event.target.value as typeof validationType)}>
        <option value="list">列表（逗号分隔）</option><option value="wholeNumber">整数范围</option><option value="decimal">小数范围</option><option value="date">日期序列范围</option>
      </select>
      <input aria-label={validationType === "list" ? "允许值" : "最小值"} value={first} onChange={event => setFirst(event.target.value)} />
      {validationType !== "list" && <input aria-label="最大值" value={second} onChange={event => setSecond(event.target.value)} />}
      <label><input type="checkbox" checked={allowBlank} onChange={event => setAllowBlank(event.target.checked)} />允许空白</label>
      <input aria-label="错误提示" placeholder="可选错误提示" value={errorMessage} onChange={event => setErrorMessage(event.target.value)} />
      <button type="button" disabled={inactive || !first || (validationType !== "list" && !second) || !can("spreadsheet.upsertDataValidation")} onClick={() => props.onApplyDataValidation({ type: validationType, first, second, allowBlank, errorMessage })}>应用验证</button>
    </div>
    <div className="ssr__data-card" aria-label="现有数据验证">
      <strong>现有规则</strong>
      {props.dataValidationRules.length === 0 ? <span>无</span> : props.dataValidationRules.map(rule => <span key={rule.id}>{rule.label}<button type="button" aria-label={`删除 ${rule.label}`} disabled={!can("spreadsheet.deleteDataValidation")} onClick={() => props.onDeleteDataValidation(rule.id)}>×</button></span>)}
    </div>
  </div>;
}

export function SpreadsheetToolbar(props: SpreadsheetToolbarProps) {
  const inactive = props.disabled || !props.selectionActive;
  const panels: Record<Exclude<RibbonTab, "home" | "tableStyle">, Group[]> = {
    insert: [unit([["透视表", "table", true], ["单元格", "cell", true], ["图表", "chart", true], ["迷你图", "spark", true], ["形状", "shape", true], ["链接", "link"], ["位置", "pin"], ["切片器", "table"], ["单元格图片", "image", true], ["浮动图片", "image"], ["批量插入图片", "image"], ["腾讯文档", "pdf"], ["本地文件", "pdf"], ["微云文件", "pdf"], ["批注", "comment"]])],
    data: [{ actions: [quiet("透视表", "table"), { label: "筛选", icon: "filter", capability: "spreadsheet.setAutoFilter", disabled: props.disabled, onClick: props.onFilterToggle, pressed: props.filterActive, menu: true }, { label: "排序", icon: "sort", capability: "spreadsheet.sortRange", disabled: inactive, onClick: () => props.onSort("ascending"), menu: true }, ...unit([["条件格式", "table", true], ["分列", "columns", true], ["分组", "group", true], ["数据验证", "validate", true], ["下拉选项", "more"], ["复选框", "check"], ["保护", "shield", true], ["生成查询", "search"], ["修订记录", "history"], ["单元格修订记录", "history"], ["订阅更新", "table"], ["导入数据", "import"]]).actions] }],
    formula: [unit([["求和", "sum", true], ["财务", "formula", true], ["文本", "formula", true], ["统计", "formula", true], ["信息", "formula", true], ["查找与引用", "search", true], ["兼容性", "formula", true], ["逻辑", "formula", true], ["日期", "formula", true], ["工程", "formula", true], ["数据库", "table", true], ["数学与三角", "formula", true], ["特色函数", "magic", true], ["名称管理", "formula"], ["快速创建", "formula"], ["跨文件引用", "table"], ["计算选项", "cell"]])],
    collaborate: [unit([["邀请协作", "comment", true], ["共享", "link", true], ["评论", "comment"], ["保护", "shield", true], ["修订记录", "history"], ["订阅更新", "table"]])],
    view: [{ actions: [...unit([["高亮所在行列", "grid", true], ["行高列宽", "columns", true]]).actions, { label: "冻结窗格", icon: "freeze", capability: "spreadsheet.setFreezePane", disabled: props.disabled, onClick: props.onFreezeToggle, pressed: props.frozen, menu: true }, ...unit([["深色显示", "moon", true], ["显示网格线", "grid"], ["显示比例", "eye", true], ["显示零值", "eye"], ["新建筛选视图", "filter"], ["切换筛选视图", "table", true]]).actions] }],
    efficiency: [unit([["重复项", "copy", true], ["批量清除", "tool", true], ["批量图片处理", "image", true], ["身份证工具", "cell", true], ["单元格处理", "cell", true], ["合并表格", "table"], ["生成目录", "pdf"], ["PDF转换", "pdf", true], ["生成图片", "image"], ["生成智能表", "magic"], ["关联收集表", "check"], ["图片转表格", "table"], ["订阅更新", "table", true], ["插件", "tool", true]])],
    membership: [unit([["会员权益", "magic"], ["智能表格", "table"], ["高级图表", "chart"], ["更多功能", "more", true]])],
  };
  const groups = props.tab === "home" || props.tab === "tableStyle" ? [] : panels[props.tab];
  return <section className="ssr" aria-label="电子表格功能区"><div className="ssr__tabs" role="tablist" aria-label="功能区标签页">{[...RIBBON_TABS, ...(props.tableStyleContext ? [{ id: "tableStyle" as const, label: "表格样式" }] : [])].map((entry) => <button key={entry.id} type="button" role="tab" aria-selected={props.tab === entry.id} className={`ssr__tab${props.tab === entry.id ? " is-active" : ""}`} onClick={() => props.onTabChange(entry.id)}>{entry.label}</button>)}</div>{props.tab === "tableStyle" && props.tableStyleContext ? <div className="ssr__bar" role="toolbar" aria-label="表格样式"><SpreadsheetTableStyles ribbon currentOptions={props.tableStyleContext.options} currentPreset={props.tableStyleContext.preset} disabled={props.disabled} onPick={props.onContextTableStyle} onClear={props.onClearTableStyle} /></div> : props.tab === "home" ? <HomeRibbon props={props} inactive={inactive} /> : props.tab === "insert" ? <InsertRibbon props={props} /> : props.tab === "formula" ? <FormulaRibbon /> : props.tab === "data" ? <DataRibbon props={props} inactive={inactive} /> : <div className="ssr__bar">{groups.map((group, index) => <RibbonGroup key={index} {...group} actions={gateActions(group.actions, props.availableCapabilities)} />)}</div>}</section>;
}
