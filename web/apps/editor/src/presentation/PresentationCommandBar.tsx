import { FontPicker } from "../typography/FontPicker.js";
import { type ReactNode, useState } from "react";

export interface PresentationCommandBarProps {
  readonly fontFamily?: string; readonly canSetFont?: boolean; readonly onFontFamily?: (value: string) => void;
  readonly hasSlide: boolean; readonly saving: boolean; readonly canUndo: boolean; readonly canRedo: boolean;
  readonly availableCapabilities: ReadonlySet<string>;
  readonly onUndo: () => void; readonly onRedo: () => void; readonly onCreateSlide: () => void;
  readonly onDuplicateSlide: () => void; readonly onInsertText: () => void;
  readonly onInsertShape: (geometry: "rectangle" | "ellipse" | "line" | "arrow") => void;
  readonly onInsertConnector: () => void; readonly onInsertChart: () => void; readonly onInsertImage: () => void;
  readonly onPlay: () => void; readonly onPresenter: () => void; readonly onOpenDeckInspector: () => void;
  readonly exportHref?: string;
  readonly onMoveSlideBackward: () => void; readonly onMoveSlideForward: () => void; readonly onDeleteSlide: () => void;
  readonly canMoveSlideBackward: boolean; readonly canMoveSlideForward: boolean;
}

export function resolvePresentationCommandBarAvailability(hasSlide: boolean, availableCapabilities: ReadonlySet<string>) {
  const has = (typeId: string) => availableCapabilities.has(typeId);
  const canInsert = hasSlide && has("presentation.insertNode");
  return {
    history: has("presentation.history"), createSlide: has("presentation.createSlide"),
    duplicateSlide: hasSlide && has("presentation.duplicateSlide"), moveSlide: hasSlide && has("presentation.moveSlide"),
    deleteSlide: hasSlide && has("presentation.deleteSlide"), insert: canInsert,
    insertChart: canInsert && has("presentation.setChartSpec"), insertImage: canInsert && has("presentation.registerAsset"),
    deckSettings: has("presentation.setPageSpec") || has("presentation.setTheme"), play: hasSlide,
  } as const;
}

type GlyphName = "undo" | "redo" | "plus" | "copy" | "text" | "shape" | "image" | "audio" | "video" | "table" | "link" | "formula" | "chart" | "page" | "folder" | "cloud" | "comment" | "arrange" | "effect" | "animation" | "beauty" | "settings" | "pdf" | "plugin" | "print" | "download" | "search" | "trash" | "play" | "review" | "view";

function Glyph({ name }: { name: GlyphName }) {
  const line = { fill: "none", stroke: "currentColor", strokeWidth: 1.5, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  const shapes: Record<GlyphName, ReactNode> = {
    undo: <path {...line} d="M7 4 3.5 7.5 7 11M4 7.5h6a4 4 0 1 1 0 8H8" />,
    redo: <path {...line} d="m11 4 3.5 3.5L11 11m3-3.5H8a4 4 0 1 0 0 8h2" />,
    plus: <><circle {...line} cx="9" cy="9" r="6" /><path {...line} d="M9 6v6M6 9h6" /></>,
    copy: <><rect {...line} x="6" y="5" width="8" height="9" rx="1" /><path {...line} d="M4 12V4h8" /></>,
    text: <><rect {...line} x="3" y="4" width="12" height="10" rx="1" /><path {...line} d="M6 7h6M9 7v5" /></>,
    shape: <><circle {...line} cx="7" cy="7" r="3" /><rect {...line} x="9" y="9" width="6" height="6" rx="1" /></>,
    image: <><rect {...line} x="2.5" y="3" width="13" height="12" rx="1" /><circle {...line} cx="6" cy="7" r="1" /><path {...line} d="m3 13 3.5-3.5 2.5 2 2-1.8 4 3.3" /></>,
    audio: <><path {...line} d="M7 13V5l7-2v8" /><circle {...line} cx="5" cy="13" r="2" /><circle {...line} cx="12" cy="11" r="2" /></>,
    video: <><rect {...line} x="2.5" y="4" width="13" height="10" rx="1" /><path {...line} d="m7 7 4 2-4 2z" /></>,
    table: <><rect {...line} x="2.5" y="3" width="13" height="12" rx="1" /><path {...line} d="M2.5 7h13M2.5 11h13M7 3v12M11 3v12" /></>,
    link: <><path {...line} d="m7.5 11.5-1 1a2.5 2.5 0 0 1-3.5-3.5l2.5-2.5A2.5 2.5 0 0 1 9 6" /><path {...line} d="m10.5 6.5 1-1A2.5 2.5 0 1 1 15 9l-2.5 2.5A2.5 2.5 0 0 1 9 12" /></>,
    formula: <path {...line} d="M4 4h10M5 14l5-10M7 9h5" />,
    chart: <path {...line} d="M3 15V3m0 12h12M6 12V9m3 3V5m3 7V7" />,
    page: <><rect {...line} x="4" y="2.5" width="10" height="13" rx="1" /><path {...line} d="M6.5 6h5M6.5 9h5M6.5 12h3" /></>,
    folder: <path {...line} d="M2.5 5h5l1.5 2h6.5v7.5h-13z" />,
    cloud: <path {...line} d="M5 14h8a3 3 0 0 0 .3-6A4.5 4.5 0 0 0 5 7a3.5 3.5 0 0 0 0 7Z" />,
    comment: <path {...line} d="M3 3.5h12v8H8l-3.5 3v-3H3z" />,
    arrange: <><rect {...line} x="3" y="3" width="6" height="6" /><rect {...line} x="9" y="9" width="6" height="6" /></>,
    effect: <><path {...line} d="m9 2.5 1.6 4 4 .5-3 2.7.9 4.1L9 11.7l-3.5 2.1.9-4.1L3.4 7l4-.5z" /></>,
    animation: <><path {...line} d="M9 2.5v13M2.5 9h13" /><path {...line} d="m5 5 8 8M13 5l-8 8" /></>,
    beauty: <><path {...line} d="m4 14 8-8" /><path {...line} d="m10.5 3.5.7-1.5.7 1.5 1.5.7-1.5.7-.7 1.5-.7-1.5-1.5-.7Z" /></>,
    settings: <><circle {...line} cx="9" cy="9" r="2.5" /><path {...line} d="M9 2.5v2M9 13.5v2M2.5 9h2M13.5 9h2M4.4 4.4l1.4 1.4m6.4 6.4 1.4 1.4m0-9.2-1.4 1.4m-6.4 6.4-1.4 1.4" /></>,
    pdf: <><path {...line} d="M5 2.5h5l3 3V15H5zM10 2.5v3h3" /><path {...line} d="M6.5 11h5" /></>,
    plugin: <path {...line} d="M7 2.5h4v3h3v4h-3v3H7v-3H4v-4h3z" />,
    print: <path {...line} d="M5 6V3h8v3m-8 6H3V7h12v5h-2m-8-2h8v5H5z" />,
    download: <path {...line} d="M9 2.5v9M5.5 8 9 11.5 12.5 8M3 15h12" />,
    search: <><circle {...line} cx="7.5" cy="7.5" r="4" /><path {...line} d="m10.5 10.5 4 4" /></>,
    trash: <path {...line} d="M4.5 5h9l-.7 10H5.2zM3 5h12M7 2.8h4" />,
    play: <><circle {...line} cx="9" cy="9" r="6" /><path {...line} d="m7.5 6.5 4 2.5-4 2.5z" /></>,
    review: <><path {...line} d="M4 3h10v12H4z" /><path {...line} d="m6 9 2 2 4-5" /></>,
    view: <><path {...line} d="M2.5 9s2.3-4 6.5-4 6.5 4 6.5 4-2.3 4-6.5 4-6.5-4-6.5-4Z" /><circle {...line} cx="9" cy="9" r="1.5" /></>,
  };
  return <svg viewBox="0 0 18 18" aria-hidden="true">{shapes[name]}</svg>;
}

type ToolProps = { label: string; ariaLabel?: string; icon: GlyphName; menu?: boolean; disabled?: boolean; onClick?: () => void; compact?: boolean; href?: string; download?: boolean };
function Tool({ label, ariaLabel, icon, menu, disabled, onClick, compact, href, download }: ToolProps) {
  const accessibleName = ariaLabel ?? label;
  if (href && !disabled) return <a className={`pptr__tool${compact ? " is-compact" : ""}`} title={accessibleName} aria-label={accessibleName} href={href} download={download}><Glyph name={icon} /><span>{label}</span>{menu && <i />}</a>;
  return <button type="button" className={`pptr__tool${compact ? " is-compact" : ""}`} title={accessibleName} aria-label={accessibleName} aria-haspopup={menu ? "menu" : undefined} disabled={disabled} onClick={onClick}><Glyph name={icon} /><span className={accessibleName === "插入" ? "presentation-command-bar__insert-label" : undefined}>{label}</span>{menu && <i />}</button>;
}
function Divider() { return <span className="pptr__divider" aria-hidden="true" />; }
function SelectLike({ children, wide }: { children: ReactNode; wide?: boolean }) { return <button type="button" className={`pptr__select${wide ? " is-wide" : ""}`}>{children}<i /></button>; }

function StartPanel({ props, enabled }: { props: PresentationCommandBarProps; enabled: ReturnType<typeof resolvePresentationCommandBarAvailability> }) {
  return <div className="pptr__panel pptr__panel--start" role="toolbar" aria-label="开始工具栏">
    {enabled.history && <><div className="pptr__history"><Tool label="撤销" icon="undo" compact disabled={!props.canUndo || props.saving} onClick={props.onUndo} /><Tool label="重做" icon="redo" compact disabled={!props.canRedo || props.saving} onClick={props.onRedo} /></div><Divider /></>}
    <div className="pptr__slide-actions" aria-label="幻灯片操作">
      {enabled.createSlide && <Tool label={props.hasSlide ? "新建幻灯片" : "创建首张幻灯片"} ariaLabel={props.hasSlide ? "新建幻灯片" : "创建首张幻灯片"} icon="plus" compact disabled={props.saving} onClick={props.onCreateSlide} />}
      {enabled.duplicateSlide && <Tool label="复制" ariaLabel="复制当前幻灯片" icon="copy" compact disabled={props.saving} onClick={props.onDuplicateSlide} />}
      {enabled.moveSlide && <><Divider /><Tool label="上移" ariaLabel="上移幻灯片" icon="arrange" compact disabled={!props.canMoveSlideBackward || props.saving} onClick={props.onMoveSlideBackward} /><Tool label="下移" ariaLabel="下移幻灯片" icon="arrange" compact disabled={!props.canMoveSlideForward || props.saving} onClick={props.onMoveSlideForward} /></>}
      {enabled.deleteSlide && <><span className="presentation-command-bar__group-separator" /><Tool label="删除" ariaLabel="删除当前幻灯片" icon="trash" compact disabled={props.saving} onClick={props.onDeleteSlide} /></>}
    </div>
    {enabled.insert && <><Divider /><Tool label="插入" ariaLabel="插入" icon="plus" menu disabled={props.saving} /><Tool label="文本框" icon="text" menu disabled={props.saving} onClick={props.onInsertText} /><Tool label="形状" icon="shape" menu disabled={props.saving} onClick={() => props.onInsertShape("rectangle")} /></>}<Divider />
    <div className="pptr__type"><div><FontPicker value={props.fontFamily ?? ""} disabled={props.saving || !props.canSetFont} onChange={value => props.onFontFamily?.(value)} /><SelectLike>字号</SelectLike><button>A⁺</button><button>A⁻</button></div><div><button><b>B</b></button><button><i>I</i></button><button><u>U</u></button><button><s>S</s></button><button>x²</button><button>x₂</button><button>AV</button><button className="pptr__mark">▰</button><button className="pptr__font-color">A</button></div></div><Divider />
    <div className="pptr__paragraph"><div><button>☷⌄</button><button>☷⌄</button><button>≡</button><button>≣</button><button>☰</button></div><div><button>≡</button><button>≣</button><button>☰</button><button>↕</button><button>↔</button></div></div><Divider />
    <Tool label="排列" icon="arrange" menu /><Tool label="效果" icon="effect" menu disabled /><Tool label="填充颜色" icon="beauty" menu disabled /><Divider />
    <Tool label="动画" icon="animation" /><Divider /><Tool label="快捷美化" icon="beauty" menu /><Divider />
    {enabled.deckSettings && <><Tool label="页面设置" ariaLabel="设计：页面比例与主题" icon="settings" menu disabled={props.saving} onClick={props.onOpenDeckInspector} /><Divider /></>}
    <Tool label="PDF转换" icon="pdf" menu /><Tool label="生成图片" icon="image" /><Tool label="插件" icon="plugin" menu /><Tool label="打印" icon="print" /><Tool label="下载 PPTX" icon="download" href={props.exportHref} download disabled={!props.hasSlide || props.saving} />
    {enabled.play && <Tool label="播放" ariaLabel="播放演示" icon="play" compact disabled={props.saving} onClick={props.onPlay} />}<span className="pptr__spacer" /><Tool label="搜索" icon="search" compact />
  </div>;
}

function InsertPanel({ props, enabled }: { props: PresentationCommandBarProps; enabled: ReturnType<typeof resolvePresentationCommandBarAvailability> }) {
  return <div className="pptr__panel" role="toolbar" aria-label="插入工具栏">
    <Tool label="空白页面" icon="page" onClick={props.onCreateSlide} disabled={!enabled.createSlide || props.saving} /><Tool label="模板页面" icon="table" menu /><Tool label="AI页面" icon="beauty" /><Divider />
    <Tool label="文本框" icon="text" menu onClick={props.onInsertText} disabled={!enabled.insert || props.saving} /><Tool label="形状" icon="shape" menu onClick={() => props.onInsertShape("rectangle")} disabled={!enabled.insert || props.saving} /><Tool label="图片" icon="image" menu onClick={props.onInsertImage} disabled={!enabled.insertImage || props.saving} /><Tool label="音频" icon="audio" /><Tool label="视频" icon="video" /><Tool label="表格" icon="table" menu /><Tool label="链接" icon="link" /><Tool label="LaTeX公式" icon="formula" /><Tool label="图表" icon="chart" menu onClick={props.onInsertChart} disabled={!enabled.insertChart || props.saving} /><Tool label="页脚" icon="page" /><Divider />
    <Tool label="腾讯文档" icon="page" /><Tool label="本地文件" icon="folder" /><Tool label="微云文件" icon="cloud" /><Tool label="第三方内容" icon="plugin" menu /><Divider /><Tool label="批注" icon="comment" />
  </div>;
}

function Palette({ colors }: { colors: string[] }) { return <button type="button" className="pptr__palette">{colors.map((color) => <i key={color} style={{ backgroundColor: color }} />)}</button>; }
function BeautyPanel() {
  const themes = ["linear-gradient(135deg,#1553aa,#36a3ff)", "linear-gradient(135deg,#ff633c,#ffb12d)", "linear-gradient(135deg,#174b42,#94d5bd)", "linear-gradient(135deg,#f2d966,#42bc78)", "linear-gradient(135deg,#172544,#6238d8)"];
  return <div className="pptr__panel pptr__panel--beauty" role="toolbar" aria-label="美化工具栏"><div className="pptr__templates">{themes.map((background, index) => <button key={background} style={{ background }} aria-label={`模板 ${index + 1}`} />)}</div><Tool label="更多模板" icon="page" /><Divider /><div className="pptr__palettes"><Palette colors={["#3468d4","#1cb8d0","#25a87b","#f2bc2e","#d84848","#8a45c8"]} /><Palette colors={["#303b4a","#8390a1","#d6dbe1","#bf6b2f","#efa52e","#4f8fd3"]} /><Palette colors={["#27345b","#6e89b8","#b4c6e4","#87481d","#c98637","#ead3af"]} /></div><Tool label="更多配色" icon="beauty" /><Divider /><Tool label="排版检查" icon="review" /><Tool label="统一字体" icon="text" /><Tool label="图片处理" icon="image" menu /><Divider /><Tool label="单页大纲" icon="page" /><Tool label="单页美化" icon="beauty" /><Divider /><Tool label="幻灯片比例" icon="page" menu /><Tool label="设置背景" icon="effect" /><Tool label="母版编辑" icon="page" /></div>;
}

function SimplePanel({ tab, props }: { tab: string; props: PresentationCommandBarProps }) {
  const tools: Record<string, ToolProps[]> = {
    切换: [{ label: "无切换", icon: "page" }, { label: "淡化", icon: "effect" }, { label: "推进", icon: "animation" }],
    动画: [{ label: "添加动画", icon: "animation" }, { label: "动画窗格", icon: "page" }, { label: "播放预览", icon: "play" }],
    放映: [{ label: "从头播放", icon: "play", onClick: props.onPlay }, { label: "演讲者视图", icon: "view", onClick: props.onPresenter }, { label: "放映设置", icon: "settings" }],
    审阅: [{ label: "批注", icon: "comment" }, { label: "修订", icon: "review" }, { label: "保护文稿", icon: "settings" }],
    视图: [{ label: "普通视图", icon: "view" }, { label: "幻灯片浏览", icon: "table" }, { label: "显示标尺", icon: "page" }],
    效率工具: [{ label: "批量美化", icon: "beauty" }, { label: "图片处理", icon: "image" }, { label: "文档转换", icon: "pdf" }],
    会员专享: [{ label: "AI生成", icon: "beauty" }, { label: "高级模板", icon: "page" }, { label: "品牌套件", icon: "settings" }],
  };
  return <div className="pptr__panel" role="toolbar" aria-label={`${tab}工具栏`}>{(tools[tab] ?? []).map((tool) => <Tool key={tool.label} {...tool} />)}</div>;
}

const TABS = ["开始", "插入", "美化", "切换", "动画", "放映", "审阅", "视图", "效率工具", "会员专享"] as const;

export function PresentationCommandBar(props: PresentationCommandBarProps) {
  const [tab, setTab] = useState<(typeof TABS)[number]>("开始");
  const enabled = resolvePresentationCommandBarAvailability(props.hasSlide, props.availableCapabilities);
  return <section className="pptr" aria-label="演示文稿功能区"><div className="pptr__tabs" role="tablist" aria-label="演示文稿标签页">{TABS.map((label) => <button key={label} type="button" role="tab" aria-selected={tab === label} className={tab === label ? "is-active" : ""} onClick={() => setTab(label)}>{label}</button>)}</div>{tab === "开始" ? <StartPanel props={props} enabled={enabled} /> : tab === "插入" ? <InsertPanel props={props} enabled={enabled} /> : tab === "美化" ? <BeautyPanel /> : <SimplePanel tab={tab} props={props} />}</section>;
}
