import type { ComponentType, KeyboardEvent, RefObject } from "react";

import type { DocumentBlock, DocumentBlockKind } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";

/** Renderer slot names are stable product capabilities, not DOM component names. */
export type BlockRendererKey = "content" | "divider" | "table" | "image" | "code";

export interface BlockRendererProps {
  block: DocumentBlock;
  session: BlockSessionApi;
  /** Product selection is provided by BlockNode; renderers never infer it from DOM focus. */
  selected: boolean;
  contentRef?: RefObject<HTMLDivElement>;
  empty: boolean;
  align?: "left" | "center" | "right" | "justify";
  lineHeight?: number;
  placeholder: string;
  onInput: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
}

export type BlockRenderer = ComponentType<BlockRendererProps>;

/**
 * Command/menu contracts belong to the block runtime, not to a particular
 * toolbar or popup component.  A future table/image/code block can provide
 * capabilities without growing a central `kind` switch in BlockEditor.
 */
export interface BlockCommandContext {
  block: DocumentBlock;
  session: BlockSessionApi;
}

export interface BlockCommandCapability {
  id: string;
  title: string;
  isEnabled?: (context: BlockCommandContext) => boolean;
  execute: (context: BlockCommandContext) => void;
}

export interface BlockMenuCapability {
  id: string;
  title: string;
  commands: readonly BlockCommandCapability[];
}

export interface BlockDefinition {
  key: BlockRendererKey;
  matches: (block: DocumentBlock) => boolean;
  renderer: BlockRenderer;
  fallback?: boolean;
  /** View metadata is owned by the definition so BlockEditor only composes it. */
  className?: (block: DocumentBlock) => string;
  placeholder?: (block: DocumentBlock) => string;
  menu?: readonly BlockMenuCapability[];
  commands?: readonly BlockCommandCapability[];
}

export interface BlockRegistry {
  resolve: (block: DocumentBlock) => BlockDefinition;
  get: (key: BlockRendererKey) => BlockDefinition | undefined;
}

/**
 * Create one runtime registry per editor session.  No module-global Map means
 * embedded editors cannot leak plugin definitions into one another, while
 * definitions remain declarative and easy to test in isolation.
 */
export function createBlockRegistry(definitions: readonly BlockDefinition[]): BlockRegistry {
  const byKey = new Map<BlockRendererKey, BlockDefinition>();
  for (const definition of definitions) {
    if (byKey.has(definition.key)) {
      throw new Error(`duplicate block definition: ${definition.key}`);
    }
    byKey.set(definition.key, definition);
  }
  if (definitions.filter((definition) => definition.fallback).length !== 1) {
    throw new Error("block registry requires a fallback definition");
  }
  return {
    resolve(block) {
      const definition = definitions.find((candidate) => candidate.matches(block));
      if (!definition) throw new Error(`no renderer for block: ${block.kind.type}`);
      return definition;
    },
    get(key) {
      return byKey.get(key);
    },
  };
}

/** Shared view metadata for the content renderer; renderer selection remains data-driven. */
export function contentClassName(kind: DocumentBlockKind): string {
  if (kind.type === "heading" && typeof kind.level === "number") return `heading-${kind.level}`;
  return kind.type;
}

export function contentPlaceholder(kind: DocumentBlockKind): string {
  if (kind.type === "heading" && typeof kind.level === "number") return `标题 ${kind.level}`;
  if (kind.type === "quote") return "引用";
  if (kind.type === "callout") return "输入高亮内容…";
  if (kind.type === "todo") return "输入待办事项…";
  if (kind.type === "code") return "输入代码…";
  if (kind.type === "link") return "输入链接标题…";
  return "输入内容，或按 Enter 创建下一个块";
}
