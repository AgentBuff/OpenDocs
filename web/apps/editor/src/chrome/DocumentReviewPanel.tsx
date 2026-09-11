import { useCallback, useEffect, useRef, useState } from "react";

import type {
  DocumentPresenceParticipant,
  DocumentReviewAnchor,
  DocumentReviewThread,
} from "@open-office/schema/api";

import { api } from "../api.js";
import type { BlockSessionApi } from "../hooks/useBlockSession.js";

interface Props {
  artifactId: string;
  session: BlockSessionApi;
  selectionAnchor: DocumentReviewAnchor | null;
  onClose: () => void;
}

export function DocumentReviewPanel({ artifactId, session, selectionAnchor, onClose }: Props) {
  const [threads, setThreads] = useState<DocumentReviewThread[]>([]);
  const [remote, setRemote] = useState<DocumentPresenceParticipant[]>([]);
  const [body, setBody] = useState("");
  const [replacement, setReplacement] = useState("");
  const [replies, setReplies] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const presenceSessionId = useRef(`document-${randomId()}`.replace(/[^A-Za-z0-9_-]/g, "_"));
  const capturedAnchor = useRef<DocumentReviewAnchor | null>(selectionAnchor ?? activeCaretAnchor(session));
  const [selectedText, setSelectedText] = useState(() => textForAnchor(session, capturedAnchor.current));

  const refresh = useCallback(async () => {
    const [reviews, presence] = await Promise.all([
      api.documentReviews(artifactId),
      api.documentPresence(artifactId),
    ]);
    setThreads(reviews.threads);
    setRemote(presence.participants.filter((participant) => participant.sessionId !== presenceSessionId.current));
  }, [artifactId]);

  useEffect(() => {
    let disposed = false;
    const read = () => void refresh().catch((error) => {
      if (!disposed) session.reportError(error);
    });
    read();
    const timer = window.setInterval(read, 2_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [refresh, session.reportError]);

  useEffect(() => {
    if (!selectionAnchor) return;
    capturedAnchor.current = selectionAnchor;
    setSelectedText(textForAnchor(session, selectionAnchor));
  }, [selectionAnchor, session]);

  useEffect(() => {
    const publish = () => {
      const anchor = capturedAnchor.current ?? activeCaretAnchor(session);
      if (!anchor) return;
      void api.updateDocumentPresence(artifactId, presenceSessionId.current, {
        revision: session.state.revision,
        blockId: anchor.blockId,
        selectedNodeIds: [],
        selection: {
          anchor: { blockId: anchor.blockId, offset: anchor.start },
          focus: { blockId: anchor.blockId, offset: anchor.end },
        },
      }).catch(() => undefined);
    };
    publish();
    const timer = window.setInterval(publish, 10_000);
    return () => {
      window.clearInterval(timer);
    };
  }, [artifactId, selectionAnchor, session, session.state.revision]);

  useEffect(() => {
    const marked = new Set<string>();
    for (const participant of remote) {
      const blockId = participant.selection?.focus.blockId ?? participant.blockId;
      if (!blockId) continue;
      const row = document.querySelector<HTMLElement>(`[data-block-id="${CSS.escape(blockId)}"]`);
      if (!row) continue;
      marked.add(blockId);
      row.dataset.remotePresence = participant.displayName;
    }
    return () => {
      for (const blockId of marked) {
        document.querySelector<HTMLElement>(`[data-block-id="${CSS.escape(blockId)}"]`)?.removeAttribute("data-remote-presence");
      }
    };
  }, [remote]);

  const create = useCallback(async (kind: "comment" | "suggestion") => {
    const anchor = capturedAnchor.current ?? activeCaretAnchor(session);
    if (!anchor || !body.trim()) return;
    const submittedBody = body.trim();
    const submittedReplacement = replacement.trim();
    const originalText = (() => {
      const block = session.projection.getBlock(anchor.blockId);
      return block?.content?.text ? scalarSlice(block.content.text, anchor.start, anchor.end) : "";
    })();
    if (kind === "suggestion" && (!originalText || !submittedReplacement)) return;
    setBusy(true);
    try {
      const base = {
        threadId: randomId(),
        messageId: randomId(),
        anchor,
        body: submittedBody,
        mentions: extractMentions(submittedBody),
      };
      const page = kind === "comment"
        ? await api.createDocumentReview(artifactId, base)
        : await api.createDocumentSuggestion(artifactId, {
            ...base,
            suggestion: { originalText, replacement: submittedReplacement },
          });
      setThreads(page.threads);
      setBody((current) => current.trim() === submittedBody ? "" : current);
      setReplacement((current) => current.trim() === submittedReplacement ? "" : current);
    } catch (error) {
      session.reportError(error);
    } finally {
      setBusy(false);
    }
  }, [artifactId, body, replacement, session]);

  const updateState = useCallback(async (thread: DocumentReviewThread, state: DocumentReviewThread["state"]) => {
    if (state === "accepted") {
      if (!thread.anchor || !thread.suggestion) return;
    }
    try {
      await api.updateDocumentReview(artifactId, thread.threadId, state);
      if (state === "accepted") await session.reload();
      await refresh();
    } catch (error) {
      session.reportError(error);
    }
  }, [artifactId, refresh, session]);

  const reply = useCallback(async (thread: DocumentReviewThread) => {
    const value = replies[thread.threadId]?.trim();
    if (!value) return;
    try {
      await api.replyDocumentReview(artifactId, thread.threadId, {
        messageId: randomId(),
        body: value,
        mentions: extractMentions(value),
      });
      setReplies((current) => ({ ...current, [thread.threadId]: "" }));
      await refresh();
    } catch (error) {
      session.reportError(error);
    }
  }, [artifactId, refresh, replies, session.reportError]);

  return (
    <aside className="document-review" aria-label="文档审阅">
      <div className="document-review__head">
        <strong>审阅</strong>
        <button className="btn btn--ghost btn--sm" type="button" onClick={onClose}>关闭</button>
      </div>
      {remote.length > 0 && (
        <p className="document-review__presence" aria-live="polite">
          在线：{remote.map((participant) => participant.displayName).join("、")}
        </p>
      )}
      <div className="document-review__composer">
        <small>{selectedText ? `已选择“${selectedText}”` : "请先在一个文本块内选择文字或放置光标"}</small>
        <textarea value={body} onChange={(event) => setBody(event.target.value)} placeholder="评论内容，可使用 @用户ID" />
        <input value={replacement} onChange={(event) => setReplacement(event.target.value)} placeholder="建议替换为…" />
        <div>
          <button className="btn btn--secondary btn--sm" disabled={busy || !body.trim()} onClick={() => void create("comment")}>添加评论</button>
          <button className="btn btn--primary btn--sm" disabled={busy || !body.trim() || !replacement.trim()} onClick={() => void create("suggestion")}>提出建议</button>
        </div>
      </div>
      <ol className="document-review__threads">
        {threads.map((thread) => (
          <li key={thread.threadId} data-review-thread-id={thread.threadId}>
            <div className="document-review__thread-title">
              <strong>{thread.kind === "suggestion" ? "建议" : "评论"}</strong>
              <span>{thread.anchorState === "detached" ? "引用已失效" : thread.anchorState === "stale" ? "基于旧版本" : "当前版本"}</span>
            </div>
            {thread.messages.map((message) => <p key={message.messageId}>{message.body}</p>)}
            {thread.suggestion && <p className="document-review__diff"><del>{thread.suggestion.originalText}</del> → <ins>{thread.suggestion.replacement}</ins></p>}
            <div className="document-review__reply">
              <input
                aria-label={`回复 ${thread.threadId}`}
                value={replies[thread.threadId] ?? ""}
                onChange={(event) => setReplies((current) => ({ ...current, [thread.threadId]: event.target.value }))}
                placeholder="回复，可 @用户ID"
              />
              <button className="btn btn--ghost btn--sm" disabled={!replies[thread.threadId]?.trim()} onClick={() => void reply(thread)}>回复</button>
            </div>
            {thread.state === "open" && thread.anchorState !== "detached" && (
              <div className="document-review__actions">
                {thread.kind === "comment" ? (
                  <button className="btn btn--ghost btn--sm" onClick={() => void updateState(thread, "resolved")}>解决</button>
                ) : (
                  <>
                    <button className="btn btn--primary btn--sm" onClick={() => void updateState(thread, "accepted")}>接受</button>
                    <button className="btn btn--ghost btn--sm" onClick={() => void updateState(thread, "rejected")}>拒绝</button>
                  </>
                )}
              </div>
            )}
            {thread.state !== "open" && <small>状态：{thread.state}</small>}
          </li>
        ))}
        {threads.length === 0 && <li className="document-review__empty">暂无评论或建议</li>}
      </ol>
    </aside>
  );
}

function activeCaretAnchor(session: BlockSessionApi): DocumentReviewAnchor | null {
  const blockId = session.state.activeBlockId;
  return blockId ? { blockId, start: 0, end: 0, revision: session.state.revision } : null;
}

function textForAnchor(session: BlockSessionApi, anchor: DocumentReviewAnchor | null): string {
  if (!anchor) return "";
  const block = session.projection.getBlock(anchor.blockId);
  return block?.content?.text ? scalarSlice(block.content.text, anchor.start, anchor.end) : "";
}

function scalarSlice(value: string, start: number, end: number): string {
  return Array.from(value).slice(start, end).join("");
}

function extractMentions(value: string): string[] {
  return [...new Set([...value.matchAll(/@([A-Za-z0-9_.-]{1,128})/g)].map((match) => match[1]))].slice(0, 32);
}

function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
