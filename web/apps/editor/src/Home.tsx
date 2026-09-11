/**
 * 首页：左侧导航 + 顶部搜索 + 文档表格 + 右侧工具面板。
 *
 * 首页统一管理 Artifact；新建入口可以创建各领域资源，尚未接入专用编辑器的类型会明确提示，
 * 不伪装成可编辑的 Word 文档。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { api, type ArtifactKind, type ArtifactMeta } from "./api.js";
import { ArtifactIcon, DocIcon, StarIcon, ThemeIcon, UploadIcon } from "./icons/index.js";
import { useDismissableLayer, useThemeRuntime } from "@open-office/ui";

const formatSize = (bytes: number) => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
};

/** 今天的显示成时分，更早的显示成月-日 时:分——和常见网盘一致。 */
function formatTime(iso: string): string {
  const date = new Date(iso);
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();

  const pad = (n: number) => String(n).padStart(2, "0");
  const time = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  return sameDay ? time : `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${time}`;
}

type Tab = "recent" | "starred";
type SortKey = "updatedAt" | "title" | "size";

const CREATE_OPTIONS: readonly {
  kind: ArtifactKind;
  label: string;
  description: string;
  group: "professional" | "creative";
}[] = [
  { kind: "document", label: "文档", description: "撰写内容、代码与表格", group: "professional" },
  { kind: "spreadsheet", label: "表格", description: "整理数据与公式", group: "professional" },
  { kind: "presentation", label: "幻灯片", description: "制作幻灯片与提案", group: "professional" },
  { kind: "mindmap", label: "思维导图", description: "梳理想法与结构", group: "creative" },
  { kind: "whiteboard", label: "智能白板", description: "自由绘制与协作", group: "creative" },
];

const KIND_LABEL: Record<ArtifactKind, string> = Object.fromEntries(
  CREATE_OPTIONS.map(({ kind, label }) => [kind, label]),
) as Record<ArtifactKind, string>;

interface Props {
  onOpen: (id: string, title: string, kind: ArtifactKind, warnings?: string[]) => void;
}

export function Home({ onOpen }: Props) {
  const { theme, toggleTheme } = useThemeRuntime();
  const [documents, setDocuments] = useState<ArtifactMeta[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<Tab>("recent");
  const [keyword, setKeyword] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({
    key: "updatedAt",
    desc: true,
  });
  const [toolboxOpen, setToolboxOpen] = useState(true);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [createMenuOpen, setCreateMenuOpen] = useState(false);
  const [createTab, setCreateTab] = useState<"office" | "ai">("office");
  const [importMode, setImportMode] = useState<"audit" | "strict">("audit");
  const fileInput = useRef<HTMLInputElement>(null);

  const refresh = useCallback(async () => {
    try {
      setDocuments(await api.listArtifacts());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const createMenuRef = useRef<HTMLDivElement>(null);
  useDismissableLayer({
    enabled: createMenuOpen,
    rootRef: createMenuRef,
    onDismiss: () => setCreateMenuOpen(false),
  });

  const run = useCallback(
    async (action: () => Promise<void>) => {
      setBusy(true);
      try {
        await action();
        await refresh();
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const handleCreate = useCallback((kind: ArtifactKind = "document") => {
    setCreateMenuOpen(false);
    void run(async () => {
      const meta = await api.create(kind);
      if (kind === "document" || kind === "presentation" || kind === "spreadsheet" || kind === "mindmap") onOpen(meta.id, meta.title, kind);
      else setError(`${KIND_LABEL[kind]}已创建，专用编辑器正在接入中。`);
    });
  }, [onOpen, run]);

  const handleUpload = useCallback(
    (file: File) => {
      void run(async () => {
        const { artifact: meta, warnings } = await api.upload(file, importMode);
        if (meta.kind === "document" || meta.kind === "presentation" || meta.kind === "spreadsheet" || meta.kind === "mindmap") {
          onOpen(meta.id, meta.title, meta.kind, meta.kind === "mindmap" ? warnings : undefined);
        } else {
          setError(`${KIND_LABEL[meta.kind]}已导入，专用编辑器正在接入中。`);
        }
      }).finally(() => {
        // 清空输入框，否则重复选择同一个文件不会触发 change。
        if (fileInput.current) fileInput.current.value = "";
      });
    },
    [importMode, onOpen, run],
  );

  const toggleStar = useCallback(
    (doc: ArtifactMeta) => {
      void run(() => api.patch(doc.id, { starred: !doc.starred }).then(() => undefined));
    },
    [run],
  );

  const rename = useCallback(
    (doc: ArtifactMeta, title: string) => {
      setRenaming(null);
      if (!title.trim() || title === doc.title) return;
      void run(() => api.patch(doc.id, { title }).then(() => undefined));
    },
    [run],
  );

  const remove = useCallback(
    (doc: ArtifactMeta) => {
      void run(() => api.remove(doc.id));
    },
    [run],
  );

  const visible = useMemo(() => {
    const needle = keyword.trim().toLowerCase();
    const filtered = documents.filter((doc) => {
      if (tab === "starred" && !doc.starred) return false;
      return !needle || doc.title.toLowerCase().includes(needle);
    });

    const direction = sort.desc ? -1 : 1;
    return [...filtered].sort((a, b) => {
      switch (sort.key) {
        case "title":
          return a.title.localeCompare(b.title, "zh-CN") * direction;
        case "size":
          return (a.size - b.size) * direction;
        default:
          return (Date.parse(a.updatedAt) - Date.parse(b.updatedAt)) * direction;
      }
    });
  }, [documents, keyword, sort, tab]);

  const totalSize = useMemo(
    () => documents.reduce((sum, doc) => sum + doc.size, 0),
    [documents],
  );

  const sortBy = (key: SortKey) =>
    setSort((current) =>
      current.key === key ? { key, desc: !current.desc } : { key, desc: true },
    );

  const sortMark = (key: SortKey) => (sort.key === key ? (sort.desc ? " ↓" : " ↑") : "");

  return (
    <div className="home">
      <aside className="sidebar">
        <div className="sidebar__brand">
          <span className="sidebar__logo">OO</span>
          open-office
          <button
            className="sidebar__theme-toggle"
            type="button"
            onClick={toggleTheme}
            aria-pressed={theme === "office-dark"}
            aria-label={theme === "office-dark" ? "切换浅色主题" : "切换深色主题"}
            title={theme === "office-dark" ? "切换浅色主题" : "切换深色主题"}
          >
            <ThemeIcon dark={theme === "office-dark"} />
          </button>
        </div>

        <div className="create-menu" ref={createMenuRef}>
          <button
            className="btn btn--primary sidebar__action"
            onClick={() => setCreateMenuOpen((open) => !open)}
            disabled={busy}
            aria-expanded={createMenuOpen}
          >
            ＋ 新建 <span className="create-menu__chevron">⌄</span>
          </button>
          <button
            className="create-menu__toggle"
            type="button"
            aria-label="选择新建类型"
            aria-expanded={createMenuOpen}
            onClick={() => setCreateMenuOpen((open) => !open)}
            disabled={busy}
          >
            ▾
          </button>
          {createMenuOpen && (
            <div className="create-menu__popover" role="dialog" aria-label="新建文件类型">
              <div className="create-menu__tabs" role="tablist" aria-label="创建类型">
                <button
                  className={`create-menu__tab${createTab === "office" ? " is-active" : ""}`}
                  type="button"
                  role="tab"
                  aria-selected={createTab === "office"}
                  aria-controls="create-menu-office-panel"
                  onClick={() => setCreateTab("office")}
                >
                  腾讯文档
                </button>
                <button
                  className={`create-menu__tab${createTab === "ai" ? " is-active" : ""}`}
                  type="button"
                  role="tab"
                  aria-selected={createTab === "ai"}
                  aria-controls="create-menu-ai-panel"
                  onClick={() => setCreateTab("ai")}
                >
                  AI 创作
                </button>
              </div>
              {createTab === "office" ? (
                <div id="create-menu-office-panel" role="tabpanel" aria-label="腾讯文档">
                  <div className="create-menu__section-label">专业文档</div>
                  <div className="create-menu__grid">
                    {CREATE_OPTIONS.filter((option) => option.group === "professional").map((option) => (
                      <button
                        key={option.kind}
                        className="create-menu__item"
                        type="button"
                        role="menuitem"
                        onClick={() => handleCreate(option.kind)}
                        disabled={busy}
                      >
                        <span className={`create-menu__icon create-menu__icon--${option.kind}`}>
                          <ArtifactIcon kind={option.kind} />
                        </span>
                        <span className="create-menu__copy">
                          <strong>{option.label}</strong>
                          <small>{option.description}</small>
                        </span>
                      </button>
                    ))}
                  </div>
                  <div className="create-menu__section-label">创新文档</div>
                  <div className="create-menu__grid">
                    {CREATE_OPTIONS.filter((option) => option.group === "creative").map((option) => (
                      <button
                        key={option.kind}
                        className="create-menu__item"
                        type="button"
                        role="menuitem"
                        onClick={() => handleCreate(option.kind)}
                        disabled={busy}
                      >
                        <span className={`create-menu__icon create-menu__icon--${option.kind}`}>
                          <ArtifactIcon kind={option.kind} />
                        </span>
                        <span className="create-menu__copy">
                          <strong>{option.label}</strong>
                          <small>{option.description}</small>
                        </span>
                      </button>
                    ))}
                  </div>
                  <div className="create-menu__divider" />
                  <div className="create-menu__section-label">更多</div>
                  <div className="create-menu__grid create-menu__grid--more">
                    <button
                      className="create-menu__item"
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setCreateMenuOpen(false);
                        fileInput.current?.click();
                      }}
                      disabled={busy}
                    >
                      <span className="create-menu__icon create-menu__icon--upload"><UploadIcon /></span>
                      <span className="create-menu__copy">
                        <strong>上传文件</strong>
                        <small>Office、Mindmap、Markdown</small>
                      </span>
                    </button>
                  </div>
                </div>
              ) : (
                <div id="create-menu-ai-panel" className="create-menu__empty" role="tabpanel" aria-label="AI 创作">
                  <span className="create-menu__empty-icon">AI</span>
                  <strong>AI 创作能力正在接入</strong>
                  <small>接入后可从这里生成文档、表格和演示内容。</small>
                </div>
              )}
            </div>
          )}
        </div>
        <label className="btn sidebar__action">
          <UploadIcon />
          上传
          <input
            ref={fileInput}
            type="file"
            accept=".docx,.xlsx,.pptx,.mindmap.json,.json,.md,.opmm,.mm,.xmind"
            hidden
            disabled={busy}
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) handleUpload(file);
            }}
          />
        </label>
        <label className="sidebar__import-mode">
          导入策略
          <select aria-label="导入策略" value={importMode} disabled={busy} onChange={(event) => setImportMode(event.target.value as "audit" | "strict")}>
            <option value="audit">兼容导入并报告降级</option>
            <option value="strict">有损内容直接拒绝</option>
          </select>
        </label>

        <nav className="sidebar__nav">
          <button
            className={`sidebar__link${tab === "recent" ? " is-active" : ""}`}
            onClick={() => setTab("recent")}
          >
            <DocIcon /> 全部文件
          </button>
          <button
            className={`sidebar__link${tab === "starred" ? " is-active" : ""}`}
            onClick={() => setTab("starred")}
          >
            <StarIcon filled={false} /> 收藏
          </button>
        </nav>

        <div className="sidebar__footer">
          <div className="sidebar__usage">
            已存 {documents.length} 个文件 · {formatSize(totalSize)}
          </div>
        </div>
      </aside>

      <main className="main">
        <header className="main__search">
          <input
            className="search"
            type="search"
            placeholder="搜索文件"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
          />
          {!toolboxOpen && (
            <button className="btn btn--ghost" onClick={() => setToolboxOpen(true)}>
              工具箱
            </button>
          )}
        </header>

        <div className="main__tabs">
          <button
            className={`tab${tab === "recent" ? " is-active" : ""}`}
            onClick={() => setTab("recent")}
          >
            最近
          </button>
          <button
            className={`tab${tab === "starred" ? " is-active" : ""}`}
            onClick={() => setTab("starred")}
          >
            收藏
          </button>
        </div>

        {error && <p className="alert">{error}</p>}

        <div className="table">
          <div className="table__head">
            <button className="table__col table__col--name" onClick={() => sortBy("title")}>
              名称{sortMark("title")}
            </button>
            <span className="table__col table__col--owner">所有者</span>
            <button className="table__col table__col--time" onClick={() => sortBy("updatedAt")}>
              最近修改{sortMark("updatedAt")}
            </button>
            <button className="table__col table__col--size" onClick={() => sortBy("size")}>
              文件大小{sortMark("size")}
            </button>
            <span className="table__col table__col--actions" />
          </div>

          {visible.length === 0 ? (
            <p className="empty">
              {documents.length === 0
                ? "还没有文件。点「新建」选择类型，或上传 Office、Mindmap JSON / Markdown 文件。"
                : "没有匹配的文件。"}
            </p>
          ) : (
            <ul className="table__body">
              {visible.map((doc) => (
                <li
                  key={doc.id}
                  className={`row row--${doc.kind}`}
                  onClick={() => {
                    if (doc.kind === "document" || doc.kind === "presentation" || doc.kind === "spreadsheet" || doc.kind === "mindmap") onOpen(doc.id, doc.title, doc.kind);
                    else setError(`${KIND_LABEL[doc.kind]}的专用编辑器正在接入中。`);
                  }}
                >
                  <div className="table__col table__col--name">
                    <span className={`row__kind row__kind--${doc.kind}`} aria-hidden="true">
                      <ArtifactIcon kind={doc.kind} />
                    </span>
                    {renaming === doc.id ? (
                      <input
                        className="row__rename"
                        defaultValue={doc.title}
                        autoFocus
                        onClick={(e) => e.stopPropagation()}
                        onBlur={(e) => rename(doc, e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") e.currentTarget.blur();
                          if (e.key === "Escape") setRenaming(null);
                        }}
                      />
                    ) : (
                      <span className="row__title">{doc.title}</span>
                    )}
                    <button
                      className={`row__star${doc.starred ? " is-on" : ""}`}
                      title={doc.starred ? "取消收藏" : "收藏"}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleStar(doc);
                      }}
                    >
                      <StarIcon filled={doc.starred} />
                    </button>
                  </div>
                  <span className="table__col table__col--owner">我</span>
                  <span className="table__col table__col--time">{formatTime(doc.updatedAt)}</span>
                  <span className="table__col table__col--size">{formatSize(doc.size)}</span>
                  <span className="table__col table__col--actions">
                    <button
                      className="btn btn--ghost btn--sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        setRenaming(doc.id);
                      }}
                    >
                      重命名
                    </button>
                    <a
                      className="btn btn--ghost btn--sm"
                      href={`/api/artifacts/${doc.id}/source`}
                      onClick={(e) => e.stopPropagation()}
                    >
                      下载
                    </a>
                    <button
                      className="btn btn--ghost btn--sm btn--danger"
                      onClick={(e) => {
                        e.stopPropagation();
                        remove(doc);
                      }}
                    >
                      删除
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </main>

      {toolboxOpen && (
        <aside className="toolbox">
          <header className="toolbox__head">
            工具箱
            <button
              className="btn btn--ghost btn--sm"
              onClick={() => setToolboxOpen(false)}
              aria-label="收起工具箱"
            >
              ✕
            </button>
          </header>

          <p className="toolbox__group">常用</p>
          <div className="toolbox__grid">
            <button className="tool" onClick={() => handleCreate("document")} disabled={busy}>
              <span className="tool__icon tool__icon--blue">＋</span>
              新建文字文档
            </button>
            <button className="tool" onClick={() => fileInput.current?.click()} disabled={busy}>
              <span className="tool__icon tool__icon--green">↑</span>
              导入文件
            </button>
          </div>

          <p className="toolbox__group">概览</p>
          <dl className="toolbox__stats">
            <div>
              <dt>文件总数</dt>
              <dd>{documents.length}</dd>
            </div>
            <div>
              <dt>已收藏</dt>
              <dd>{documents.filter((d) => d.starred).length}</dd>
            </div>
            <div>
              <dt>占用空间</dt>
              <dd>{formatSize(totalSize)}</dd>
            </div>
          </dl>

          <p className="toolbox__note">
            新建入口已统一支持文字文档、电子表格、演示文稿、思维导图和白板。
          </p>
        </aside>
      )}
    </div>
  );
}
