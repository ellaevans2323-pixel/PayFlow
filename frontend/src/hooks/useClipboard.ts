// useClipboard: writes text to the system clipboard and exposes a timed `copied` flag (#56)
import { useState, useCallback } from "react";

export function useClipboard(timeout = 2000) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState(false);

  const copy = useCallback(
    async (text: string) => {
      try {
        setError(false);
        await navigator.clipboard.writeText(text);
        setCopied(true);
        setTimeout(() => setCopied(false), timeout);
      } catch {
        setError(true);
        setTimeout(() => setError(false), timeout);
      }
    },
    [timeout],
  );

  return { copied, error, copy };
}
