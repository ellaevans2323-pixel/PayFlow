import { useCallback, useMemo, useState } from "react";

type AnalyticsEvent = "wallet_connect" | "subscribe" | "cancel" | "pay_per_use";

const ANALYTICS_OPT_IN_KEY = "flowpay_analytics_opt_in";

function readOptInPreference(): boolean {
  if (typeof window === "undefined") return false;
  return window.localStorage.getItem(ANALYTICS_OPT_IN_KEY) === "true";
}

export function useAnalytics() {
  const [isOptedIn, setIsOptedIn] = useState<boolean>(() => readOptInPreference());

  const setOptIn = useCallback((enabled: boolean) => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(ANALYTICS_OPT_IN_KEY, String(enabled));
    }
    setIsOptedIn(enabled);
  }, []);

  const track = useCallback(
    (event: AnalyticsEvent, metadata?: Record<string, string | number | boolean>) => {
      if (!isOptedIn || typeof window === "undefined") return;

      const payload = {
        event,
        metadata: metadata ?? {},
        timestamp: new Date().toISOString(),
      };

      // Keep this privacy-first: local event only, no automatic network transport.
      window.dispatchEvent(new CustomEvent("flowpay-analytics", { detail: payload }));
    },
    [isOptedIn]
  );

  return useMemo(
    () => ({
      isOptedIn,
      setOptIn,
      track,
    }),
    [isOptedIn, setOptIn, track]
  );
}
