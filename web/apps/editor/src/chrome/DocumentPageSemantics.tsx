import { useEffect, useMemo, useState } from "react";

import type {
  DocumentHeaderFooter,
  DocumentPageNumbering,
  DocumentSection,
  HeaderFooterSegment,
} from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";

export function DocumentPageSemantics({
  session,
  revision,
  onClose,
}: {
  session: BlockSessionApi;
  revision: number;
  onClose: () => void;
}) {
  const projection = useMemo(() => session.printProjection(), [revision, session]);
  const roots = session.projection.getStructureSnapshot().root;
  const activeRoot = session.state.activeBlockId && roots.includes(session.state.activeBlockId)
    ? session.state.activeBlockId
    : roots[0];
  const sectionIndex = Math.max(0, projection?.sections.findIndex((item) => item.rootBlockIds.includes(activeRoot ?? "")) ?? 0);
  const section = projection?.sections[sectionIndex];
  const [headerText, setHeaderText] = useState("");
  const [footerText, setFooterText] = useState("");
  const [showPageNumber, setShowPageNumber] = useState(false);
  const [pageStart, setPageStart] = useState(1);
  const [noteText, setNoteText] = useState("");

  useEffect(() => {
    setHeaderText(readText(section?.header ?? null));
    setFooterText(readText(section?.footer ?? null));
    setShowPageNumber(section?.footer?.default.segments.some((item) => item.type === "pageNumber") ?? false);
    setPageStart(section?.pageNumbering?.startAt ?? 1);
  }, [section]);

  if (!projection || !activeRoot || !section) return null;

  const saveSection = () => {
    const footerSegments = textSegments(footerText);
    if (showPageNumber) footerSegments.push({ type: "pageNumber" });
    const next: DocumentSection = {
      id: section.sectionId ?? `section-${crypto.randomUUID()}`,
      startBlockId: section.rootBlockIds[0] ?? activeRoot,
      pageSetup: section.pageSetup,
      header: headerText.trim() ? headerFooter(textSegments(headerText)) : null,
      footer: footerSegments.length > 0 ? headerFooter(footerSegments) : null,
      pageNumbering: showPageNumber
        ? { startAt: Math.max(1, Math.floor(pageStart)), format: "decimal" } satisfies DocumentPageNumbering
        : null,
    };
    session.upsertSection(next, sectionIndex);
  };

  const addNote = (kind: "footnote" | "endnote") => {
    const content = noteText.trim();
    const block = session.projection.getBlock(activeRoot);
    if (!content || !block?.content) return;
    session.upsertNote(kind, {
      id: `${kind}-${crypto.randomUUID()}`,
      anchor: { blockId: activeRoot, rowId: null, cellId: null, start: 0, end: 0 },
      content: [{ text: content, runs: [] }],
    });
    setNoteText("");
  };

  return (
    <aside className="document-page-semantics" aria-label="页面语义">
      <header>
        <strong>页面语义</strong>
        <button className="btn btn--ghost btn--sm" type="button" onClick={onClose}>关闭</button>
      </header>
      <label>页眉<input value={headerText} onChange={(event) => setHeaderText(event.target.value)} /></label>
      <label>页脚<input value={footerText} onChange={(event) => setFooterText(event.target.value)} /></label>
      <label className="document-page-semantics__check">
        <input type="checkbox" checked={showPageNumber} onChange={(event) => setShowPageNumber(event.target.checked)} />显示页码
      </label>
      <label>起始页码<input type="number" min={1} value={pageStart} onChange={(event) => setPageStart(Number(event.target.value))} /></label>
      <button className="btn btn--primary btn--sm" type="button" onClick={saveSection}>应用到当前节</button>
      {section.sectionId && projection.sections.length > 1 && (
        <button className="btn btn--ghost btn--sm" type="button" onClick={() => session.deleteSection(section.sectionId!)}>删除当前节</button>
      )}
      <hr />
      <label>注释内容<textarea value={noteText} onChange={(event) => setNoteText(event.target.value)} rows={3} /></label>
      <div className="document-page-semantics__actions">
        <button className="btn btn--ghost btn--sm" type="button" onClick={() => addNote("footnote")}>添加脚注</button>
        <button className="btn btn--ghost btn--sm" type="button" onClick={() => addNote("endnote")}>添加尾注</button>
      </div>
      {[{ label: "脚注", kind: "footnote" as const, notes: projection.footnotes }, { label: "尾注", kind: "endnote" as const, notes: projection.endnotes }].map((group) => group.notes.length > 0 && (
        <section key={group.kind}>
          <strong>{group.label}</strong>
          {group.notes.map((note) => (
            <div className="document-page-semantics__note" key={note.id}>
              <span>{note.content.map((item) => item.text).join(" ")}</span>
              <button type="button" aria-label={`删除${group.label}`} onClick={() => session.deleteNote(group.kind, note.id)}>×</button>
            </div>
          ))}
        </section>
      ))}
    </aside>
  );
}

function readText(value: DocumentHeaderFooter | null): string {
  return value?.default.segments
    .filter((segment): segment is Extract<HeaderFooterSegment, { type: "text" }> => segment.type === "text")
    .map((segment) => segment.content.text)
    .join("") ?? "";
}

function textSegments(value: string): HeaderFooterSegment[] {
  return value.trim() ? [{ type: "text", content: { text: value, runs: [] } }] : [];
}

function headerFooter(segments: HeaderFooterSegment[]): DocumentHeaderFooter {
  return { default: { segments }, firstPage: null, evenPages: null };
}
