import { useState, useRef, useCallback } from "react";

export function useTimer() {
  const [elapsed, setElapsed] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const startTimer = useCallback(
    (initialSecs: number) => {
      clearTimer();
      setElapsed(initialSecs);
      timerRef.current = setInterval(() => {
        setElapsed((prev) => prev + 1);
      }, 1000);
    },
    [clearTimer],
  );

  return { elapsed, startTimer, clearTimer };
}
