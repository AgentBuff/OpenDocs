/** Preserve styles around a plain-text edit; offsets are Unicode scalar indices. */
export function preserveTextRuns<S, B extends { text: string; runs: Array<{ start: number; end: number; style: S }> }>(body: B, text: string): B {
  if (!body.runs.length || !text) return { ...body, text, runs: [] };
  const before = [...body.text], after = [...text];
  let start = 0, suffix = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  while (suffix < before.length - start && suffix < after.length - start && before[before.length - suffix - 1] === after[after.length - suffix - 1]) suffix++;
  const oldEnd = before.length - suffix, newEnd = after.length - suffix;
  const runs: typeof body.runs = [];
  const append = (start: number, end: number, style: S) => {
    if (end <= start) return;
    const last = runs.at(-1);
    if (last && last.end === start && JSON.stringify(last.style) === JSON.stringify(style)) last.end = end;
    else runs.push({ start, end, style });
  };
  for (const run of body.runs) append(run.start, Math.min(run.end, start), run.style);
  const inherited = body.runs.find(run => run.start <= Math.max(0, start - 1) && run.end > Math.max(0, start - 1)) ?? body.runs[0]!;
  append(start, newEnd, inherited.style);
  for (const run of body.runs) if (run.end > oldEnd) append(Math.max(run.start, oldEnd) + newEnd - oldEnd, run.end + newEnd - oldEnd, run.style);
  return { ...body, text, runs };
}
