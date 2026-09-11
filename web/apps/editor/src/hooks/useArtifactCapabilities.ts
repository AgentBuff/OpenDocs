import { useEffect, useState } from "react";

import { OpenOfficeSdk } from "@open-office/sdk";
import type { ArtifactKind } from "@open-office/schema/artifact";

/**
 * 能力目录是进程级的只读发现文档，不需要每个 Studio 各建一个客户端。
 * 每个模块自建 `new OpenOfficeSdk()` 会让同一份静态文档被重复拉取。
 */
const sdk = new OpenOfficeSdk();

export interface ArtifactCapabilities {
  /** 服务端 `/api/capabilities` 为该 kind 发布的命令 typeId 集合。 */
  availableCapabilities: ReadonlySet<string>;
  /** 目录是否已经问过——成功或失败都算，避免界面永久停在加载态。 */
  capabilitiesLoaded: boolean;
  /** 读取失败的原因；`null` 表示成功。 */
  capabilitiesError: string | null;
}

/**
 * 读取某个 Artifact kind 的服务端能力目录。
 *
 * 这是 UI 门控的唯一输入：只有服务端声明可调度的命令才允许渲染成可用控件。
 * 失败时返回空集合（fail-closed），并把原因交给调用方展示——静默放行会让用户
 * 以为某个操作可用，实际只会收到 400。
 */
export function useArtifactCapabilities(kind: ArtifactKind): ArtifactCapabilities {
  const [availableCapabilities, setAvailableCapabilities] = useState<ReadonlySet<string>>(() => new Set());
  const [capabilitiesLoaded, setCapabilitiesLoaded] = useState(false);
  const [capabilitiesError, setCapabilitiesError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    void sdk.capabilities().then((catalog) => {
      if (disposed) return;
      const artifact = catalog.artifacts.find((entry) => entry.kind === kind);
      setAvailableCapabilities(new Set(artifact?.commands.map((command) => command.typeId) ?? []));
      setCapabilitiesLoaded(true);
    }).catch((reason: unknown) => {
      if (disposed) return;
      setCapabilitiesError(reason instanceof Error ? reason.message : String(reason));
      setCapabilitiesLoaded(true);
    });
    return () => {
      disposed = true;
    };
  }, [kind]);

  return { availableCapabilities, capabilitiesLoaded, capabilitiesError };
}
