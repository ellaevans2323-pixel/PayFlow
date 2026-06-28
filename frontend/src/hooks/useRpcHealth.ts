import { useState, useEffect } from "react";
import { server } from "../stellar";

export type RpcStatus = "healthy" | "degraded" | "unreachable";

interface UseRpcHealthResult {
  status: RpcStatus;
  latencyMs: number | null;
  error: string | null;
}

export function useRpcHealth(): UseRpcHealthResult {
  const [status, setStatus] = useState<RpcStatus>("healthy");
  const [latencyMs, setLatencyMs] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;
    let timerId: ReturnType<typeof setTimeout> | null = null;
    let currentDelay = 2000;

    async function checkHealth() {
      const startTime = performance.now();
      try {
        await server.getHealth();
        if (!isMounted) return;

        const endTime = performance.now();
        const latency = Math.round(endTime - startTime);

        setLatencyMs(latency);
        setStatus(latency > 2000 ? "degraded" : "healthy");
        setError(null);

        // Reset backoff sequence
        currentDelay = 2000;

        // Schedule next check in 60 seconds
        timerId = setTimeout(checkHealth, 60000);
      } catch (e: unknown) {
        if (!isMounted) return;

        setStatus("unreachable");
        setError(e instanceof Error ? e.message : "RPC endpoint unreachable");
        setLatencyMs(null);

        const delayToUse = currentDelay;
        // Capped at 30 seconds
        currentDelay = Math.min(currentDelay * 2, 30000);

        // Schedule retry
        timerId = setTimeout(checkHealth, delayToUse);
      }
    }

    checkHealth();

    return () => {
      isMounted = false;
      if (timerId) {
        clearTimeout(timerId);
      }
    };
  }, []);

  return { status, latencyMs, error };
}

