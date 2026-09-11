import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Popover } from "@open-office/ui";
import { FONT_ASSETS, loadFont, type FontAsset } from "./font-loading.js";
import { FONT_LABELS as labels, fontValue } from "./fonts.js";
import "./font-picker.css";

function scriptLabel(font: FontAsset): string {
  if (font.subsets.some(subset => subset.includes('chinese'))) return '中文';
  for (const [subset, label] of [['japanese', '日文'], ['korean', '韩文'], ['arabic', '阿拉伯文'], ['hebrew', '希伯来文'], ['devanagari', '天城文'], ['thai', '泰文']] as const) {
    if (font.subsets.includes(subset)) return label;
  }
  return '西文';
}

function FontPreview({ font, selected, onPick }: { font: FontAsset; selected: boolean; onPick: () => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      void loadFont(font.family, labels[font.id] ?? font.family).catch(() => setFailed(true));
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [font]);
  return <button ref={ref} type="button" role="option" aria-label={labels[font.id] ?? font.family} aria-selected={selected} className="oo-font-option" style={{ fontFamily: fontValue(font) }} onClick={onPick}>
    <span>{labels[font.id] ?? font.family}</span><small aria-hidden="true">{failed ? "加载失败 · 点击重试" : scriptLabel(font)}</small>
  </button>;
}

export function FontPicker({ value, disabled, onChange, className = "" }: { value: string; disabled?: boolean; onChange: (value: string) => void; className?: string }) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const primary = value.split(',')[0]?.trim().replace(/^['"]|['"]$/g, '');
  const current = FONT_ASSETS.find(font => font.family === primary);
  const options = useMemo(() => FONT_ASSETS.filter(font => `${font.family} ${labels[font.id] ?? ''} ${font.category}`.toLowerCase().includes(query.toLowerCase())), [query]);
  const groups = useMemo(() => [
    { label: '中文与东亚文字', fonts: options.filter(font => font.subsets.some(s => s.includes('chinese') || s === 'japanese' || s === 'korean')) },
    { label: '西文与其他文字', fonts: options.filter(font => !font.subsets.some(s => s.includes('chinese') || s === 'japanese' || s === 'korean') && font.category !== 'monospace') },
    { label: '等宽', fonts: options.filter(font => font.category === 'monospace') },
  ], [options]);
  const moveFocus = (event: React.KeyboardEvent<HTMLElement>) => {
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    if (event.target instanceof HTMLInputElement && ['Home', 'End'].includes(event.key)) return;
    const items = Array.from(document.getElementById(listId)?.querySelectorAll<HTMLButtonElement>('[role="option"]') ?? []);
    if (!items.length) return;
    event.preventDefault();
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : event.key === 'ArrowDown' ? (index + 1) % items.length : (index < 0 ? items.length - 1 : (index - 1 + items.length) % items.length);
    items[next]?.focus();
  };
  const pick = async (font: FontAsset) => {
    setLoading(true); setError("");
    try {
      const sample = font.subsets.some(s => s.includes('chinese')) ? '字体中文' : font.subsets.includes('japanese') ? '日本語' : font.subsets.includes('korean') ? '한국어' : font.subsets.includes('arabic') ? 'العربية' : font.subsets.includes('devanagari') ? 'नमस्ते' : font.subsets.includes('thai') ? 'ภาษาไทย' : font.subsets.includes('hebrew') ? 'שלום' : 'BESbswy';
      await loadFont(font.family, sample);
      onChange(fontValue(font)); setOpen(false);
    } catch { setError(`无法加载 ${labels[font.id] ?? font.family}，请重试。`); }
    finally { setLoading(false); }
  };
  return <Popover open={open} onOpenChange={next => { setOpen(next); if (next) { setQuery(''); setError(''); } }} popupClassName="oo-font-popup" content={<div className="oo-font-picker" onKeyDown={moveFocus}>
    <input aria-label="搜索字体" placeholder="搜索字体" value={query} onChange={event => setQuery(event.target.value)} />
    <div id={listId} role="listbox" aria-label="字体" aria-busy={loading}>{groups.filter(group => group.fonts.length).map(group => <div key={group.label} role="group" aria-label={group.label}><div className="oo-font-group" aria-hidden="true">{group.label}</div>{group.fonts.map(font => <FontPreview key={font.id} font={font} selected={font.family === primary} onPick={() => { if (!loading) void pick(font); }} />)}</div>)}{!options.length && <p>没有匹配的字体</p>}</div>
    {(loading || error) && <p role="status">{error || '正在加载字体…'}</p>}
  </div>}><button type="button" role="combobox" aria-label="字体" title="字体" aria-expanded={open} aria-controls={open ? listId : undefined} disabled={disabled} className={`oo-font-trigger ${className}`} onMouseDown={event => event.preventDefault()}><span>{current ? labels[current.id] ?? current.family : primary || '默认字体'}</span><span aria-hidden="true">⌄</span></button></Popover>;
}
