import { Button, Icon, Input, Portal, Select } from "@open-office/ui";
import { useEffect, useState } from "react";

export interface ParagraphSettingsValue {
  align: "left" | "center" | "right" | "justify";
  indentLevel: number;
  indentRight: number;
  spacingBefore: number;
  spacingAfter: number;
  lineHeight: number;
}

interface ParagraphSettingsDialogProps {
  open: boolean;
  value: ParagraphSettingsValue;
  onClose: () => void;
  onApply: (value: ParagraphSettingsValue) => void;
}

const numberValue = (value: string, fallback: number) => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? Math.max(0, parsed) : fallback;
};

/** Paragraph formatting is a document attribute editor, not a visual-only modal. */
export function ParagraphSettingsDialog({ open, value, onClose, onApply }: ParagraphSettingsDialogProps) {
  const [draft, setDraft] = useState(value);

  useEffect(() => { if (open) setDraft(value); }, [open, value]);
  if (!open) return null;

  const updateNumber = (key: keyof Pick<ParagraphSettingsValue, "indentLevel" | "indentRight" | "spacingBefore" | "spacingAfter" | "lineHeight">, raw: string) => {
    setDraft((current) => ({ ...current, [key]: numberValue(raw, current[key]) }));
  };

  return (
    <Portal>
      <div className="paragraph-settings__scrim" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
        <section className="paragraph-settings" role="dialog" aria-modal="true" aria-labelledby="paragraph-settings-title">
          <header className="paragraph-settings__header">
            <h2 id="paragraph-settings-title">段落</h2>
            <button type="button" className="paragraph-settings__close" aria-label="关闭段落设置" onClick={onClose}><Icon name="close" /></button>
          </header>
          <div className="paragraph-settings__tabs" role="tablist" aria-label="段落设置分类">
            <button type="button" role="tab" aria-selected>缩进和间距</button>
            <button type="button" role="tab" aria-selected={false} disabled>换行和分页</button>
            <button type="button" role="tab" aria-selected={false} disabled>中文版式</button>
          </div>
          <div className="paragraph-settings__body">
            <fieldset>
              <legend>常规</legend>
              <label><span>对齐方式</span><Select value={draft.align} onChange={(event) => setDraft((current) => ({ ...current, align: event.target.value as ParagraphSettingsValue["align"] }))}>
                <option value="left">左对齐</option><option value="center">居中对齐</option><option value="right">右对齐</option><option value="justify">两端对齐</option>
              </Select></label>
            </fieldset>
            <fieldset>
              <legend>缩进</legend>
              <div className="paragraph-settings__grid">
                <label><span>左侧</span><Input type="number" min="0" step="1" value={draft.indentLevel} onChange={(event) => updateNumber("indentLevel", event.target.value)} suffix="字符" /></label>
                <label><span>右侧</span><Input type="number" min="0" step="1" value={draft.indentRight} onChange={(event) => updateNumber("indentRight", event.target.value)} suffix="字符" /></label>
              </div>
            </fieldset>
            <fieldset>
              <legend>间距</legend>
              <div className="paragraph-settings__grid">
                <label><span>段前</span><Input type="number" min="0" step="1" value={draft.spacingBefore} onChange={(event) => updateNumber("spacingBefore", event.target.value)} suffix="px" /></label>
                <label><span>段后</span><Input type="number" min="0" step="1" value={draft.spacingAfter} onChange={(event) => updateNumber("spacingAfter", event.target.value)} suffix="px" /></label>
                <label><span>行距</span><Select value={String(draft.lineHeight)} onChange={(event) => updateNumber("lineHeight", event.target.value)}>{[1, 1.15, 1.3, 1.5, 2, 3].map((lineHeight) => <option key={lineHeight} value={lineHeight}>{lineHeight} 倍</option>)}</Select></label>
              </div>
            </fieldset>
          </div>
          <footer className="paragraph-settings__footer"><Button onClick={onClose}>取消</Button><Button variant="primary" onClick={() => { onApply(draft); onClose(); }}>确定</Button></footer>
        </section>
      </div>
    </Portal>
  );
}
