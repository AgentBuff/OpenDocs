import { useCallback, useEffect, useState } from "react";

import { OpenOfficeSdk, isVersionConflict } from "@open-office/sdk";
import type { SpreadsheetModel } from "@open-office/schema/artifact";

const sdk = new OpenOfficeSdk();
const ACTOR_ID = "spreadsheet-web";

export type SpreadsheetTransactionOrigin = "local" | "undo" | "redo";

export interface SpreadsheetViewport {
  startRow: number;
  endRow: number;
  startColumn: number;
  endColumn: number;
}

export function message(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

/**
 * 幂等用户操作（重复设同样式、已排序再排序、空区域清除、重复冻结等）
 * 在引擎里天然 NoChanges，服务端以 400 回应。产品语义：无事发生 = 成功，
 * 不弹错误横幅、不刷界面。
 */
export function isNoChanges(reason: unknown): boolean {
  return message(reason).includes("没有产生 Spreadsheet 变更");
}

/**
 * Server-authoritative spreadsheet session.
 *
 * The browser owns no mutable grid model. It reads compact workbook/sheet
 * structure separately from bounded cell windows and submits named semantic
 * commands for writes. Commits advance the projection revision and refresh
 * metadata only; the grid then re-requests its visible window.
 */
export function useSpreadsheetSession(id: string) {
  const [model, setModel] = useState<SpreadsheetModel | null>(null);
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeSheetId, setActiveSheetId] = useState<string | null>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  /**
   * The server-owned command catalog for the `spreadsheet` namespace.
   *
   * The ribbon must not offer a control whose command the server cannot
   * dispatch. `GET /api/capabilities` is the only machine-readable source of
   * that set, so the UI is gated on it rather than on a hardcoded list that
   * would silently drift from the engine's registry.
   *
   * Fail-closed: an empty set hides the engine-backed controls instead of
   * offering buttons that always 400. The catalog is built in memory by the
   * server and shares the transport with the structure fetch, so a failure
   * here means the server is already unreachable and the studio is showing its
   * error page anyway.
   */
  const [availableCapabilities, setAvailableCapabilities] = useState<ReadonlySet<string>>(() => new Set());
  const [capabilitiesLoaded, setCapabilitiesLoaded] = useState(false);

  const refresh = useCallback(async (options?: { silent?: boolean; historyState?: { canUndo: boolean; canRedo: boolean } }) => {
    // 提交后的刷新走 silent：模型已在浏览器中，静默换数据即可。
    // 每次都置 loading 会把整个编辑器卸载成占位页，销毁所有面板状态。
    if (!options?.silent) setLoading(true);
    setError(null);
    try {
      const [structure, history] = await Promise.all([
        sdk.spreadsheetStructure(id),
        options?.historyState ? Promise.resolve(options.historyState) : sdk.history(id),
      ]);
      setModel(structure.data);
      setRevision(structure.revision);
      setActiveSheetId((current) => {
        if (current && structure.data.sheets.some((sheet) => sheet.id === current)) return current;
        return structure.data.metadata.activeSheetId ?? structure.data.sheets[0]?.id ?? null;
      });
      setCanUndo(history.canUndo);
      setCanRedo(history.canRedo);
      return structure;
    } catch (reason) {
      setError(message(reason));
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    void sdk.capabilities().then((catalog) => {
      if (disposed) return;
      const spreadsheet = catalog.artifacts.find((artifact) => artifact.kind === "spreadsheet");
      setAvailableCapabilities(new Set(spreadsheet?.commands.map((command) => command.typeId) ?? []));
      setCapabilitiesLoaded(true);
    }).catch((reason: unknown) => {
      if (disposed) return;
      // Surface the failure instead of silently rendering an ungated ribbon.
      setCapabilitiesLoaded(true);
      setError(message(reason));
    });
    return () => {
      disposed = true;
    };
  }, []);

  const applyHistoryState = useCallback(
    (result: { canUndo: boolean; canRedo: boolean }) => {
      setCanUndo(result.canUndo);
      setCanRedo(result.canRedo);
    },
    [],
  );

  const submit = useCallback(
    async (
      commands: Array<{ typeId: string; payload: Record<string, unknown> }>,
      origin: SpreadsheetTransactionOrigin = "local",
      options?: { retryOnConflict?: boolean },
    ): Promise<boolean> => {
      if (!model || commands.length === 0) return false;
      setSaving(true);
      setError(null);
      try {
        const result = await sdk.submit({
          artifactId: id,
          baseRevision: revision,
          actorId: ACTOR_ID,
          origin,
          commands,
        });
        applyHistoryState(result);
        setRevision(result.revision);
        await refresh({ silent: true, historyState: result });
        return true;
      } catch (reason) {
        if (isVersionConflict(reason)) {
          const latest = await refresh({ silent: true });
          if (options?.retryOnConflict && latest && latest.revision !== revision) {
            try {
              const retried = await sdk.submit({
                artifactId: id,
                baseRevision: latest.revision,
                actorId: ACTOR_ID,
                origin,
                commands,
              });
              applyHistoryState(retried);
              setRevision(retried.revision);
              await refresh({ silent: true, historyState: retried });
              return true;
            } catch (retryReason) {
              await refresh({ silent: true });
              setError(isVersionConflict(retryReason)
                ? "此表格在重试期间再次更新，已刷新，请重新粘贴。"
                : message(retryReason));
              return false;
            }
          }
          setError("此表格已由其他编辑者更新，已按最新 revision 刷新。");
        } else if (isNoChanges(reason)) {
          // 幂等操作：目标状态已达成，按成功处理。
          return true;
        } else {
          setError(message(reason));
        }
        return false;
      } finally {
        setSaving(false);
      }
    },
    [id, model, refresh, revision, applyHistoryState],
  );

  /**
   * Server-authoritative undo/redo. The command carries only the undo/redo
   * intent; the server replays the durable semantic command log against the
   * pre-transaction snapshot and returns the resulting revision.
   */
  const submitHistory = useCallback(
    async (action: "undo" | "redo"): Promise<boolean> => {
      if (!model) return false;
      setSaving(true);
      setError(null);
      try {
        const result = await sdk.submit({
          artifactId: id,
          baseRevision: revision,
          actorId: ACTOR_ID,
          origin: action,
          commands: [
            {
              typeId: "spreadsheet.history",
              payload: { action },
            },
          ],
        });
        applyHistoryState(result);
        setRevision(result.revision);
        await refresh({ silent: true, historyState: result });
        return true;
      } catch (reason) {
        if (isVersionConflict(reason)) {
          await refresh({ silent: true });
          setError("此表格已由其他编辑者更新，已按最新 revision 刷新。");
        } else if (isNoChanges(reason)) {
          return true;
        } else {
          setError(message(reason));
        }
        return false;
      } finally {
        setSaving(false);
      }
    },
    [id, model, refresh, revision, applyHistoryState],
  );

  const projectRange = useCallback(async (sheetId: string, viewport: SpreadsheetViewport) => {
    const requests = [];
    for (let startRow = viewport.startRow; startRow <= viewport.endRow; startRow += 1_000) {
      for (let startColumn = viewport.startColumn; startColumn <= viewport.endColumn; startColumn += 200) {
        requests.push(sdk.spreadsheet(id, {
          sheetId,
          startRow,
          endRow: Math.min(viewport.endRow + 1, startRow + 1_000),
          startColumn,
          endColumn: Math.min(viewport.endColumn + 1, startColumn + 200),
        }));
      }
    }
    const projections = await Promise.all(requests);
    if (projections.some((projection) => projection.revision !== revision)) {
      throw new Error("复制期间表格已更新，请重试");
    }
    return projections.flatMap((projection) => projection.data.cells);
  }, [id, revision]);

  return {
    model,
    revision,
    loading,
    saving,
    error,
    reportError: setError,
    activeSheetId,
    setActiveSheetId,
    refresh,
    submit,
    submitHistory,
    projectRange,
    canUndo,
    canRedo,
    availableCapabilities,
    capabilitiesLoaded,
  };
}
