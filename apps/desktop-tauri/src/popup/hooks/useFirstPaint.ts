import { useEffect, useRef } from "react";

// Phase 3 D6: returns true on the very first render of a component
// and false on every subsequent render. We use it to suppress the
// initial bar tween so the popup does not animate from 0% on every
// open. After the first commit, real data updates flip it to false
// and CSS transitions take over.

export function useFirstPaint(): boolean {
  const seenRef = useRef(false);
  const value = !seenRef.current;
  useEffect(() => {
    seenRef.current = true;
  }, []);
  return value;
}
