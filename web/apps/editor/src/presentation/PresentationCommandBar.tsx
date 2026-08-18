import { useMemo, useState } from "react";
import {
  Dropdown,
  Icon,
  MenuItem,
  MenuPanel,
  MenuSeparator,
  MenuSectionTitle,
  Toolbar,
  ToolbarButton,
  ToolbarGroup,
  ToolbarMenuButton,
  ToolbarSeparator,
} from "@open-office/ui";
import { capabilitiesForSurface } from "@open-office/presentation-ui";

export interface PresentationCommandBarProps {
  readonly hasSlide: boolean;
  readonly saving: boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  readonly availableCapabilities: ReadonlySet<string>;
  readonly onUndo: () => void;
  readonly onRedo: () => void;
  readonly onCreateSlide: () => void;
  readonly onDuplicateSlide: () => void;
  readonly onInsertText: () => void;
  readonly onInsertShape: (geometry: "rectangle" | "ellipse" | "line" | "arrow") => void;
  readonly onInsertConnector: () => void;
  readonly onInsertChart: () => void;
  readonly onInsertImage: () => void;
  readonly onPlay: () => void;
  readonly onOpenDeckInspector: () => void;
  readonly onMoveSlideBackward: () => void;
  readonly onMoveSlideForward: () => void;
  readonly onDeleteSlide: () => void;
  readonly canMoveSlideBackward: boolean;
  readonly canMoveSlideForward: boolean;
}

/**
 * Resolves the global toolbar from the server's executable capability
 * catalogue.  Keeping this pure prevents the command bar from accidentally
 * rendering a control just because a handler happens to exist in React.
 */
export function resolvePresentationCommandBarAvailability(
  hasSlide: boolean,
  availableCapabilities: ReadonlySet<string>,
) {
  const has = (typeId: string) => availableCapabilities.has(typeId);
  const canInsert = hasSlide && has("presentation.insertNode");

  return {
    history: has("presentation.history"),
    createSlide: has("presentation.createSlide"),
    duplicateSlide: hasSlide && has("presentation.duplicateSlide"),
    moveSlide: hasSlide && has("presentation.moveSlide"),
    deleteSlide: hasSlide && has("presentation.deleteSlide"),
    insert: canInsert,
    insertChart: canInsert && has("presentation.setChartSpec"),
    insertImage: canInsert && has("presentation.registerAsset"),
    deckSettings: has("presentation.setPageSpec") || has("presentation.setTheme"),
    play: hasSlide,
  } as const;
}

/**
 * Global/insert control surface. Its contents are capability-filtered; object
 * controls live in their own contextual toolbar and never leak into this bar.
 */
export function PresentationCommandBar(props: PresentationCommandBarProps) {
  const [insertOpen, setInsertOpen] = useState(false);
  const insertItems = useMemo(
    () => capabilitiesForSurface("insert", props.availableCapabilities, "slide"),
    [props.availableCapabilities],
  );
  const availability = resolvePresentationCommandBarAvailability(props.hasSlide, props.availableCapabilities);
  // Preserve the UI catalogue as a second guard: an action must be both
  // advertised by the server and registered for the insert/slide surface.
  const canInsert = availability.insert && insertItems.some((item) => item.typeId === "presentation.insertNode");
  const canInsertImage = canInsert && availability.insertImage;
  const canInsertChart = availability.insertChart;
  const showSlideGroup = availability.createSlide || availability.duplicateSlide || availability.moveSlide || availability.deleteSlide;
  const closeThen = (action: () => void) => {
    setInsertOpen(false);
    action();
  };

  return (
    <Toolbar className="presentation-command-bar" density="comfortable" aria-label="演示文稿工具栏" aria-busy={props.saving || undefined}>
      {availability.history && (
        <ToolbarGroup aria-label="历史操作">
          <ToolbarButton aria-label="撤销" title="撤销" disabled={!props.canUndo || props.saving} onClick={props.onUndo}>
            <Icon name="undo" />
          </ToolbarButton>
          <ToolbarButton aria-label="重做" title="重做" disabled={!props.canRedo || props.saving} onClick={props.onRedo}>
            <Icon name="redo" />
          </ToolbarButton>
        </ToolbarGroup>
      )}

      {availability.history && showSlideGroup && <ToolbarSeparator />}
      {showSlideGroup && <ToolbarGroup aria-label="幻灯片操作" className="presentation-command-bar__slide-group">
        {availability.createSlide && <ToolbarButton aria-label={props.hasSlide ? "新建幻灯片" : "创建首张幻灯片"} disabled={props.saving} onClick={props.onCreateSlide} title={props.hasSlide ? "新建幻灯片" : "创建首张幻灯片"}>
          <Icon name="plus" />
          <span className="oo-toolbar__item-label">{props.hasSlide ? "新建幻灯片" : "创建首张幻灯片"}</span>
        </ToolbarButton>}
        {availability.duplicateSlide && <ToolbarButton aria-label="复制当前幻灯片" title="复制当前幻灯片" disabled={props.saving} onClick={props.onDuplicateSlide}>
          <Icon name="copy" />
          <span className="oo-toolbar__item-label">复制</span>
        </ToolbarButton>}
        {availability.moveSlide && <>
          {(availability.createSlide || availability.duplicateSlide) && <ToolbarSeparator className="presentation-command-bar__group-separator" />}
          <ToolbarButton aria-label="上移幻灯片" title="上移幻灯片" disabled={!props.canMoveSlideBackward || props.saving} onClick={props.onMoveSlideBackward}><Icon name="arrow-up" /></ToolbarButton>
          <ToolbarButton aria-label="下移幻灯片" title="下移幻灯片" disabled={!props.canMoveSlideForward || props.saving} onClick={props.onMoveSlideForward}><Icon name="arrow-down" /></ToolbarButton>
        </>}
        {availability.deleteSlide && <>
          {(availability.createSlide || availability.duplicateSlide || availability.moveSlide) && <ToolbarSeparator className="presentation-command-bar__group-separator" />}
          <ToolbarButton aria-label="删除当前幻灯片" title="删除当前幻灯片" tone="danger" disabled={props.saving} onClick={props.onDeleteSlide}><Icon name="delete" /></ToolbarButton>
        </>}
      </ToolbarGroup>
      }

      {showSlideGroup && canInsert && <ToolbarSeparator />}
      {canInsert && <ToolbarGroup aria-label="插入">
        <Dropdown
          open={insertOpen}
          onOpenChange={setInsertOpen}
          placement="bottom-start"
          popupClassName="presentation-command-bar__insert-menu"
          content={(
            <MenuPanel aria-label="插入对象">
              <MenuSectionTitle>内容</MenuSectionTitle>
              <MenuItem icon={<Icon name="text" />} disabled={props.saving} onClick={() => closeThen(props.onInsertText)}>文本框</MenuItem>
              <MenuSeparator />
              <MenuSectionTitle>图形与连接</MenuSectionTitle>
              <MenuItem icon={<Icon name="shape" />} disabled={props.saving} onClick={() => closeThen(() => props.onInsertShape("rectangle"))}>矩形</MenuItem>
              <MenuItem icon={<Icon name="shape" />} disabled={props.saving} onClick={() => closeThen(() => props.onInsertShape("ellipse"))}>圆形</MenuItem>
              <MenuItem icon={<Icon name="shape" />} disabled={props.saving} onClick={() => closeThen(() => props.onInsertShape("line"))}>直线</MenuItem>
              <MenuItem icon={<Icon name="arrow-right" />} disabled={props.saving} onClick={() => closeThen(() => props.onInsertShape("arrow"))}>箭头</MenuItem>
              <MenuItem icon={<Icon name="arrow-right" />} disabled={props.saving} onClick={() => closeThen(props.onInsertConnector)}>连接线</MenuItem>
              {(canInsertChart || canInsertImage) && <>
                <MenuSeparator />
                <MenuSectionTitle>数据与媒体</MenuSectionTitle>
                {canInsertChart && <MenuItem icon={<Icon name="table" />} disabled={props.saving} onClick={() => closeThen(props.onInsertChart)}>图表</MenuItem>}
                {canInsertImage && <MenuItem icon={<Icon name="insert" />} disabled={props.saving} onClick={() => closeThen(props.onInsertImage)}>图片</MenuItem>}
              </>}
            </MenuPanel>
          )}
        >
          <ToolbarMenuButton disabled={props.saving} aria-label="插入" title="插入" aria-haspopup="menu">
            <Icon name="insert" />
            <span className="presentation-command-bar__insert-label">插入</span>
            <Icon name="arrow-down" />
          </ToolbarMenuButton>
        </Dropdown>
      </ToolbarGroup>
      }

      {(showSlideGroup || canInsert) && availability.deckSettings && <ToolbarSeparator />}
      {availability.deckSettings && <ToolbarGroup aria-label="演示文稿设置">
        <ToolbarButton aria-label="设计：页面比例与主题" disabled={props.saving} onClick={props.onOpenDeckInspector} title="页面比例与主题">
          <Icon name="settings" />
          <span className="oo-toolbar__item-label">设计</span>
        </ToolbarButton>
      </ToolbarGroup>}

      {(showSlideGroup || canInsert || availability.deckSettings) && availability.play && <ToolbarSeparator />}
      {availability.play && <ToolbarGroup aria-label="演示">
        <ToolbarButton aria-label="播放演示" disabled={props.saving} onClick={props.onPlay} title="播放演示">
          <Icon name="arrow-right" />
          <span className="oo-toolbar__item-label">播放</span>
        </ToolbarButton>
      </ToolbarGroup>}
    </Toolbar>
  );
}
