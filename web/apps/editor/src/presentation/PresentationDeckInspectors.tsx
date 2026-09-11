import { useEffect, useState } from "react";
import { Button, Checkbox, ColorPalette, Icon, IconButton, Input, Popover, Select, Textarea } from "@open-office/ui";
import type {
  PresentationV5Deck,
  PresentationV5Layout,
  PresentationV5Master,
  PresentationV5SlideBackground,
  PresentationV5SlideTransition,
  PresentationV5TimelineEntry,
} from "@open-office/schema";
import type { PresentationDeckProjection, PresentationSlideProjection } from "@open-office/schema/api";

import { TimelinePanel } from "./TimelinePanel.js";
import {
  asSlideBackground,
  colorRefInputValue,
  pageSafeArea,
  solidColor,
  themeIdForName,
} from "./presentationInspectorValues.js";

export function SlideInspector({
  slide,
  deck,
  selectedNodeId,
  disabled,
  availableCapabilities,
  onClose,
  onNotesChange,
  onBackgroundChange,
  onLayoutChange,
  onTransitionChange,
  onAnimationUpsert,
  onAnimationDelete,
  onAnimationMove,
}: {
  slide: PresentationSlideProjection;
  deck: PresentationDeckProjection;
  selectedNodeId?: string | null;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onClose: () => void;
  onNotesChange: (notes: string | null) => void;
  onBackgroundChange: (background: PresentationV5SlideBackground) => void;
  onLayoutChange: (layoutId: string | null) => void;
  onTransitionChange: (transition: PresentationV5SlideTransition | null) => void;
  onAnimationUpsert: (animation: PresentationV5TimelineEntry) => void;
  onAnimationDelete: (animationId: string) => void;
  onAnimationMove: (animationId: string, index: number) => void;
}) {
  const initialBackground = asSlideBackground(slide.background);
  const [notes, setNotes] = useState(slide.notes ?? "");
  const [notesComposing, setNotesComposing] = useState(false);
  const [backgroundEnabled, setBackgroundEnabled] = useState(initialBackground.type === "solid");
  const [backgroundColor, setBackgroundColor] = useState(colorRefInputValue(initialBackground.type === "solid" ? initialBackground.value : null));
  const [layoutId, setLayoutId] = useState(slide.layoutId ?? "");
  useEffect(() => {
    const nextBackground = asSlideBackground(slide.background);
    setNotes(slide.notes ?? "");
    setBackgroundEnabled(nextBackground.type === "solid");
    setBackgroundColor(colorRefInputValue(nextBackground.type === "solid" ? nextBackground.value : null));
    setLayoutId(slide.layoutId ?? "");
  }, [slide.background, slide.layoutId, slide.notes, slide.slideId]);
  const canNotes = availableCapabilities.has("presentation.setSlideNotes");
  const canBackground = availableCapabilities.has("presentation.setSlideBackground");
  const canLayout = availableCapabilities.has("presentation.setSlideLayout") && deck.layouts.length > 0;
  return <div className="presentation-studio__inspector-card">
    <header className="presentation-studio__inspector-header">
      <div><span>幻灯片属性</span><strong>{slide.name || "未命名幻灯片"}</strong></div>
      <IconButton type="button" variant="ghost" size="sm" aria-label="关闭幻灯片检查器" title="关闭幻灯片检查器" onClick={onClose}><Icon name="close" /></IconButton>
    </header>
    {canLayout && <section className="presentation-studio__inspector-section">
      <h3>版式</h3>
      <label>幻灯片版式<Select aria-label="幻灯片版式" disabled={disabled} value={layoutId} onChange={(event) => setLayoutId(event.target.value)}>
        <option value="">空白</option>
        {deck.masters.map((master) => {
          const layouts = deck.layouts.filter((layout) => layout.masterId === master.id);
          return layouts.length > 0 ? <optgroup key={master.id} label={master.name || "未命名母版"}>
            {layouts.map((layout) => <option key={layout.id} value={layout.id}>{layout.name || "未命名版式"}</option>)}
          </optgroup> : null;
        })}
      </Select></label>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onLayoutChange(layoutId || null)}>应用版式</Button>
    </section>}
    {canBackground && <section className="presentation-studio__inspector-section">
      <h3>背景</h3>
      <div className="presentation-studio__inspector-control-group">
        <Checkbox checked={backgroundEnabled} disabled={disabled} onChange={(event) => setBackgroundEnabled(event.target.checked)}>使用纯色背景</Checkbox>
        <ColorPickerField ariaLabel="背景颜色" role="fill" value={backgroundColor} disabled={disabled || !backgroundEnabled} compact onValueChange={(value) => setBackgroundColor(value ?? "#ffffff")} />
      </div>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onBackgroundChange(backgroundEnabled ? { type: "solid", value: solidColor(backgroundColor) } : { type: "none" })}>应用背景</Button>
    </section>}
    <TimelinePanel
      slide={slide}
      selectedNodeId={selectedNodeId}
      disabled={disabled}
      availableCapabilities={availableCapabilities}
      onTransitionChange={onTransitionChange}
      onAnimationUpsert={onAnimationUpsert}
      onAnimationDelete={onAnimationDelete}
      onAnimationMove={onAnimationMove}
    />
    {canNotes && <section className="presentation-studio__inspector-section">
      <h3>演讲者备注</h3>
      <Textarea aria-label="演讲者备注" value={notes} disabled={disabled} placeholder="仅在编辑与演讲者视图中可见" onCompositionStart={() => setNotesComposing(true)} onCompositionEnd={(event) => { setNotesComposing(false); setNotes(event.currentTarget.value); }} onChange={(event) => setNotes(event.target.value)} />
      <Button type="button" size="sm" disabled={disabled || notesComposing} onClick={() => onNotesChange(notes.trim() ? notes : null)}>保存备注</Button>
    </section>}
  </div>;
}

export function DeckInspector({
  deck,
  disabled,
  availableCapabilities,
  onClose,
  onPageSpecChange,
  onThemeChange,
  onCreateMaster,
  onUpdateMaster,
  onDeleteMaster,
  onCreateLayout,
  onUpdateLayout,
  onDeleteLayout,
}: {
  deck: PresentationDeckProjection;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onClose: () => void;
  onPageSpecChange: (pageSpec: PresentationV5Deck["pageSpec"]) => void;
  onThemeChange: (theme: PresentationV5Deck["theme"]) => void;
  onCreateMaster: () => void;
  onUpdateMaster: (master: PresentationV5Master) => void;
  onDeleteMaster: (masterId: string) => void;
  onCreateLayout: (masterId: string) => void;
  onUpdateLayout: (layout: PresentationV5Layout) => void;
  onDeleteLayout: (layoutId: string) => void;
}) {
  const initialFormat = deck.pageSpec.width / deck.pageSpec.height > 1.55 ? "wide" : "standard";
  const [format, setFormat] = useState(initialFormat);
  const [themeName, setThemeName] = useState(deck.themeName);
  useEffect(() => {
    setFormat(deck.pageSpec.width / deck.pageSpec.height > 1.55 ? "wide" : "standard");
    setThemeName(deck.themeName);
  }, [deck.pageSpec.height, deck.pageSpec.width, deck.themeName]);
  const canSetPageSpec = availableCapabilities.has("presentation.setPageSpec");
  const canSetTheme = availableCapabilities.has("presentation.setTheme");
  const canCreateMaster = availableCapabilities.has("presentation.createMaster");
  const canUpdateMaster = availableCapabilities.has("presentation.updateMaster");
  const canDeleteMaster = availableCapabilities.has("presentation.deleteMaster");
  const canCreateLayout = availableCapabilities.has("presentation.createLayout");
  const canUpdateLayout = availableCapabilities.has("presentation.updateLayout");
  const canDeleteLayout = availableCapabilities.has("presentation.deleteLayout");
  const formatSpec = format === "wide"
    ? { width: 12_192_000, height: 6_858_000 }
    : { width: 9_144_000, height: 6_858_000 };
  return <div className="presentation-studio__inspector-card">
    <header className="presentation-studio__inspector-header">
      <div><span>演示文稿</span><strong>设计</strong></div>
      <IconButton type="button" variant="ghost" size="sm" aria-label="关闭设计检查器" title="关闭设计检查器" onClick={onClose}><Icon name="close" /></IconButton>
    </header>
    {canSetPageSpec && <section className="presentation-studio__inspector-section">
      <h3>页面比例</h3>
      <label>幻灯片尺寸<Select disabled={disabled} value={format} onChange={(event) => setFormat(event.target.value as typeof format)}>
        <option value="wide">宽屏 16:9</option><option value="standard">标准 4:3</option>
      </Select></label>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onPageSpecChange({ ...formatSpec, unit: deck.pageSpec.unit, safeArea: pageSafeArea(deck.pageSpec.safeArea) })}>应用页面比例</Button>
    </section>}
    {canSetTheme && <section className="presentation-studio__inspector-section">
      <h3>主题</h3>
      <label>主题名称<Input value={themeName} disabled={disabled} onChange={(event) => setThemeName(event.target.value)} /></label>
      <Button type="button" size="sm" disabled={disabled || !themeName.trim()} onClick={() => onThemeChange({ id: themeIdForName(themeName), name: themeName.trim() })}>应用主题</Button>
      <p>主题的字体和配色由 Deck 的严格主题引用解析；此入口不会重写幻灯片或对象样式。</p>
    </section>}
    {(canCreateMaster || deck.masters.length > 0) && <section className="presentation-studio__inspector-section">
      <h3>母版</h3>
      {canCreateMaster && <Button type="button" size="sm" disabled={disabled} onClick={onCreateMaster}>新建母版</Button>}
      {deck.masters.map(({ master }) => <div className="presentation-studio__inspector-control-group" key={master.id}>
        <Input
          aria-label={`母版名称：${master.name}`}
          defaultValue={master.name}
          disabled={disabled || !canUpdateMaster}
          onBlur={(event) => {
            const name = event.currentTarget.value.trim();
            if (name && name !== master.name) onUpdateMaster({ ...master, name });
          }}
        />
        {canCreateLayout && <Button type="button" size="sm" disabled={disabled} onClick={() => onCreateLayout(master.id)}>新建版式</Button>}
        {canDeleteMaster && <Button type="button" size="sm" variant="danger" disabled={disabled} onClick={() => onDeleteMaster(master.id)}>删除</Button>}
      </div>)}
    </section>}
    {(canCreateLayout || deck.layouts.length > 0) && <section className="presentation-studio__inspector-section">
      <h3>版式</h3>
      {deck.layouts.map(({ layout }) => <div className="presentation-studio__inspector-control-group" key={layout.id}>
        <Input
          aria-label={`版式名称：${layout.name}`}
          defaultValue={layout.name}
          disabled={disabled || !canUpdateLayout}
          onBlur={(event) => {
            const name = event.currentTarget.value.trim();
            if (name && name !== layout.name) onUpdateLayout({ ...layout, name });
          }}
        />
        {canDeleteLayout && <Button type="button" size="sm" variant="danger" disabled={disabled} onClick={() => onDeleteLayout(layout.id)}>删除</Button>}
      </div>)}
    </section>}
  </div>;
}

export function ColorPickerField({
  label,
  ariaLabel,
  role,
  value,
  disabled,
  compact = false,
  onValueChange,
}: {
  label?: string;
  ariaLabel?: string;
  role: "text" | "fill" | "stroke";
  value: string;
  disabled: boolean;
  compact?: boolean;
  onValueChange: (value: string | null) => void;
}) {
  const trigger = <Button type="button" variant="secondary" size="sm" disabled={disabled} className={compact ? "presentation-studio__color-trigger presentation-studio__color-trigger--compact" : "presentation-studio__color-trigger"} aria-label={ariaLabel ?? label}>
    <span className="presentation-studio__color-swatch" style={{ backgroundColor: value }} aria-hidden="true" />
    {!compact && <span>{value.toUpperCase()}</span>}
  </Button>;
  const picker = <Popover placement="bottom-start" content={<ColorPalette role={role} value={value} onValueChange={onValueChange} />}>
    {trigger}
  </Popover>;
  return label ? <label>{label}{picker}</label> : picker;
}
