import type { ArtifactPageSetup } from "@open-office/schema/artifact";
import { ToolbarSelect } from "@open-office/ui";

export const DEFAULT_PAGE_SETUP: ArtifactPageSetup = {
  width: 595.28,
  height: 841.89,
  marginTop: 72,
  marginRight: 72,
  marginBottom: 72,
  marginLeft: 72,
};

export function PageSetupPanel({
  value,
  onChange,
}: {
  value: ArtifactPageSetup;
  onChange: (patch: Partial<ArtifactPageSetup>) => void;
}) {
  const landscape = value.width > value.height;
  return (
    <div className="page-setup-panel" aria-label="页面设置">
      <div className="page-setup-panel__head">
        <strong>页面设置</strong>
      </div>
      <label>纸张
        <ToolbarSelect
          aria-label="纸张方向"
          className="page-setup-panel__select"
          value={landscape ? "landscape" : "portrait"}
          options={[
            { value: "portrait", label: "A4 纵向" },
            { value: "landscape", label: "A4 横向" },
          ]}
          onValueChange={(nextValue) => {
          const nextLandscape = nextValue === "landscape";
          if (nextLandscape !== landscape) onChange({ width: value.height, height: value.width });
        }}
        />
      </label>
      <div className="page-setup-panel__margins">
        {(["marginTop", "marginRight", "marginBottom", "marginLeft"] as const).map((key) => (
          <label key={key}>{marginLabel(key)}
            <input
              type="number"
              min="0"
              max="300"
              step="1"
              value={Math.round(value[key])}
              onChange={(event) => {
                const next = Number(event.target.value);
                if (Number.isFinite(next)) onChange({ [key]: Math.max(0, Math.min(300, next)) });
              }}
            />
          </label>
        ))}
      </div>
      <small>尺寸和边距单位：pt</small>
    </div>
  );
}

function marginLabel(key: "marginTop" | "marginRight" | "marginBottom" | "marginLeft"): string {
  return ({ marginTop: "上", marginRight: "右", marginBottom: "下", marginLeft: "左" })[key];
}
