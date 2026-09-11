import { useCallback, useEffect, useRef, useState } from "react";
import { OpenOfficeSdk } from "@open-office/sdk";
import type { SceneElement, WhiteboardModel } from "@open-office/schema/artifact";
import { api } from "../api.js";
import { FontPicker } from "../typography/FontPicker.js";
import "./whiteboard.css";
const sdk = new OpenOfficeSdk();

/** Text objects use the whiteboard Scene Graph and its canonical transaction path. */
export function WhiteboardStudio({ id, title, onBack }: { id: string; title: string; onBack: () => void }) {
  const [model, setModel] = useState<WhiteboardModel | null>(null);
  const queue = useRef(Promise.resolve());
  const pending = useRef(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [text, setText] = useState("");
  const selected = model?.elements.find(element => element.id === selectedId);
  const textSelected = selected?.typeId === "whiteboard.text";
  const refresh = useCallback(async () => {
    const snapshot = await api.getArtifact(id);
    if (snapshot.artifact.payload.kind !== "whiteboard") throw new Error("此文件不是白板");
    setModel(snapshot.artifact.payload.data);
  }, [id]);
  useEffect(() => { void refresh().catch(reason => setError(String(reason))); }, [refresh]);
  useEffect(() => { setText(typeof selected?.attrs.text === "string" ? selected.attrs.text : ""); }, [selected]);
  const submit = (commands: Array<{ typeId: string; payload: Record<string, unknown> }>): Promise<boolean> => {
    pending.current++; setSaving(true); setError("");
    const job = queue.current.then(async () => {
      try {
        const latest = await api.getArtifact(id);
        if (latest.artifact.payload.kind !== "whiteboard") throw new Error("此文件不是白板");
        const elements = latest.artifact.payload.data.elements;
        const merged = commands.map(command => command.typeId === "whiteboard.updateElement" ? {
          ...command, payload: { ...command.payload, attrs: { ...elements.find(element => element.id === command.payload.elementId)?.attrs, ...command.payload.attrs as Record<string, unknown> } },
        } : command);
        await sdk.submit({ artifactId: id, baseRevision: latest.artifact.revision, actorId: "whiteboard-web", origin: "local", commands: merged });
        await refresh(); return true;
      } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); await refresh().catch(() => {}); return false; }
      finally { pending.current--; setSaving(pending.current > 0); }
    });
    queue.current = job.then(() => {});
    return job;
  };
  const updateText = (patch: Record<string, unknown>) => {
    if (!selected || !textSelected || Object.entries(patch).every(([key, value]) => selected.attrs[key] === value)) return;
    void submit([{ typeId: "whiteboard.updateElement", payload: { type: "updateElement", elementId: selected.id, attrs: { text, ...patch } } }]);
  };
  const addText = async () => {
    if (!model) return;
    const element: SceneElement = { id: crypto.randomUUID(), typeId: "whiteboard.text", transform: { x: 80 + model.elements.length * 24, y: 80 + model.elements.length * 32, width: 320, height: 160, rotation: 0 }, attrs: { text: "双击编辑文字", fontFamily: '"Noto Sans SC", sans-serif', fontSize: 24 }, children: [] };
    if (await submit([{ typeId: "whiteboard.addElement", payload: { type: "addElement", element, index: model.elements.length } }])) setSelectedId(element.id);
  };
  return <main className="wb-studio" aria-label="白板编辑器">
    <header><button onClick={onBack}>‹ 所有文件</button><strong>{title || "未命名白板"}</strong><span>{saving ? "保存中…" : error ? "保存失败" : model ? "已保存" : "加载中…"}</span></header>
    <div role="toolbar" aria-label="白板文字工具栏"><button disabled={saving || !model} onClick={() => void addText()}>添加文字</button><FontPicker disabled={saving || !textSelected} value={typeof selected?.attrs.fontFamily === "string" ? selected.attrs.fontFamily : ""} onChange={fontFamily => updateText({ fontFamily })} /><label>字号<select aria-label="字号" disabled={saving || !textSelected} value={Number(selected?.attrs.fontSize) || 24} onChange={event => updateText({ fontSize: Number(event.target.value) })}>{[12,16,18,24,32,48,64,96].map(size => <option key={size}>{size}</option>)}</select></label><button disabled={saving || !selected} onClick={() => { if (selected) void submit([{ typeId: "whiteboard.deleteElement", payload: { type: "deleteElement", elementId: selected.id } }]); }}>删除</button></div>
    {error && <p role="alert">{error}</p>}
    <div className="wb-canvas" aria-label="白板画布" onPointerDown={event => { if (event.target === event.currentTarget) { updateText({ text }); setSelectedId(null); } }}>
      {!model && !error && <p>正在加载白板…</p>}
      <div className="wb-scene" style={{ transform: `translate(${model?.camera.x ?? 0}px, ${model?.camera.y ?? 0}px) scale(${model?.camera.scale ?? 1})` }}>
        {model?.elements.map(element => <div key={element.id} data-element-id={element.id} className={`wb-element${selectedId === element.id ? " is-selected" : ""}`} style={{ left: element.transform.x, top: element.transform.y, width: element.transform.width, height: element.transform.height, transform: `rotate(${element.transform.rotation}deg)`, fontFamily: typeof element.attrs.fontFamily === "string" ? element.attrs.fontFamily : undefined, fontSize: Number(element.attrs.fontSize) || 24 }} onClick={() => setSelectedId(element.id)}>
          {element.typeId === "whiteboard.text" ? selectedId === element.id ? <textarea aria-label="白板文字" value={text} disabled={saving} onChange={event => setText(event.target.value)} onBlur={() => updateText({ text })} /> : <span>{String(element.attrs.text ?? "")}</span> : <span>暂不支持的对象：{element.typeId}</span>}
        </div>)}
      </div>
    </div>
  </main>;
}
