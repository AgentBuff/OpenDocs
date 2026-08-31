import { Icon, Popover, Toolbar, ToolbarButton, ToolbarField, ToolbarGroup, ToolbarSelect, ToolbarSplitGroup } from "@open-office/ui";
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import type { ArtifactPageSetup, DocumentBlockKind, DocumentCommand } from "@open-office/schema/artifact";
import { action } from "../actions/registry.js";
import type { ActionId, EditorActionContext } from "../actions/registry.js";
import { blockToolbarSections } from "../toolbar/items.js";
import type { ToolbarItem } from "../toolbar/items.js";
import { InsertMenu } from "../toolbar/InsertMenu.js";
import { PageSetupPanel } from "./PageSetupPanel.js";
import { ColorPalette, type ColorRole, type ColorValue } from "./ColorPalette.js";
import { ParagraphSettingsDialog, type ParagraphSettingsValue } from "./ParagraphSettingsDialog.js";
import type { ToolbarSelectionState } from "../toolbar/selectionState.js";
import { FONT_OPTIONS } from "../typography/fonts.js";

type ActionToolbarItem = ToolbarItem & { kind: "button" | "toggle" };

export function BlockToolbar({
  activeKind,
  actionContext,
  formatPainterActive,
  insertEnabled,
  onInsertCode,
  onInsertQuote,
  onInsertCallout,
  onInsertTodo,
  onInsertDivider,
  onInsertImage,
  onInlineAttrs,
  onBlockPresentation,
  onHistory,
  historyOpen,
  exportHref,
  onPageSetupOpenChange,
  pageSetupOpen,
  pageSetup,
  onPageSetupChange,
  onSave,
  saveDisabled,
  lineHeight,
  paragraphSettings,
  toolbarSelection,
  activePresentation,
}: {
  activeKind: DocumentBlockKind;
  actionContext: EditorActionContext;
  formatPainterActive: boolean;
  insertEnabled: boolean;
  onInsertCode: () => void;
  onInsertQuote: () => void;
  onInsertCallout: () => void;
  onInsertTodo: () => void;
  onInsertDivider: () => void;
  onInsertImage: (file: File) => Promise<void> | void;
  onInlineAttrs: (attrs: Record<string, unknown | null>) => void;
  onBlockPresentation: (patch: Extract<DocumentCommand, { type: "setBlockPresentation" }>["patch"]) => void;
  onHistory: () => void;
  historyOpen: boolean;
  exportHref: string;
  onPageSetupOpenChange: (open: boolean) => void;
  pageSetupOpen: boolean;
  pageSetup: ArtifactPageSetup;
  onPageSetupChange: (patch: Partial<ArtifactPageSetup>) => void;
  onSave: () => void;
  saveDisabled: boolean;
  lineHeight?: number;
  paragraphSettings: ParagraphSettingsValue;
  toolbarSelection: ToolbarSelectionState;
  activePresentation: {
    align: "left" | "center" | "right" | "justify";
    list: "bullet" | "ordered" | null;
  };
}) {
  const [paragraphSettingsOpen, setParagraphSettingsOpen] = useState(false);
  const isSpecialBlock = activeKind.type === "quote" || activeKind.type === "code";
  const kind = activeKind.type === "heading" ? `heading-${activeKind.level}` : activeKind.type;
  const renderAction = (item: ActionToolbarItem) => {
    const actionId = item.action;
    if (!actionId || actionId === "insert-menu" || actionId === "kind-select") return null;
    const registered = action(actionId as ActionId);
    const active = item.action === "formatPainter"
      ? formatPainterActive
      : item.action === "bold"
        ? toolbarSelection.marks.bold
        : item.action === "italic"
          ? toolbarSelection.marks.italic
          : item.action === "underline"
            ? toolbarSelection.marks.underline
            : item.action === "strike"
              ? toolbarSelection.marks.strikethrough
              : item.action === "alignLeft"
                ? activePresentation.align === "left"
                : item.action === "alignCenter"
                  ? activePresentation.align === "center"
                  : item.action === "alignRight"
                    ? activePresentation.align === "right"
                    : item.action === "alignJustify"
                      ? activePresentation.align === "justify"
                      : item.action === "bulletList"
                        ? activePresentation.list === "bullet"
                        : item.action === "orderedList"
                          ? activePresentation.list === "ordered"
                          : item.action === "todo" && activeKind.type === "todo";
    return (
      <ToolbarButton
        key={item.id}
        tone={item.action === "delete" ? "danger" : "default"}
        active={active}
        aria-label={registered.title}
        aria-keyshortcuts={registered.shortcut}
        onClick={() => registered.run(actionContext)}
        disabled={!registered.isEnabled(actionContext)}
        title={`${registered.title}${registered.shortcut ? ` (${registered.shortcut})` : ""}`}
      >
        {item.icon && <Icon name={item.icon} />}
      </ToolbarButton>
    );
  };

  const renderToolbarItem = (item: ToolbarItem) => {
    if (item.kind === "button" || item.kind === "toggle") return renderAction(item as ActionToolbarItem);
    if (item.kind === "menu" && item.action === "insert-menu") {
      return <InsertMenu
        key={item.id}
        disabled={!insertEnabled}
        onInsert={actionContext.onInsert}
        onInsertCode={onInsertCode}
        onInsertQuote={onInsertQuote}
        onInsertCallout={onInsertCallout}
        onInsertTodo={onInsertTodo}
        onInsertDivider={onInsertDivider}
        onInsertLink={actionContext.onLink}
        onInsertImage={onInsertImage}
        onInsertTable={actionContext.onInsertTable}
      />;
    }
    if (item.kind === "select" && item.action === "kind-select") return <span key={item.id}>{renderKindSelect()}</span>;
    return null;
  };

  const renderKindSelect = () => isSpecialBlock ? (
    <ToolbarField className="toolbar-field--special" aria-label={activeKind.type === "quote" ? "引用块" : "代码块"}>
      {activeKind.type === "quote" ? "引用块" : "代码块"}
    </ToolbarField>
  ) : (
    <ToolbarSelect
      className="toolbar-select--kind"
      value={kind}
      disabled={!actionContext.hasActiveBlock}
      onValueChange={(nextValue) => actionContext.onKind(parseKind(nextValue))}
      aria-label="块样式"
      options={[
        { value: "paragraph", label: "正文" },
        { value: "heading-1", label: "标题 1" },
        { value: "heading-2", label: "标题 2" },
        { value: "heading-3", label: "标题 3" },
        { value: "heading-4", label: "标题 4" },
        { value: "heading-5", label: "标题 5" },
        { value: "heading-6", label: "标题 6" },
        { value: "todo", label: "待办事项" },
      ]}
    />
  );

  const renderFontControls = () => (
    <>
      <ToolbarSelect
        className="toolbar-select--font"
        value={toolbarSelection.fontFamily ?? ""}
        disabled={!actionContext.hasTextSelection}
        onValueChange={(nextValue) => {
          if (nextValue) onInlineAttrs({ fontFamily: nextValue });
        }}
        aria-label="字体"
        options={FONT_OPTIONS}
      />
      <ToolbarSelect
        className="toolbar-select--size"
        value={toolbarSelection.fontSize ? String(Math.round(toolbarSelection.fontSize)) : ""}
        disabled={!actionContext.hasTextSelection}
        onValueChange={(nextValue) => {
          const size = Number(nextValue);
          if (Number.isFinite(size)) onInlineAttrs({ fontSize: size });
        }}
        aria-label="字号"
        options={[
          { value: "", label: "字号" },
          ...[12, 14, 16, 18, 24, 32].map((size) => ({ value: String(size), label: String(size) })),
        ]}
      />
    </>
  );

  const renderColorControls = () => (
    <>
      <ToolbarColorSplit
        label="字体颜色"
        className="toolbar-color-split--text"
        primaryClassName="toolbar-button--text-accent"
        primaryContent={<Icon name="font-colors" />}
        role="text"
        selectedColor={toolbarSelection.color}
        disabled={!actionContext.hasTextSelection}
        defaultColor="#1d2129"
        onApply={(color) => onInlineAttrs({ color })}
      />
      <ToolbarColorSplit
        label="文字高亮"
        className="toolbar-color-split--highlight"
        primaryClassName="toolbar-button--highlight"
        primaryContent={<Icon name="highlight" />}
        role="highlight"
        selectedColor={toolbarSelection.highlight}
        disabled={!actionContext.hasTextSelection}
        defaultColor="#fff2ac"
        onApply={(color) => onInlineAttrs({ highlight: color })}
      />
    </>
  );

  const renderLineHeight = () => (
      <ToolbarField className="toolbar-field--line-height">
        <Icon name="line-height" />
        <ToolbarSelect
        className="toolbar-select--line-height"
        popupClassName="oo-toolbar-select__popover--line-height"
          value={String(lineHeight ?? 1)}
          disabled={!actionContext.hasActiveBlock}
        onValueChange={(nextValue) => {
          const lineHeight = Number(nextValue);
          if (Number.isFinite(lineHeight)) onBlockPresentation({ lineHeight });
        }}
        aria-label="行距"
        options={[
          ...[1, 1.15, 1.3, 1.5, 2, 3].map((lineHeight) => ({ value: String(lineHeight), label: String(lineHeight) })),
        ]}
      />
    </ToolbarField>
  );

  return (
    <Toolbar className="toolbar--blocks" density="compact" role="toolbar" aria-label="文档工具栏">
        {blockToolbarSections.map((section) => (
          <ToolbarGroup className={`toolbar__group--${section.id}`} role="group" aria-label={section.label} key={section.id}>
            {section.id === "text" ? (
              <>
                {renderKindSelect()}
                {renderFontControls()}
                {section.items.filter((item): item is ActionToolbarItem => item.kind === "button" || item.kind === "toggle").map(renderAction)}
                {renderColorControls()}
              </>
            ) : section.id === "more" ? (
              <>
                <ToolbarButton aria-label="插入引用" title="插入引用" onClick={onInsertQuote} disabled={!insertEnabled}><Icon name="quote" /></ToolbarButton>
                <ToolbarButton aria-label="插入高亮块" title="插入高亮块" onClick={onInsertCallout} disabled={!insertEnabled}><Icon name="highlight" /></ToolbarButton>
                <ToolbarButton aria-label="段落设置" title="段落设置" onClick={() => setParagraphSettingsOpen(true)} disabled={!actionContext.hasActiveBlock}><Icon name="settings" /></ToolbarButton>
              </>
            ) : section.items.map(renderToolbarItem)}
            {section.id === "paragraph" && renderLineHeight()}
          </ToolbarGroup>
        ))}
        <span className="toolbar__utility" role="group" aria-label="文档工具">
          <ToolbarButton className="toolbar__utility-button" onClick={onHistory} aria-label="历史版本" aria-expanded={historyOpen}><Icon name="history" /><span className="toolbar__utility-button-label">历史版本</span></ToolbarButton>
          <a className="toolbar__utility-button" href={exportHref} download aria-label="导出 DOCX"><Icon name="download" /><span className="toolbar__utility-button-label">导出 DOCX</span></a>
          <Popover
            open={pageSetupOpen}
            onOpenChange={onPageSetupOpenChange}
            placement="bottom-end"
            role="presentation"
            popupClassName="oo-overlay--page-setup"
            content={<PageSetupPanel value={pageSetup} onChange={onPageSetupChange} />}
          >
          <ToolbarButton className="toolbar__utility-button" aria-label="页面设置" aria-expanded={pageSetupOpen}><Icon name="settings" /><span className="toolbar__utility-button-label">页面设置</span></ToolbarButton>
          </Popover>
          <ToolbarButton className="toolbar__utility-button toolbar__utility-button--primary" tone="accent" onClick={onSave} aria-label="保存" disabled={saveDisabled}>保存</ToolbarButton>
        </span>
        <ParagraphSettingsDialog
          open={paragraphSettingsOpen}
          value={paragraphSettings}
          onClose={() => setParagraphSettingsOpen(false)}
          onApply={(value) => onBlockPresentation({ ...value })}
        />
    </Toolbar>
  );
}

function ToolbarColorSplit({
  label,
  className,
  primaryClassName,
  primaryContent,
  role,
  defaultColor,
  selectedColor: externalColor,
  disabled = false,
  onApply,
}: {
  label: string;
  className?: string;
  primaryClassName?: string;
  primaryContent: ReactNode;
  role: ColorRole;
  defaultColor: string;
  selectedColor?: ColorValue;
  disabled?: boolean;
  onApply: (color: ColorValue) => void;
}) {
  const [open, setOpen] = useState(false);
  const [selectedColor, setSelectedColor] = useState<ColorValue>(externalColor ?? null);
  const [recentColors, setRecentColors] = useState<string[]>([]);
  useEffect(() => setSelectedColor(externalColor ?? null), [externalColor]);
  const apply = (color: ColorValue) => {
    setSelectedColor(color);
    if (color) setRecentColors((previous) => [color, ...previous.filter((item) => item !== color)].slice(0, 10));
    onApply(color);
    setOpen(false);
  };
  return (
    <ToolbarSplitGroup className={className} aria-label={label}>
      <ToolbarButton className={primaryClassName} aria-label={label} title={label} disabled={disabled} onClick={() => apply(selectedColor ?? defaultColor)}>
        <span
          className="toolbar-color-trigger-icon"
          style={{ "--oo-toolbar-color-value": selectedColor ?? defaultColor } as CSSProperties}
        >
          {primaryContent}
        </span>
      </ToolbarButton>
      <Popover
        open={open}
        onOpenChange={setOpen}
        placement="bottom-start"
        role="presentation"
        popupClassName="oo-overlay--toolbar-color"
        content={(
          <ColorPalette role={role} selectedColor={selectedColor} recentColors={recentColors} onSelect={apply} />
        )}
      >
        <ToolbarButton className="oo-toolbar__item--split-arrow" aria-label={`${label}更多选项`} aria-haspopup="menu" disabled={disabled}>
          <Icon name="arrow-down" />
        </ToolbarButton>
      </Popover>
    </ToolbarSplitGroup>
  );
}

function parseKind(value: string): DocumentBlockKind {
  if (value.startsWith("heading-")) return { type: "heading", level: Number(value.slice(8)) };
  if (value === "todo") return { type: "todo" };
  return { type: "paragraph" };
}
