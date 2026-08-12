import type {
  ArtifactCommandEnvelope,
  DocumentCommand,
  PendingTransaction,
} from "@open-office/schema/artifact";

/** A queued local transaction together with the commands required for local replay. */
export type AutosaveTransaction = PendingTransaction & {
  commands: DocumentCommand[];
  /** Identity for an unsent local edit stream; never persisted or replayed. */
  coalesceKey: string | null;
  state: OutboxTransactionState;
  attempts: number;
  nextAttemptAt: number | null;
  lastError: string | null;
};

/** The delivery state is intentionally explicit so UI and lifecycle callers can distinguish
 * an item that is waiting, in flight, retryable, or permanently blocked. */
export type OutboxTransactionState = "pending" | "sending" | "retrying" | "failed" | "acknowledged";

export interface OutboxRetryResult {
  state: OutboxTransactionState;
  attempts: number;
  nextAttemptAt: number | null;
}

export interface AutosaveOutboxOptions {
  maxAttempts?: number;
  retryBaseDelayMs?: number;
  retryMaxDelayMs?: number;
}

export interface EnqueueTransactionInput {
  artifactId: string;
  baseRevision: number;
  sequence: number;
  commands: DocumentCommand[];
  /** Consecutive pending edits with the same key replace the unsent draft. */
  coalesceKey?: string | null;
  actorId?: string;
}

/**
 * The persistence boundary for local edits.
 *
 * The outbox deliberately has no timers, React state, or HTTP knowledge. It owns ordering and
 * revision retargeting only, so autosave can be driven by a browser lifecycle, a manual save, or
 * a future offline worker without duplicating queue semantics.
 */
export class AutosaveOutbox {
  private readonly transactions: AutosaveTransaction[] = [];
  private readonly maxAttempts: number;
  private readonly retryBaseDelayMs: number;
  private readonly retryMaxDelayMs: number;

  constructor(options: AutosaveOutboxOptions = {}) {
    this.maxAttempts = options.maxAttempts ?? 5;
    this.retryBaseDelayMs = options.retryBaseDelayMs ?? 250;
    this.retryMaxDelayMs = options.retryMaxDelayMs ?? 8_000;
    if (!Number.isSafeInteger(this.maxAttempts) || this.maxAttempts < 1) {
      throw new Error("AutosaveOutbox maxAttempts 必须是正整数");
    }
    if (!Number.isFinite(this.retryBaseDelayMs) || this.retryBaseDelayMs < 0) {
      throw new Error("AutosaveOutbox retryBaseDelayMs 必须是非负数");
    }
    if (!Number.isFinite(this.retryMaxDelayMs) || this.retryMaxDelayMs < this.retryBaseDelayMs) {
      throw new Error("AutosaveOutbox retryMaxDelayMs 不能小于 retryBaseDelayMs");
    }
  }

  get size(): number {
    return this.transactions.length;
  }

  get isEmpty(): boolean {
    return this.transactions.length === 0;
  }

  peek(): AutosaveTransaction | null {
    return this.transactions[0] ?? null;
  }

  /** Return the head only when it is eligible for delivery at `now`. */
  peekReady(now = Date.now()): AutosaveTransaction | null {
    const transaction = this.peek();
    if (!transaction || transaction.state === "sending" || transaction.state === "failed") return null;
    if (transaction.nextAttemptAt !== null && transaction.nextAttemptAt > now) return null;
    return transaction;
  }

  get blocked(): boolean {
    return this.transactions[0]?.state === "failed";
  }

  /** Milliseconds until the head can be attempted; null means no queued work. */
  nextAttemptDelay(now = Date.now()): number | null {
    const transaction = this.peek();
    if (!transaction || transaction.state === "failed" || transaction.state === "sending") return null;
    return transaction.nextAttemptAt === null ? 0 : Math.max(0, transaction.nextAttemptAt - now);
  }

  entries(): AutosaveTransaction[] {
    return this.transactions.map((transaction) => ({
      ...transaction,
      envelope: { ...transaction.envelope, commands: transaction.envelope.commands.map((command) => ({ ...command })) },
      commands: [...transaction.commands],
    }));
  }

  enqueue(input: EnqueueTransactionInput): AutosaveTransaction {
    const coalesceKey = input.coalesceKey ?? null;
    const tail = this.transactions.at(-1);
    if (coalesceKey && tail?.state === "pending" && tail.coalesceKey === coalesceKey) {
      // This transaction has not crossed the network. Preserve its revision,
      // queue position and idempotency id while replacing only the draft.
      tail.commands = [...input.commands];
      tail.envelope = { ...tail.envelope, commands: commandRecords(input.commands) };
      return tail;
    }
    const transaction = createAutosaveTransaction(input, this.transactions.length);
    this.transactions.push(transaction);
    return transaction;
  }

  /** Claim the head for one network attempt. The transaction id remains unchanged for idempotent retry. */
  beginAttempt(transactionId: string, now = Date.now()): AutosaveTransaction | null {
    const transaction = this.peekReady(now);
    if (!transaction || transaction.envelope.transactionId !== transactionId) return null;
    transaction.state = "sending";
    transaction.attempts += 1;
    transaction.nextAttemptAt = null;
    return transaction;
  }

  markRetry(transactionId: string, error: unknown, now = Date.now()): OutboxRetryResult | null {
    const transaction = this.findHead(transactionId);
    if (!transaction || transaction.state !== "sending") return null;
    const message = error instanceof Error ? error.message : String(error);
    transaction.lastError = message;
    if (transaction.attempts >= this.maxAttempts) {
      transaction.state = "failed";
      transaction.nextAttemptAt = null;
      return snapshotRetryState(transaction);
    }
    const exponent = Math.max(0, transaction.attempts - 1);
    const delay = Math.min(this.retryMaxDelayMs, this.retryBaseDelayMs * 2 ** exponent);
    transaction.state = "retrying";
    transaction.nextAttemptAt = now + delay;
    return snapshotRetryState(transaction);
  }

  /** Reset a failed/retrying item for an explicit user or conflict-resolution retry. */
  retry(transactionId: string, now = Date.now()): boolean {
    const transaction = this.findHead(transactionId);
    if (!transaction || transaction.state === "acknowledged") return false;
    transaction.state = "pending";
    transaction.nextAttemptAt = now;
    transaction.lastError = null;
    return true;
  }

  markFailed(transactionId: string, error: unknown): boolean {
    const transaction = this.findHead(transactionId);
    if (!transaction) return false;
    transaction.state = "failed";
    transaction.nextAttemptAt = null;
    transaction.lastError = error instanceof Error ? error.message : String(error);
    return true;
  }

  acknowledge(transactionId: string): AutosaveTransaction | null {
    const transaction = this.findHead(transactionId);
    if (!transaction || transaction.state !== "sending") {
      return null;
    }
    transaction.state = "acknowledged";
    transaction.nextAttemptAt = null;
    transaction.lastError = null;
    return this.transactions.shift() ?? null;
  }

  retarget(baseRevision: number): void {
    this.transactions.forEach((transaction, index) => {
      transaction.envelope = {
        ...transaction.envelope,
        baseRevision: baseRevision + index,
      };
    });
  }

  clear(): void {
    this.transactions.length = 0;
  }

  private findHead(transactionId: string): AutosaveTransaction | null {
    const transaction = this.transactions[0] ?? null;
    return transaction?.envelope.transactionId === transactionId ? transaction : null;
  }
}

function createAutosaveTransaction(
  { artifactId, baseRevision, sequence, commands, coalesceKey = null, actorId = "dev-user" }: EnqueueTransactionInput,
  pendingCount: number,
): AutosaveTransaction {
  const transactionId = randomId();
  const envelope: ArtifactCommandEnvelope = {
    protocolVersion: 1,
    transactionId,
    intentId: randomId(),
    artifactId,
    actorId,
    baseRevision: baseRevision + pendingCount,
    origin: "local",
    commands: commandRecords(commands),
  };
  return {
    sequence,
    envelope,
    commands: [...commands],
    coalesceKey,
    state: "pending",
    attempts: 0,
    nextAttemptAt: null,
    lastError: null,
  };
}

function commandRecords(commands: DocumentCommand[]): ArtifactCommandEnvelope["commands"] {
  return commands.map((payload) => ({
    commandId: randomId(),
    typeId: `document.${payload.type}`,
    payload,
  }));
}

function snapshotRetryState(transaction: AutosaveTransaction): OutboxRetryResult {
  return {
    state: transaction.state,
    attempts: transaction.attempts,
    nextAttemptAt: transaction.nextAttemptAt,
  };
}


function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
