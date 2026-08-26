import type { DocumentBlock, DocumentBlockKind } from "@open-office/schema/artifact";
import { Icon, Popover } from "@open-office/ui";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import { BlockMenu } from "./BlockMenu.js";

export interface BlockGutterProps {
  block: DocumentBlock;
  emptyTextBlock: boolean;
  menuOpen: boolean;
  session: BlockSessionApi;
  align?: "left" | "center" | "right" | "justify";
  listType: "bullet" | "ordered" | null;
  onMenuOpenChange: (open: boolean) => void;
  onInsert: () => void;
  onDelete: () => void;
  onKind: (kind: DocumentBlockKind) => void;
  onAlignment: (align: "left" | "center" | "right" | "justify") => void;
  onList: (type: "bullet" | "ordered") => void;
  onLink: () => void;
  onInsertTable: (rows: number, columns: number) => void;
  onInsertImage: (file: File) => Promise<void> | void;
  onInsertQuote: () => void;
  onInsertCallout: () => void;
  onInsertTodo: () => void;
  onInsertCode: () => void;
  onDivider: () => void;
}

/**
 * Block-gutter composition only. It deliberately owns no document mutation:
 * every menu choice is provided by BlockNode as a semantic session action.
 */
export function BlockGutter({
  block,
  emptyTextBlock,
  menuOpen,
  session,
  align,
  listType,
  onMenuOpenChange,
  onInsert,
  onDelete,
  onKind,
  onAlignment,
  onList,
  onLink,
  onInsertTable,
  onInsertImage,
  onInsertQuote,
  onInsertCallout,
  onInsertTodo,
  onInsertCode,
  onDivider,
}: BlockGutterProps) {
  return (
    <div className="block-row__gutter">
      <Popover
        open={menuOpen}
        onOpenChange={(open) => {
          onMenuOpenChange(open);
          if (open) session.setActiveBlock(block.id);
        }}
        placement="left-start"
        offset={8}
        role="presentation"
        popupClassName="oo-overlay--block-menu"
        content={(
          <BlockMenu
            block={block}
            onClose={() => onMenuOpenChange(false)}
            onInsert={onInsert}
            onDelete={onDelete}
            onKind={onKind}
            activeAlign={align}
            activeList={listType}
            onAlignment={onAlignment}
            onList={onList}
            onLink={onLink}
            onInsertTable={onInsertTable}
            onInsertImage={onInsertImage}
            onInsertQuote={onInsertQuote}
            onInsertCallout={onInsertCallout}
            onInsertTodo={onInsertTodo}
            onInsertCode={onInsertCode}
            onDivider={onDivider}
          />
        )}
      >
        <button
          className="block-row__handle"
          type="button"
          aria-label={emptyTextBlock ? "插入块" : "打开块菜单"}
          aria-haspopup="menu"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => session.setActiveBlock(block.id)}
        >
          {emptyTextBlock ? <Icon name="insert" /> : <Icon name="block-handle" />}
        </button>
      </Popover>
    </div>
  );
}
