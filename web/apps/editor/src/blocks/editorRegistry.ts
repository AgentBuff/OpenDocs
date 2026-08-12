import type { BlockCommandContext, BlockRegistry } from "./registry.js";
import { contentClassName, contentPlaceholder, createBlockRegistry } from "./registry.js";
import { ContentBlockRenderer, CodeBlockRenderer, DividerBlockRenderer, ImageBlockRenderer, TableBlockRenderer } from "./renderers.js";

export function createEditorBlockRegistry(): BlockRegistry {
  const deleteCommand = {
    id: "delete",
    title: "删除",
    execute: ({ block, session }: BlockCommandContext) => {
      session.deleteBlock(block.id);
    },
  };
  return createBlockRegistry([
    {
      key: "divider",
      matches: (block) => block.kind.type === "divider",
      renderer: DividerBlockRenderer,
      className: () => "divider",
      placeholder: () => "分割线",
      commands: [deleteCommand],
    },
    {
      key: "code",
      matches: (block) => block.kind.type === "code",
      renderer: CodeBlockRenderer,
      className: () => "code",
      placeholder: () => "输入代码…",
      commands: [deleteCommand],
    },
    {
      key: "table",
      matches: (block) => block.kind.type === "table" && block.data.type === "table",
      renderer: TableBlockRenderer,
      className: () => "table",
      placeholder: () => "表格",
      commands: [deleteCommand],
    },
    {
      key: "image",
      matches: (block) => block.kind.type === "image" && block.data.type === "image",
      renderer: ImageBlockRenderer,
      className: () => "image",
      placeholder: () => "图片",
      commands: [deleteCommand],
    },
    {
      key: "content",
      fallback: true,
      matches: () => true,
      renderer: ContentBlockRenderer,
      className: (block) => contentClassName(block.kind),
      placeholder: (block) => contentPlaceholder(block.kind),
      commands: [deleteCommand],
    },
  ]);
}
