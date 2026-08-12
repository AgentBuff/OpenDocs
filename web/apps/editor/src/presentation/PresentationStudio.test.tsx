import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { PresentationV5Node } from "@open-office/schema";

import { SlideNode } from "./PresentationStudio.js";

const image: PresentationV5Node = {
  id: "image-1",
  name: "产品封面",
  altText: "产品封面",
  parentId: null,
  orderKey: "a",
  layoutPlaceholderId: null,
  transform: { x: 0, y: 0, width: 320_000, height: 180_000, rotation: 0 },
  visible: true,
  locked: false,
  opacity: 1,
  kind: {
    type: "image",
    data: {
      assetId: "asset-current",
      originalAssetId: "asset-original",
      crop: { top: 0.1, right: 0.2, bottom: 0.1, left: 0.2 },
      flipH: true,
      flipV: false,
      caption: "产品封面",
    },
  },
};

describe("PresentationStudio node rendering smoke", () => {
  it("keeps image rendering on the immutable asset endpoint and exposes selection feedback", () => {
    const html = renderToStaticMarkup(
      <SlideNode
        artifactId="presentation-1"
        node={image}
        transform={image.transform}
        scale={1}
        editing={false}
        adornments={[{
          kind: "outline",
          node: { slideId: "slide-1", nodeId: "image-1" },
          bounds: image.transform,
          handles: ["southEast"],
        }]}
        unsupportedReason={null}
        onSelect={() => undefined}
        onEdit={() => undefined}
        onPointerDown={() => undefined}
        onTextSave={() => undefined}
      />,
    );

    expect(html).toContain("presentation-studio__node--hit-target is-selected");
    expect(html).toContain('data-node-id="image-1"');
    expect(html).toContain('src="/api/artifacts/presentation-1/assets/asset-current"');
    expect(html).toContain('aria-label="调整对象大小"');
    expect(html).toContain('alt="产品封面"');
    expect(html).toContain("scale(-1, 1)");
  });

  it("does not show a resize control for an unselected image", () => {
    const html = renderToStaticMarkup(
      <SlideNode
        artifactId="presentation-1"
        node={image}
        transform={image.transform}
        scale={1}
        editing={false}
        adornments={[]}
        unsupportedReason={null}
        onSelect={() => undefined}
        onEdit={() => undefined}
        onPointerDown={() => undefined}
        onTextSave={() => undefined}
      />,
    );
    expect(html).not.toContain('aria-label="调整对象大小"');
    expect(html).not.toContain("is-selected");
  });
});
