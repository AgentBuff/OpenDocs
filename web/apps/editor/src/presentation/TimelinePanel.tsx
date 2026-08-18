import { useEffect, useMemo, useState } from "react";

import { Button, Icon, IconButton, Input, Select } from "@open-office/ui";
import type {
  PresentationSlideProjection,
  PresentationV5SlideTransition,
  PresentationV5TimelineEntry,
} from "@open-office/schema";

export interface TimelinePanelAvailability {
  readonly transition: boolean;
  readonly upsert: boolean;
  readonly remove: boolean;
  readonly move: boolean;
}

/**
 * A pure capability gate for the timeline surface.  It deliberately does not
 * infer editability from a handler, which keeps unadvertised protocol
 * operations out of the Studio.
 */
export function resolveTimelinePanelAvailability(
  capabilities: ReadonlySet<string>,
): TimelinePanelAvailability {
  return {
    transition: capabilities.has("presentation.setSlideTransition"),
    upsert: capabilities.has("presentation.upsertAnimation"),
    remove: capabilities.has("presentation.deleteAnimation"),
    move: capabilities.has("presentation.moveAnimation"),
  };
}

export interface TimelinePanelProps {
  readonly slide: PresentationSlideProjection;
  readonly disabled: boolean;
  readonly availableCapabilities: ReadonlySet<string>;
  readonly onTransitionChange: (transition: PresentationV5SlideTransition | null) => void;
  readonly onAnimationUpsert: (animation: PresentationV5TimelineEntry) => void;
  readonly onAnimationDelete: (animationId: string) => void;
  readonly onAnimationMove: (animationId: string, index: number) => void;
}

/**
 * Slide-scoped timeline editor.  The panel keeps only form draft state; the
 * authoritative timeline stays in the server projection and is changed by
 * one narrow semantic command per action.
 */
export function TimelinePanel({
  slide,
  disabled,
  availableCapabilities,
  onTransitionChange,
  onAnimationUpsert,
  onAnimationDelete,
  onAnimationMove,
}: TimelinePanelProps) {
  const availability = resolveTimelinePanelAvailability(availableCapabilities);
  const [transition, setTransition] = useState<PresentationV5SlideTransition | null>(slide.transition ?? null);
  useEffect(() => setTransition(slide.transition ?? null), [slide.slideId, slide.transition]);

  const entries = useMemo(
    () => [...(slide.timeline?.entries ?? [])].sort((left, right) => left.orderKey.localeCompare(right.orderKey)),
    [slide.timeline?.entries],
  );
  const targetNames = useMemo(
    () => new Map((slide.nodes ?? []).map((node) => [node.id, node.name || node.kind.type])),
    [slide.nodes],
  );
  const transitionKind = transition?.kind ?? "none";
  const transitionDuration = transition?.durationMs ?? 300;

  if (!availability.transition && !availability.upsert && !availability.remove && !availability.move) return null;

  return <section className="presentation-timeline" aria-label="动画与切换">
    <h3>动画与切换</h3>
    {availability.transition && <div className="presentation-timeline__transition">
      <label>页面切换
        <Select aria-label="页面切换效果" disabled={disabled} value={transitionKind} onChange={(event) => {
          const kind = event.target.value as PresentationV5SlideTransition["kind"] | "none";
          setTransition(kind === "none" ? null : { kind, durationMs: transitionDuration });
        }}>
          <option value="none">无</option>
          <option value="fade">淡入淡出</option>
          <option value="push">推进</option>
          <option value="wipe">擦除</option>
        </Select>
      </label>
      {transition && <label>时长（毫秒）
        <Input aria-label="页面切换时长" type="number" min="0" max="600000" disabled={disabled} value={transition.durationMs} onChange={(event) => setTransition((current) => current ? { ...current, durationMs: boundedMilliseconds(event.target.value) } : current)} />
      </label>}
      <Button type="button" size="sm" disabled={disabled} onClick={() => onTransitionChange(transition)}>应用切换</Button>
    </div>}

    <div className="presentation-timeline__header">
      <span>对象动画</span>
      <span aria-label={`${entries.length} 个动画`}>{entries.length}</span>
    </div>
    {entries.length === 0 ? <p className="presentation-timeline__empty">选择对象后可在对象属性中添加动画。</p> : (
      <ol className="presentation-timeline__entries" aria-label="动画顺序">
        {entries.map((entry, index) => <TimelineEntryEditor
          key={entry.id}
          entry={entry}
          targetName={targetNames.get(entry.targetNodeId) ?? "未知对象"}
          index={index}
          total={entries.length}
          disabled={disabled}
          availability={availability}
          onUpsert={onAnimationUpsert}
          onDelete={onAnimationDelete}
          onMove={onAnimationMove}
        />)}
      </ol>
    )}
  </section>;
}

function TimelineEntryEditor({
  entry,
  targetName,
  index,
  total,
  disabled,
  availability,
  onUpsert,
  onDelete,
  onMove,
}: {
  entry: PresentationV5TimelineEntry;
  targetName: string;
  index: number;
  total: number;
  disabled: boolean;
  availability: TimelinePanelAvailability;
  onUpsert: (animation: PresentationV5TimelineEntry) => void;
  onDelete: (animationId: string) => void;
  onMove: (animationId: string, index: number) => void;
}) {
  const [draft, setDraft] = useState(entry);
  useEffect(() => setDraft(entry), [entry]);

  return <li className="presentation-timeline__entry">
    <div className="presentation-timeline__entry-heading">
      <span className="presentation-timeline__order" aria-label={`第 ${index + 1} 个动画`}>{index + 1}</span>
      <strong title={targetName}>{targetName}</strong>
      <div className="presentation-timeline__entry-actions" aria-label="调整动画顺序">
        {availability.move && <>
          <IconButton type="button" size="sm" variant="ghost" aria-label="上移动画" title="上移动画" disabled={disabled || index === 0} onClick={() => onMove(entry.id, index - 1)}><Icon name="arrow-up" /></IconButton>
          <IconButton type="button" size="sm" variant="ghost" aria-label="下移动画" title="下移动画" disabled={disabled || index === total - 1} onClick={() => onMove(entry.id, index + 1)}><Icon name="arrow-down" /></IconButton>
        </>}
        {availability.remove && <IconButton type="button" size="sm" variant="ghost" aria-label="删除动画" title="删除动画" disabled={disabled} onClick={() => onDelete(entry.id)}><Icon name="delete" /></IconButton>}
      </div>
    </div>
    {availability.upsert && <div className="presentation-timeline__entry-fields">
      <label>效果<Select aria-label={`${targetName} 动画效果`} disabled={disabled} value={draft.preset} onChange={(event) => setDraft((current) => ({ ...current, preset: event.target.value as PresentationV5TimelineEntry["preset"] }))}>
        <option value="appear">出现</option><option value="fade">淡入</option><option value="flyIn">飞入</option><option value="wipe">擦除</option>
      </Select></label>
      <label>触发<Select aria-label={`${targetName} 动画触发`} disabled={disabled} value={draft.trigger} onChange={(event) => setDraft((current) => ({ ...current, trigger: event.target.value as PresentationV5TimelineEntry["trigger"] }))}>
        <option value="onClick">单击时</option><option value="withPrevious">与上一动画同时</option><option value="afterPrevious">上一动画之后</option>
      </Select></label>
      <label>时长<Input aria-label={`${targetName} 动画时长`} type="number" min="0" max="600000" disabled={disabled} value={draft.durationMs} onChange={(event) => setDraft((current) => ({ ...current, durationMs: boundedMilliseconds(event.target.value) }))} /></label>
      <label>延迟<Input aria-label={`${targetName} 动画延迟`} type="number" min="0" max="600000" disabled={disabled} value={draft.delayMs} onChange={(event) => setDraft((current) => ({ ...current, delayMs: boundedMilliseconds(event.target.value) }))} /></label>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onUpsert(draft)}>更新</Button>
    </div>}
  </li>;
}

function boundedMilliseconds(value: string): number {
  return Math.max(0, Math.min(600_000, Number(value) || 0));
}
