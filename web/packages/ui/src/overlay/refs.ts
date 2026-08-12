import type { MutableRefObject, Ref, RefCallback } from "react";

export type MaybeRef<T> = MutableRefObject<T | null> | RefCallback<T> | null | undefined;

function assignRef<T>(ref: MaybeRef<T>, value: T | null): void {
  if (typeof ref === "function") ref(value);
  else if (ref) ref.current = value;
}

export function composeRefs<T>(...refs: Array<MaybeRef<T>>): RefCallback<T> {
  return (value) => refs.forEach((ref) => assignRef(ref, value));
}

export function readElementRef(ref: Ref<HTMLElement> | undefined): MaybeRef<HTMLElement> {
  return ref as MaybeRef<HTMLElement>;
}
