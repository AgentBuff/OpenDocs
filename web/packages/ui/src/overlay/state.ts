import { useCallback, useRef, useState } from "react";

export interface ControllableStateOptions<T> {
  value?: T;
  defaultValue: T;
  onChange?: (value: T) => void;
}

export function useControllableState<T>({ value, defaultValue, onChange }: ControllableStateOptions<T>): [T, (next: T | ((current: T) => T)) => void] {
  const [uncontrolled, setUncontrolled] = useState(defaultValue);
  const isControlled = value !== undefined;
  const current = (isControlled ? value : uncontrolled) as T;
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const set = useCallback((next: T | ((current: T) => T)) => {
    const resolved = typeof next === "function" ? (next as (current: T) => T)(current) : next;
    if (!isControlled) setUncontrolled(resolved);
    if (!Object.is(resolved, current)) onChangeRef.current?.(resolved);
  }, [current, isControlled]);

  return [current, set];
}
